//! Read-only GPT inspection of a regular host-side disk-image file.
//!
//! This is diagnostic tooling. It never provisions, repairs or writes a disk.
use std::env;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

const MAX_ENTRY_ARRAY_BYTES: u64 = 16 * 1024 * 1024;
const GPT_HEADER_MIN_BYTES: usize = 92;
const GPT_ENTRY_MIN_BYTES: u32 = 128;
const GPT_REVISION_1_0: u32 = 0x0001_0000;
const ESP_TYPE_GUID: [u8; 16] = [
    0x28, 0x73, 0x2a, 0xc1, 0x1f, 0xf8, 0xd2, 0x11, 0xba, 0x4b, 0x00, 0xa0, 0xc9, 0x3e, 0xc9, 0x3b,
];

#[derive(Debug, PartialEq, Eq)]
struct Report {
    sector_size: u64,
    last_lba: u64,
    partitions: usize,
    efi_system_partitions: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Header {
    current_lba: u64,
    alternate_lba: u64,
    first_usable: u64,
    last_usable: u64,
    disk_guid: [u8; 16],
    entry_lba: u64,
    entry_count: u32,
    entry_size: u32,
    entry_crc: u32,
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("checked input"))
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().expect("checked input"))
}

/// Reflected IEEE CRC-32, as used by UEFI GPT.
fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ if crc & 1 != 0 { 0xedb8_8320 } else { 0 };
        }
    }
    !crc
}

fn read_header<R: Read + Seek>(
    image: &mut R,
    sector_size: u64,
    lba: u64,
    label: &str,
) -> Result<Header, String> {
    let sector_len = usize::try_from(sector_size).map_err(|e| e.to_string())?;
    let offset = lba
        .checked_mul(sector_size)
        .ok_or_else(|| format!("{label} GPT header offset overflow"))?;
    let mut raw = vec![0u8; sector_len];
    image
        .seek(SeekFrom::Start(offset))
        .map_err(|e| e.to_string())?;
    image.read_exact(&mut raw).map_err(|e| e.to_string())?;
    if &raw[..8] != b"EFI PART" {
        return Err(format!("missing {label} GPT signature"));
    }
    let header_size = usize::try_from(read_u32(&raw, 12)).map_err(|e| e.to_string())?;
    if !(GPT_HEADER_MIN_BYTES..=sector_len).contains(&header_size) {
        return Err(format!("invalid {label} GPT header size"));
    }
    let expected_header_crc = read_u32(&raw, 16);
    raw[16..20].fill(0);
    if crc32(&raw[..header_size]) != expected_header_crc {
        return Err(format!("{label} GPT header CRC mismatch"));
    }
    if read_u32(&raw, 8) != GPT_REVISION_1_0 || read_u32(&raw, 20) != 0 {
        return Err(format!(
            "unsupported {label} GPT revision or nonzero reserved bytes"
        ));
    }
    let mut disk_guid = [0u8; 16];
    disk_guid.copy_from_slice(&raw[56..72]);
    Ok(Header {
        current_lba: read_u64(&raw, 24),
        alternate_lba: read_u64(&raw, 32),
        first_usable: read_u64(&raw, 40),
        last_usable: read_u64(&raw, 48),
        disk_guid,
        entry_lba: read_u64(&raw, 72),
        entry_count: read_u32(&raw, 80),
        entry_size: read_u32(&raw, 84),
        entry_crc: read_u32(&raw, 88),
    })
}

fn entry_array_len(header: &Header, label: &str) -> Result<u64, String> {
    if header.entry_count == 0
        || header.entry_size < GPT_ENTRY_MIN_BYTES
        || header.entry_size % 8 != 0
    {
        return Err(format!("invalid {label} GPT partition-entry layout"));
    }
    let len = u64::from(header.entry_count)
        .checked_mul(u64::from(header.entry_size))
        .ok_or_else(|| format!("{label} GPT entry array overflow"))?;
    if len > MAX_ENTRY_ARRAY_BYTES {
        return Err(format!(
            "{label} GPT entry array exceeds the 16 MiB inspection limit"
        ));
    }
    Ok(len)
}

