//! Read-only GPT inspection of a regular host-side disk-image file.
//!
//! This is diagnostic tooling. It never provisions, repairs or writes a disk.
use std::collections::HashSet;
use std::env;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

const MAX_ENTRY_ARRAY_BYTES: u64 = 16 * 1024 * 1024;
const GPT_HEADER_MIN_BYTES: usize = 92;
const GPT_ENTRY_MIN_BYTES: u32 = 128;
const ESP_TYPE_GUID: [u8; 16] = [
    0x28, 0x73, 0x2a, 0xc1, 0x1f, 0xf8, 0xd2, 0x11, 0xba, 0x4b, 0x00, 0xa0, 0xc9, 0x3e, 0xc9, 0x3b,
];

#[derive(Debug, PartialEq, Eq)]
struct Report {
    sector_size: u64,
    last_lba: u64,
    partitions: usize,
    efi_system_partitions: usize,
    disk_guid: [u8; 16],
    partition_guids: Vec<(usize, [u8; 16])>,
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("checked input"))
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().expect("checked input"))
}

/// GPT stores the first three GUID fields in little-endian byte order.
/// Render the standard GUID text instead of printing the raw on-disk bytes.
fn format_guid(guid: &[u8; 16]) -> String {
    format!(
        "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        read_u32(guid, 0),
        u16::from_le_bytes([guid[4], guid[5]]),
        u16::from_le_bytes([guid[6], guid[7]]),
        guid[8],
        guid[9],
        guid[10],
        guid[11],
        guid[12],
        guid[13],
        guid[14],
        guid[15]
    )
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

    let sector_len = usize::try_from(sector_size).map_err(|e| e.to_string())?;
    let mut header = vec![0u8; sector_len];
    image
        .seek(SeekFrom::Start(sector_size))
        .map_err(|e| e.to_string())?;
    image.read_exact(&mut header).map_err(|e| e.to_string())?;
    if &header[..8] != b"EFI PART" {
        return Err("missing primary GPT signature".into());
    }
    let header_size = usize::try_from(read_u32(&header, 12)).map_err(|e| e.to_string())?;
    if !(GPT_HEADER_MIN_BYTES..=sector_len).contains(&header_size) {
        return Err("invalid GPT header size".into());
    }
    let expected_header_crc = read_u32(&header, 16);
    header[16..20].fill(0);
    if crc32(&header[..header_size]) != expected_header_crc {
        return Err("primary GPT header CRC mismatch".into());
    }
    if read_u32(&header, 8) != 0x0001_0000 || read_u32(&header, 20) != 0 {
        return Err("unsupported GPT revision or nonzero reserved bytes".into());
    }
    if read_u64(&header, 24) != 1 || read_u64(&header, 32) != last_lba {
        return Err("invalid primary/backup GPT header locations".into());
    }
    let first_usable = read_u64(&header, 40);
    let last_usable = read_u64(&header, 48);
    if first_usable < 2 || first_usable > last_usable || last_usable >= last_lba {
        return Err("invalid GPT usable-LBA range".into());
    }

    let entry_lba = read_u64(&header, 72);
    let entry_count = read_u32(&header, 80);
    let entry_size = read_u32(&header, 84);
    if entry_lba < 2 || entry_count == 0 || entry_size < GPT_ENTRY_MIN_BYTES || entry_size % 8 != 0
    {
        return Err("invalid GPT partition-entry layout".into());
    }
    let array_len = u64::from(entry_count)
        .checked_mul(u64::from(entry_size))
        .ok_or("GPT entry array overflow")?;
    if array_len > MAX_ENTRY_ARRAY_BYTES {
        return Err("GPT entry array exceeds the 16 MiB inspection limit".into());
    }
    let array_start = entry_lba
        .checked_mul(sector_size)
        .ok_or("GPT entry offset overflow")?;
    let array_end = array_start
        .checked_add(array_len)
        .ok_or("GPT entry array end overflow")?;
    let first_usable_byte = first_usable
        .checked_mul(sector_size)
        .ok_or("GPT usable range overflow")?;
    if array_end > first_usable_byte || array_end > image_len {
        return Err("GPT entry array overlaps usable space or exceeds image".into());
    }

    let array_size = usize::try_from(array_len).map_err(|e| e.to_string())?;
    let mut entries = vec![0u8; array_size];
    image
        .seek(SeekFrom::Start(array_start))
        .map_err(|e| e.to_string())?;
    image.read_exact(&mut entries).map_err(|e| e.to_string())?;
    if crc32(&entries) != read_u32(&header, 88) {
        return Err("GPT partition-entry array CRC mismatch".into());
    }

    // The backup header occupies the final logical block. Its entry array
    // must be wholly between usable space and that final header block.
    let mut backup = vec![0u8; sector_len];
    let backup_start = last_lba
        .checked_mul(sector_size)
        .ok_or("backup GPT offset overflow")?;
    image
        .seek(SeekFrom::Start(backup_start))
        .map_err(|e| e.to_string())?;
    image.read_exact(&mut backup).map_err(|e| e.to_string())?;
    if &backup[..8] != b"EFI PART" {
        return Err("missing backup GPT signature".into());
    }
    let backup_size = usize::try_from(read_u32(&backup, 12)).map_err(|e| e.to_string())?;
    if !(GPT_HEADER_MIN_BYTES..=sector_len).contains(&backup_size) {
        return Err("invalid backup GPT header size".into());
    }
    let backup_crc = read_u32(&backup, 16);
    backup[16..20].fill(0);
    if crc32(&backup[..backup_size]) != backup_crc {
        return Err("backup GPT header CRC mismatch".into());
    }
    if read_u32(&backup, 8) != 0x0001_0000 || read_u32(&backup, 20) != 0 {
        return Err("unsupported backup GPT revision or nonzero reserved bytes".into());
    }
    if read_u64(&backup, 24) != last_lba || read_u64(&backup, 32) != 1 {
        return Err("invalid reciprocal backup GPT header locations".into());
    }
    if backup_size != header_size
        || backup[8..16] != header[8..16]
        || backup[40..72] != header[40..72]
        || backup[80..88] != header[80..88]
    {
        return Err("primary/backup GPT disk or layout metadata mismatch".into());
    }
    let backup_entry_lba = read_u64(&backup, 72);
    let backup_array_start = backup_entry_lba
        .checked_mul(sector_size)
        .ok_or("backup GPT entry offset overflow")?;
    let backup_array_end = backup_array_start
        .checked_add(array_len)
        .ok_or("backup GPT entry array end overflow")?;
    let after_usable = last_usable
        .checked_add(1)
        .and_then(|lba| lba.checked_mul(sector_size))
        .ok_or("backup GPT usable range overflow")?;
    if backup_entry_lba <= last_usable
        || backup_array_start < after_usable
        || backup_array_end > backup_start
    {
        return Err("backup GPT entry array overlaps usable space or backup header".into());
    }
    let mut backup_entries = vec![0u8; array_size];
    image
        .seek(SeekFrom::Start(backup_array_start))
        .map_err(|e| e.to_string())?;
    image
        .read_exact(&mut backup_entries)
        .map_err(|e| e.to_string())?;
    if crc32(&backup_entries) != read_u32(&backup, 88) {
        return Err("backup GPT partition-entry array CRC mismatch".into());
    }
    if entries != backup_entries {
        return Err("primary/backup GPT partition-entry arrays differ".into());
    }

    let mut partitions = 0;
    let mut efi_system_partitions = 0;
    let mut used_ranges = Vec::new();
    let mut seen_guids = HashSet::new();
    let mut partition_guids = Vec::new();
    for (index, entry) in entries.chunks_exact(entry_size as usize).enumerate() {
        if entry[..16].iter().all(|&byte| byte == 0) {
            continue;
        }
        let unique_guid: [u8; 16] = entry[16..32].try_into().expect("fixed GPT GUID width");
        if unique_guid.iter().all(|&byte| byte == 0) {
            return Err("used GPT partition has an empty unique GUID".into());
        }
        let first = read_u64(entry, 32);
        let last = read_u64(entry, 40);
        if first < first_usable || first > last || last > last_usable {
            return Err("partition LBA range outside GPT usable space".into());
        }
        if !seen_guids.insert(unique_guid) {
            return Err(format!(
                "duplicate GPT partition unique GUID: {}",
                format_guid(&unique_guid)
            ));
        }
        used_ranges.push((first, last));
        partition_guids.push((index + 1, unique_guid));
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
        disk_guid: header[56..72]
            .try_into()
            .expect("fixed GPT disk GUID width"),
        partition_guids,
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
        Ok(report) => {
            println!(
                "GPT primary and backup header/entry CRCs valid; sector_size={} last_lba={} partitions={} efi_system_partitions={} disk_guid={}",
                report.sector_size,
                report.last_lba,
                report.partitions,
                report.efi_system_partitions,
                format_guid(&report.disk_guid)
            );
            for (index, guid) in &report.partition_guids {
                println!(
                    "partition_entry={} unique_guid={}",
                    index,
                    format_guid(guid)
                );
            }
        }
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

    fn synthetic_gpt(sector_size: usize) -> Vec<u8> {
        let mut image = vec![0u8; sector_size * 128];
        image[510..512].copy_from_slice(&[0x55, 0xaa]);
        image[450] = 0xee;

        let entries_start = sector_size * 2;
        let entry = &mut image[entries_start..entries_start + 128];
        entry[..16].copy_from_slice(&ESP_TYPE_GUID);
        entry[16..32].copy_from_slice(&[0x42; 16]);
        entry[32..40].copy_from_slice(&40u64.to_le_bytes());
        entry[40..48].copy_from_slice(&70u64.to_le_bytes());
        let array_crc = crc32(&image[entries_start..entries_start + 512]);

        let header = &mut image[sector_size..sector_size * 2];
        header[..8].copy_from_slice(b"EFI PART");
        header[8..12].copy_from_slice(&0x0001_0000u32.to_le_bytes());
        header[12..16].copy_from_slice(&92u32.to_le_bytes());
        header[24..32].copy_from_slice(&1u64.to_le_bytes());
        header[32..40].copy_from_slice(&127u64.to_le_bytes());
        header[40..48].copy_from_slice(&34u64.to_le_bytes());
        header[48..56].copy_from_slice(&93u64.to_le_bytes());
        header[56..72].copy_from_slice(&[0x44; 16]);
        header[72..80].copy_from_slice(&2u64.to_le_bytes());
        header[80..84].copy_from_slice(&4u32.to_le_bytes());
        header[84..88].copy_from_slice(&128u32.to_le_bytes());
        header[88..92].copy_from_slice(&array_crc.to_le_bytes());
        let header_crc = crc32(&header[..92]);
        header[16..20].copy_from_slice(&header_crc.to_le_bytes());
        let backup_entries_start = sector_size * 126;
        let primary_entries = image[entries_start..entries_start + 512].to_vec();
        image[backup_entries_start..backup_entries_start + 512].copy_from_slice(&primary_entries);
        let primary_header = image[sector_size..sector_size * 2].to_vec();
        let backup = &mut image[sector_size * 127..sector_size * 128];
        backup[..92].copy_from_slice(&primary_header[..92]);
        backup[24..32].copy_from_slice(&127u64.to_le_bytes());
        backup[32..40].copy_from_slice(&1u64.to_le_bytes());
        backup[72..80].copy_from_slice(&126u64.to_le_bytes());
        backup[16..20].fill(0);
        let backup_crc = crc32(&backup[..92]);
        backup[16..20].copy_from_slice(&backup_crc.to_le_bytes());
        image
    }

    fn reset_checksums(image: &mut [u8], sector_size: usize) {
        let entries_crc = crc32(&image[sector_size * 2..sector_size * 2 + 512]);
        let header = &mut image[sector_size..sector_size * 2];
        header[88..92].copy_from_slice(&entries_crc.to_le_bytes());
        header[16..20].fill(0);
        let header_crc = crc32(&header[..92]);
        header[16..20].copy_from_slice(&header_crc.to_le_bytes());
        let primary_entries = image[sector_size * 2..sector_size * 2 + 512].to_vec();
        image[sector_size * 126..sector_size * 126 + 512].copy_from_slice(&primary_entries);
        let backup = &mut image[sector_size * 127..sector_size * 128];
        backup[88..92].copy_from_slice(&entries_crc.to_le_bytes());
        backup[16..20].fill(0);
        let backup_crc = crc32(&backup[..92]);
        backup[16..20].copy_from_slice(&backup_crc.to_le_bytes());
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
    fn valid_primary_gpt_512_and_4096() {
        for sector_size in [512, 4096] {
            let result = check(synthetic_gpt(sector_size), sector_size as u64).unwrap();
            assert_eq!(result.partitions, 1);
            assert_eq!(result.efi_system_partitions, 1);
            assert_eq!(result.last_lba, 127);
            assert_eq!(result.disk_guid, [0x44; 16]);
            assert_eq!(result.partition_guids, vec![(1, [0x42; 16])]);
        }
    }

    #[test]
    fn formats_uefi_mixed_endian_guids() {
        let raw = [
            0x78, 0x56, 0x34, 0x12, 0xbc, 0x9a, 0xf0, 0xde, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66,
            0x77, 0x88,
        ];
        assert_eq!(format_guid(&raw), "12345678-9abc-def0-1122-334455667788");
    }

    fn add_second_partition(image: &mut [u8], sector_size: usize, guid: [u8; 16]) {
        let second = sector_size * 2 + 128;
        image[second..second + 16].copy_from_slice(&[0x43; 16]);
        image[second + 16..second + 32].copy_from_slice(&guid);
        image[second + 32..second + 40].copy_from_slice(&72u64.to_le_bytes());
        image[second + 40..second + 48].copy_from_slice(&82u64.to_le_bytes());
        reset_checksums(image, sector_size);
    }

    #[test]
    fn reports_disk_and_distinct_partition_guids_for_both_sector_sizes() {
        for sector_size in [512usize, 4096] {
            let mut image = synthetic_gpt(sector_size);
            add_second_partition(&mut image, sector_size, [0x43; 16]);
            let report = check(image, sector_size as u64).unwrap();
            assert_eq!(report.partitions, 2);
            assert_eq!(report.efi_system_partitions, 1);
            assert_eq!(
                format_guid(&report.disk_guid),
                "44444444-4444-4444-4444-444444444444"
            );
            assert_eq!(
                report.partition_guids,
                vec![(1, [0x42; 16]), (2, [0x43; 16])]
            );
        }
    }

    #[test]
    fn rejects_duplicate_unique_guids_with_valid_copy_crcs_for_both_sector_sizes() {
        for sector_size in [512usize, 4096] {
            let mut image = synthetic_gpt(sector_size);
            add_second_partition(&mut image, sector_size, [0x42; 16]);
            let err = check(image, sector_size as u64).unwrap_err();
            assert!(err.contains("duplicate GPT partition unique GUID"), "{err}");
            assert!(
                err.contains("42424242-4242-4242-4242-424242424242"),
                "{err}"
            );
        }
    }

    #[test]
    fn rejects_missing_protective_mbr() {
        let mut image = synthetic_gpt(512);
        image[450] = 0;
        assert!(check(image, 512).unwrap_err().contains("protective MBR"));
    }

    #[test]
    fn rejects_corrupt_header_checksum() {
        let mut image = synthetic_gpt(512);
        image[512 + 40] ^= 1;
        assert!(check(image, 512).unwrap_err().contains("header CRC"));
    }

    #[test]
    fn rejects_corrupt_partition_array_checksum() {
        let mut image = synthetic_gpt(512);
        image[1024 + 40] ^= 1;
        assert!(check(image, 512).unwrap_err().contains("array CRC"));
    }

    #[test]
    fn rejects_partition_outside_usable_lbas() {
        let mut image = synthetic_gpt(512);
        image[1024 + 40..1024 + 48].copy_from_slice(&99u64.to_le_bytes());
        reset_checksums(&mut image, 512);
        assert!(check(image, 512).unwrap_err().contains("usable space"));
    }

    #[test]
    fn rejects_overlapping_partitions() {
        let mut image = synthetic_gpt(512);
        let second = 1024 + 128;
        image[second..second + 16].copy_from_slice(&ESP_TYPE_GUID);
        image[second + 16..second + 32].copy_from_slice(&[0x43; 16]);
        image[second + 32..second + 40].copy_from_slice(&60u64.to_le_bytes());
        image[second + 40..second + 48].copy_from_slice(&80u64.to_le_bytes());
        reset_checksums(&mut image, 512);
        assert!(check(image, 512).unwrap_err().contains("overlapping"));
    }

    #[test]
    fn rejects_entry_array_extent_overflow() {
        let mut image = synthetic_gpt(512);
        let header = &mut image[512..1024];
        header[80..84].copy_from_slice(&u32::MAX.to_le_bytes());
        header[16..20].fill(0);
        let header_crc = crc32(&header[..92]);
        header[16..20].copy_from_slice(&header_crc.to_le_bytes());
        assert!(check(image, 512).unwrap_err().contains("16 MiB"));
    }

    #[test]
    fn rejects_backup_corruption_for_both_sector_sizes() {
        for sector_size in [512usize, 4096] {
            let mut image = synthetic_gpt(sector_size);
            image[sector_size * 127 + 40] ^= 1;
            assert!(check(image, sector_size as u64)
                .unwrap_err()
                .contains("backup GPT header CRC"));

            let mut image = synthetic_gpt(sector_size);
            image[sector_size * 126 + 40] ^= 1;
            assert!(check(image, sector_size as u64)
                .unwrap_err()
                .contains("backup GPT partition-entry array CRC"));

            let mut image = synthetic_gpt(sector_size);
            image[sector_size * 127 + 32..sector_size * 127 + 40]
                .copy_from_slice(&2u64.to_le_bytes());
            reset_backup_header_crc(&mut image, sector_size);
            assert!(check(image, sector_size as u64)
                .unwrap_err()
                .contains("reciprocal"));

            let mut image = synthetic_gpt(sector_size);
            image[sector_size * 127 + 56] ^= 1;
            reset_backup_header_crc(&mut image, sector_size);
            assert!(check(image, sector_size as u64)
                .unwrap_err()
                .contains("metadata mismatch"));

            let mut image = synthetic_gpt(sector_size);
            image[sector_size * 126 + 16] ^= 1;
            let crc = crc32(&image[sector_size * 126..sector_size * 126 + 512]);
            image[sector_size * 127 + 88..sector_size * 127 + 92]
                .copy_from_slice(&crc.to_le_bytes());
            reset_backup_header_crc(&mut image, sector_size);
            assert!(check(image, sector_size as u64)
                .unwrap_err()
                .contains("arrays differ"));

            let mut image = synthetic_gpt(sector_size);
            image[sector_size * 127 + 72..sector_size * 127 + 80]
                .copy_from_slice(&93u64.to_le_bytes());
            reset_backup_header_crc(&mut image, sector_size);
            assert!(check(image, sector_size as u64)
                .unwrap_err()
                .contains("backup GPT entry array overlaps"));
        }
    }

    fn reset_backup_header_crc(image: &mut [u8], sector_size: usize) {
        let backup = &mut image[sector_size * 127..sector_size * 128];
        backup[16..20].fill(0);
        let crc = crc32(&backup[..92]);
        backup[16..20].copy_from_slice(&crc.to_le_bytes());
    }

    #[test]
    fn rejects_non_sector_aligned_image() {
        let mut image = synthetic_gpt(512);
        image.push(0);
        assert!(check(image, 512).unwrap_err().contains("sector aligned"));
    }
}