fn read_entry_array<R: Read + Seek>(
    image: &mut R,
    image_len: u64,
    sector_size: u64,
    header: &Header,
    label: &str,
) -> Result<Vec<u8>, String> {
    let array_len = entry_array_len(header, label)?;
    let start = header
        .entry_lba
        .checked_mul(sector_size)
        .ok_or_else(|| format!("{label} GPT entry offset overflow"))?;
    let end = start
        .checked_add(array_len)
        .ok_or_else(|| format!("{label} GPT entry array end overflow"))?;
    if end > image_len {
        return Err(format!("{label} GPT entry array exceeds image"));
    }
    let size = usize::try_from(array_len).map_err(|e| e.to_string())?;
    let mut entries = vec![0u8; size];
    image
        .seek(SeekFrom::Start(start))
        .map_err(|e| e.to_string())?;
    image.read_exact(&mut entries).map_err(|e| e.to_string())?;
    if crc32(&entries) != header.entry_crc {
        return Err(format!("{label} GPT partition-entry array CRC mismatch"));
    }
    Ok(entries)
}

fn inspect<R: Read + Seek>(
    image: &mut R,
    image_len: u64,
    sector_size: u64,
) -> Result<Report, String> {
    if !matches!(sector_size, 512 | 4096) {
        return Err("sector size must be 512 or 4096".into());
    }
    if image_len < sector_size * 6 || image_len % sector_size != 0 {
        return Err("image is too small or not sector aligned".into());
    }
    let last_lba = image_len / sector_size - 1;

    let mut mbr = [0u8; 512];
    image.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    image.read_exact(&mut mbr).map_err(|e| e.to_string())?;
    if mbr[510..512] != [0x55, 0xaa] || mbr[450] != 0xee {
        return Err("missing protective MBR signature/type 0xEE".into());
    }

    let primary = read_header(image, sector_size, 1, "primary")?;
    let backup = read_header(image, sector_size, last_lba, "backup")?;

    if primary.current_lba != 1
        || primary.alternate_lba != last_lba
        || backup.current_lba != last_lba
        || backup.alternate_lba != 1
    {
        return Err("invalid reciprocal primary/backup GPT header locations".into());
    }
    if primary.first_usable < 2
        || primary.first_usable > primary.last_usable
        || primary.last_usable >= last_lba
    {
        return Err("invalid GPT usable-LBA range".into());
    }
    if primary.first_usable != backup.first_usable
        || primary.last_usable != backup.last_usable
        || primary.disk_guid != backup.disk_guid
        || primary.entry_count != backup.entry_count
        || primary.entry_size != backup.entry_size
        || primary.entry_crc != backup.entry_crc
    {
        return Err("primary/backup GPT metadata mismatch".into());
    }

    let primary_len = entry_array_len(&primary, "primary")?;
    let primary_start = primary
        .entry_lba
        .checked_mul(sector_size)
        .ok_or("primary GPT entry offset overflow")?;
    let primary_end = primary_start
        .checked_add(primary_len)
        .ok_or("primary GPT entry array end overflow")?;
    let first_usable_byte = primary
        .first_usable
        .checked_mul(sector_size)
        .ok_or("GPT usable range overflow")?;
    if primary.entry_lba < 2 || primary_end > first_usable_byte || primary_end > image_len {
        return Err("primary GPT entry array overlaps usable space or exceeds image".into());
    }

    let backup_len = entry_array_len(&backup, "backup")?;
    let backup_start = backup
        .entry_lba
        .checked_mul(sector_size)
        .ok_or("backup GPT entry offset overflow")?;
    let backup_end = backup_start
        .checked_add(backup_len)
        .ok_or("backup GPT entry array end overflow")?;
    let last_usable_end = primary
        .last_usable
        .checked_add(1)
        .and_then(|lba| lba.checked_mul(sector_size))
        .ok_or("GPT usable range overflow")?;
    let backup_header_start = last_lba
        .checked_mul(sector_size)
        .ok_or("backup GPT header offset overflow")?;
    if backup.entry_lba <= primary.last_usable
        || backup_start < last_usable_end
        || backup_end > backup_header_start
    {
        return Err("backup GPT entry array overlaps usable space or backup header".into());
    }

    let primary_entries = read_entry_array(image, image_len, sector_size, &primary, "primary")?;
    let backup_entries = read_entry_array(image, image_len, sector_size, &backup, "backup")?;
    if primary_entries != backup_entries {
        return Err("primary/backup GPT partition-entry arrays differ".into());
    }

    let mut partitions = 0;
    let mut efi_system_partitions = 0;
    let mut used_ranges = Vec::new();
    for entry in primary_entries.chunks_exact(primary.entry_size as usize) {
        if entry[..16].iter().all(|&byte| byte == 0) {
            continue;
        }
        if entry[16..32].iter().all(|&byte| byte == 0) {
            return Err("used GPT partition has an empty unique GUID".into());
        }
        let first = read_u64(entry, 32);
        let last = read_u64(entry, 40);
        if first < primary.first_usable || first > last || last > primary.last_usable {
            return Err("partition LBA range outside GPT usable space".into());
        }
        used_ranges.push((first, last));
        partitions += 1;
        if entry[..16] == ESP_TYPE_GUID {
            efi_system_partitions += 1;
        }
    }
    used_ranges.sort_unstable();
    if used_ranges.windows(2).any(|pair| pair[0].1 >= pair[1].0) {
        return Err("overlapping GPT partitions".into());
    }
    Ok(Report {
        sector_size,
        last_lba,
        partitions,
        efi_system_partitions,
    })
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if !(2..=3).contains(&args.len()) {
        eprintln!("usage: inspect-gpt <regular-image-file> [512|4096]");
        std::process::exit(2);
    }
    let sector_size = match args.get(2).map(String::as_str).unwrap_or("512") {
        "512" => 512,
        "4096" => 4096,
        _ => {
            eprintln!("sector size must be 512 or 4096");
            std::process::exit(2);
        }
    };
    let mut file = File::open(&args[1]).unwrap_or_else(|err| {
        eprintln!("cannot open image read-only: {err}");
        std::process::exit(1)
    });
    let metadata = file.metadata().unwrap_or_else(|err| {
        eprintln!("cannot inspect image metadata: {err}");
        std::process::exit(1)
    });
    if !metadata.is_file() {
        eprintln!("refusing non-regular file (including block devices)");
        std::process::exit(1);
    }
    match inspect(&mut file, metadata.len(), sector_size) {
        Ok(report) => println!(
            "GPT primary+backup headers/entries valid; sector_size={} last_lba={} partitions={} efi_system_partitions={}",
            report.sector_size, report.last_lba, report.partitions, report.efi_system_partitions
        ),
        Err(err) => {
            eprintln!("GPT inspection failed: {err}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn header_crc(header_sector: &mut [u8]) {
        header_sector[16..20].fill(0);
        let size = read_u32(header_sector, 12) as usize;
        let crc = crc32(&header_sector[..size]);
        header_sector[16..20].copy_from_slice(&crc.to_le_bytes());
    }

    fn synthetic_gpt(sector_size: usize) -> Vec<u8> {
        let mut image = vec![0u8; sector_size * 128];
        image[510..512].copy_from_slice(&[0x55, 0xaa]);
        image[450] = 0xee;

        let primary_entries_start = sector_size * 2;
        let backup_entries_start = sector_size * 126;
        let primary_entry = &mut image[primary_entries_start..primary_entries_start + 128];
        primary_entry[..16].copy_from_slice(&ESP_TYPE_GUID);
        primary_entry[16..32].copy_from_slice(&[0x42; 16]);
        primary_entry[32..40].copy_from_slice(&40u64.to_le_bytes());
        primary_entry[40..48].copy_from_slice(&70u64.to_le_bytes());
        let primary_entries =
            image[primary_entries_start..primary_entries_start + 512].to_vec();
        image[backup_entries_start..backup_entries_start + 512].copy_from_slice(&primary_entries);
        let array_crc = crc32(&primary_entries);

        let primary = &mut image[sector_size..sector_size * 2];
        primary[..8].copy_from_slice(b"EFI PART");
        primary[8..12].copy_from_slice(&GPT_REVISION_1_0.to_le_bytes());
        primary[12..16].copy_from_slice(&92u32.to_le_bytes());
        primary[24..32].copy_from_slice(&1u64.to_le_bytes());
        primary[32..40].copy_from_slice(&127u64.to_le_bytes());
        primary[40..48].copy_from_slice(&34u64.to_le_bytes());
        primary[48..56].copy_from_slice(&93u64.to_le_bytes());
        primary[56..72].copy_from_slice(&[0x44; 16]);
        primary[72..80].copy_from_slice(&2u64.to_le_bytes());
        primary[80..84].copy_from_slice(&4u32.to_le_bytes());
        primary[84..88].copy_from_slice(&128u32.to_le_bytes());
        primary[88..92].copy_from_slice(&array_crc.to_le_bytes());
        header_crc(primary);

        let backup = &mut image[sector_size * 127..sector_size * 128];
        backup[..8].copy_from_slice(b"EFI PART");
        backup[8..12].copy_from_slice(&GPT_REVISION_1_0.to_le_bytes());
        backup[12..16].copy_from_slice(&92u32.to_le_bytes());
        backup[24..32].copy_from_slice(&127u64.to_le_bytes());
        backup[32..40].copy_from_slice(&1u64.to_le_bytes());
        backup[40..48].copy_from_slice(&34u64.to_le_bytes());
        backup[48..56].copy_from_slice(&93u64.to_le_bytes());
        backup[56..72].copy_from_slice(&[0x44; 16]);
        backup[72..80].copy_from_slice(&126u64.to_le_bytes());
        backup[80..84].copy_from_slice(&4u32.to_le_bytes());
        backup[84..88].copy_from_slice(&128u32.to_le_bytes());
        backup[88..92].copy_from_slice(&array_crc.to_le_bytes());
        header_crc(backup);
        image
    }

    fn reset_primary_checksums(image: &mut [u8], sector_size: usize) {
        let entries_crc = crc32(&image[sector_size * 2..sector_size * 2 + 512]);
        let header = &mut image[sector_size..sector_size * 2];
        header[88..92].copy_from_slice(&entries_crc.to_le_bytes());
        header_crc(header);
    }

    fn reset_backup_checksums(image: &mut [u8], sector_size: usize) {
        let entries_crc = crc32(&image[sector_size * 126..sector_size * 126 + 512]);
        let header = &mut image[sector_size * 127..sector_size * 128];
        header[88..92].copy_from_slice(&entries_crc.to_le_bytes());
        header_crc(header);
    }

    fn check(image: Vec<u8>, sector_size: u64) -> Result<Report, String> {
        let len = image.len() as u64;
        inspect(&mut Cursor::new(image), len, sector_size)
    }

    #[test]
    fn crc32_known_vector() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    }

    #[test]
    fn valid_full_gpt_512_and_4096() {
        for sector_size in [512, 4096] {
            let result = check(synthetic_gpt(sector_size), sector_size as u64).unwrap();
            assert_eq!(result.partitions, 1);
            assert_eq!(result.efi_system_partitions, 1);
            assert_eq!(result.last_lba, 127);
        }
    }

    #[test]
    fn rejects_missing_protective_mbr() {
        let mut image = synthetic_gpt(512);
        image[450] = 0;
        assert!(check(image, 512).unwrap_err().contains("protective MBR"));
    }

    #[test]
    fn rejects_corrupt_primary_header_checksum() {
        let mut image = synthetic_gpt(512);
        image[512 + 40] ^= 1;
        assert!(check(image, 512).unwrap_err().contains("primary GPT header CRC"));
    }

    #[test]
    fn rejects_corrupt_primary_partition_array_checksum() {
        let mut image = synthetic_gpt(512);
        image[1024 + 40] ^= 1;
        assert!(
            check(image, 512)
                .unwrap_err()
                .contains("primary GPT partition-entry array CRC")
        );
    }

    #[test]
    fn rejects_corrupt_backup_header_checksum() {
        let mut image = synthetic_gpt(512);
        image[127 * 512 + 40] ^= 1;
        assert!(check(image, 512).unwrap_err().contains("backup GPT header CRC"));
    }

    #[test]
    fn rejects_corrupt_backup_partition_array_checksum() {
        let mut image = synthetic_gpt(512);
        image[126 * 512 + 40] ^= 1;
        assert!(
            check(image, 512)
                .unwrap_err()
                .contains("backup GPT partition-entry array CRC")
        );
    }

    #[test]
    fn rejects_nonreciprocal_backup_header_locations() {
        let mut image = synthetic_gpt(512);
        let backup = &mut image[127 * 512..128 * 512];
        backup[32..40].copy_from_slice(&2u64.to_le_bytes());
        header_crc(backup);
        assert!(check(image, 512).unwrap_err().contains("reciprocal"));
    }

    #[test]
    fn rejects_mismatched_backup_disk_guid() {
        let mut image = synthetic_gpt(512);
        let backup = &mut image[127 * 512..128 * 512];
        backup[56] ^= 1;
        header_crc(backup);
        assert!(check(image, 512).unwrap_err().contains("metadata mismatch"));
    }

    #[test]
    fn rejects_mismatched_backup_layout() {
        let mut image = synthetic_gpt(512);
        let backup = &mut image[127 * 512..128 * 512];
        backup[48..56].copy_from_slice(&92u64.to_le_bytes());
        header_crc(backup);
        assert!(check(image, 512).unwrap_err().contains("metadata mismatch"));
    }

    #[test]
    fn rejects_mismatched_entry_arrays_even_with_matching_metadata_crc() {
        let mut image = synthetic_gpt(512);
        image[126 * 512 + 40] ^= 1;
        reset_backup_checksums(&mut image, 512);
        let backup_crc = read_u32(&image[127 * 512..128 * 512], 88);
        let primary = &mut image[512..1024];
        primary[88..92].copy_from_slice(&backup_crc.to_le_bytes());
        header_crc(primary);
        assert!(check(image, 512).unwrap_err().contains("arrays differ"));
    }

    #[test]
    fn rejects_backup_array_overlapping_usable_space() {
        let mut image = synthetic_gpt(512);
        let backup = &mut image[127 * 512..128 * 512];
        backup[72..80].copy_from_slice(&93u64.to_le_bytes());
        header_crc(backup);
        assert!(
            check(image, 512)
                .unwrap_err()
                .contains("backup GPT entry array overlaps")
        );
    }

    #[test]
    fn rejects_partition_outside_usable_lbas() {
        let mut image = synthetic_gpt(512);
        image[1024 + 40..1024 + 48].copy_from_slice(&99u64.to_le_bytes());
        image[126 * 512 + 40..126 * 512 + 48].copy_from_slice(&99u64.to_le_bytes());
        reset_primary_checksums(&mut image, 512);
        reset_backup_checksums(&mut image, 512);
        assert!(check(image, 512).unwrap_err().contains("usable space"));
    }

    #[test]
    fn rejects_overlapping_partitions() {
        let mut image = synthetic_gpt(512);
        for entries_start in [1024, 126 * 512] {
            let second = entries_start + 128;
            image[second..second + 16].copy_from_slice(&ESP_TYPE_GUID);
            image[second + 16..second + 32].copy_from_slice(&[0x43; 16]);
            image[second + 32..second + 40].copy_from_slice(&60u64.to_le_bytes());
            image[second + 40..second + 48].copy_from_slice(&80u64.to_le_bytes());
        }
        reset_primary_checksums(&mut image, 512);
        reset_backup_checksums(&mut image, 512);
        assert!(check(image, 512).unwrap_err().contains("overlapping"));
    }

    #[test]
    fn rejects_entry_array_extent_overflow() {
        let mut image = synthetic_gpt(512);
        let primary = &mut image[512..1024];
        primary[80..84].copy_from_slice(&u32::MAX.to_le_bytes());
        header_crc(primary);
        assert!(check(image, 512).unwrap_err().contains("16 MiB"));
    }

    #[test]
    fn rejects_non_sector_aligned_image() {
        let mut image = synthetic_gpt(512);
        image.push(0);
        assert!(check(image, 512).unwrap_err().contains("sector aligned"));
    }
}
