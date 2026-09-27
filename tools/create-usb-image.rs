//! Create a *new regular-file-only*, blank GPT image for later Vibrix USB tooling.
//! Never opens an existing image for writing and never accepts a raw device.
//! Contents are intentionally unformatted: this is NOT a bootable installer.

use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

const MIB: u64 = 1024 * 1024;
const ESP_MIB: u64 = 32;
const ENTRY_COUNT: u32 = 128;
const ENTRY_BYTES: u32 = 128;
const HEADER_BYTES: usize = 92;
const ESP_TYPE: [u8; 16] = [
    0x28, 0x73, 0x2a, 0xc1, 0x1f, 0xf8, 0xd2, 0x11, 0xba, 0x4b, 0x00, 0xa0, 0xc9, 0x3e, 0xc9, 0x3b,
];
// Standard generic GPT Basic Data type, NOT an adopted Vibrix filesystem type.
const DATA_TYPE: [u8; 16] = [
    0xa2, 0xa0, 0xd0, 0xeb, 0xe5, 0xb9, 0x33, 0x44, 0x87, 0xc0, 0x68, 0xb6, 0xb7, 0x26, 0x99, 0xc7,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Layout {
    sector_size: u64,
    bytes: u64,
    last_lba: u64,
    backup_entries_lba: u64,
    first_usable: u64,
    last_usable: u64,
    esp_first: u64,
    esp_last: u64,
    data_first: u64,
    data_last: u64,
}

fn layout(size_mib: u64, sector_size: u64) -> Result<Layout, &'static str> {
    if !matches!(sector_size, 512 | 4096) {
        return Err("sector size must be 512 or 4096");
    }
    if !(64..=1_048_576).contains(&size_mib) {
        return Err("size must be between 64 MiB and 1 TiB");
    }
    let bytes = size_mib.checked_mul(MIB).ok_or("image size overflow")?;
    let sectors = bytes / sector_size;
    let array_sectors = u64::from(ENTRY_COUNT) * u64::from(ENTRY_BYTES) / sector_size;
    let last_lba = sectors.checked_sub(1).ok_or("invalid image size")?;
    let backup_entries_lba = last_lba
        .checked_sub(array_sectors)
        .ok_or("invalid backup GPT")?;
    let first_usable = 2 + array_sectors;
    let last_usable = backup_entries_lba
        .checked_sub(1)
        .ok_or("invalid usable space")?;
    let alignment = MIB / sector_size;
    let esp_first = alignment;
    let esp_last = esp_first + ESP_MIB * alignment - 1;
    let data_first = esp_last + 1;
    let data_last = (last_usable + 1) / alignment * alignment - 1;
    if first_usable > esp_first || esp_last >= data_first || data_first > data_last {
        return Err("not enough aligned space for ESP and data partition");
    }
    Ok(Layout {
        sector_size,
        bytes,
        last_lba,
        backup_entries_lba,
        first_usable,
        last_usable,
        esp_first,
        esp_last,
        data_first,
        data_last,
    })
}

/// Reflected UEFI/GPT IEEE CRC-32, no allocation or host OS dependency.
fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ if crc & 1 == 1 { 0xedb8_8320 } else { 0 };
        }
    }
    !crc
}

/// GPT stores GUID fields 1–3 in little-endian order.
fn random_guid(random: &mut impl Read) -> io::Result<[u8; 16]> {
    let mut guid = [0u8; 16];
    random.read_exact(&mut guid)?;
    // RFC 4122 version 4 and variant, interpreted through GPT mixed endian.
    guid[7] = guid[7] & 0x0f | 0x40;
    guid[8] = guid[8] & 0x3f | 0x80;
    Ok(guid)
}

fn write_partition(
    entry: &mut [u8],
    type_guid: [u8; 16],
    unique_guid: [u8; 16],
    first: u64,
    last: u64,
    name: &str,
) {
    entry[..16].copy_from_slice(&type_guid);
    entry[16..32].copy_from_slice(&unique_guid);
    entry[32..40].copy_from_slice(&first.to_le_bytes());
    entry[40..48].copy_from_slice(&last.to_le_bytes());
    for (slot, unit) in entry[56..128]
        .chunks_exact_mut(2)
        .zip(name.encode_utf16().take(36))
    {
        slot.copy_from_slice(&unit.to_le_bytes());
    }
}

fn header(plan: Layout, disk_guid: [u8; 16], entries_crc: u32, primary: bool) -> Vec<u8> {
    let mut bytes = vec![0u8; plan.sector_size as usize];
    bytes[..8].copy_from_slice(b"EFI PART");
    bytes[8..12].copy_from_slice(&0x0001_0000u32.to_le_bytes());
    bytes[12..16].copy_from_slice(&(HEADER_BYTES as u32).to_le_bytes());
    bytes[24..32].copy_from_slice(&(if primary { 1 } else { plan.last_lba }).to_le_bytes());
    bytes[32..40].copy_from_slice(&(if primary { plan.last_lba } else { 1 }).to_le_bytes());
    bytes[40..48].copy_from_slice(&plan.first_usable.to_le_bytes());
    bytes[48..56].copy_from_slice(&plan.last_usable.to_le_bytes());
    bytes[56..72].copy_from_slice(&disk_guid);
    bytes[72..80]
        .copy_from_slice(&(if primary { 2 } else { plan.backup_entries_lba }).to_le_bytes());
    bytes[80..84].copy_from_slice(&ENTRY_COUNT.to_le_bytes());
    bytes[84..88].copy_from_slice(&ENTRY_BYTES.to_le_bytes());
    bytes[88..92].copy_from_slice(&entries_crc.to_le_bytes());
    let checksum = crc32(&bytes[..HEADER_BYTES]);
    bytes[16..20].copy_from_slice(&checksum.to_le_bytes());
    bytes
}

/// Writes only GPT metadata, never filesystem contents or boot code.
/// The caller must hold an exclusively created *regular file* of plan.bytes.
fn write_gpt(
    file: &mut (impl Write + Seek),
    plan: Layout,
    disk_guid: [u8; 16],
    esp_guid: [u8; 16],
    data_guid: [u8; 16],
) -> io::Result<()> {
    let mut mbr = [0u8; 512];
    mbr[450] = 0xee;
    mbr[454..458].copy_from_slice(&1u32.to_le_bytes());
    let protected_sectors = plan.last_lba.min(u64::from(u32::MAX)) as u32;
    mbr[458..462].copy_from_slice(&protected_sectors.to_le_bytes());
    mbr[510..512].copy_from_slice(&[0x55, 0xaa]);

    let mut entries = vec![0u8; (ENTRY_COUNT * ENTRY_BYTES) as usize];
    write_partition(
        &mut entries[..ENTRY_BYTES as usize],
        ESP_TYPE,
        esp_guid,
        plan.esp_first,
        plan.esp_last,
        "EFI System",
    );
    write_partition(
        &mut entries[ENTRY_BYTES as usize..(2 * ENTRY_BYTES) as usize],
        DATA_TYPE,
        data_guid,
        plan.data_first,
        plan.data_last,
        "Vibrix data (unformatted)",
    );
    let entries_crc = crc32(&entries);

    file.seek(SeekFrom::Start(0))?;
    file.write_all(&mbr)?;
    file.seek(SeekFrom::Start(plan.sector_size))?;
    file.write_all(&header(plan, disk_guid, entries_crc, true))?;
    file.seek(SeekFrom::Start(2 * plan.sector_size))?;
    file.write_all(&entries)?;
    file.seek(SeekFrom::Start(plan.backup_entries_lba * plan.sector_size))?;
    file.write_all(&entries)?;
    file.seek(SeekFrom::Start(plan.last_lba * plan.sector_size))?;
    file.write_all(&header(plan, disk_guid, entries_crc, false))?;
    Ok(())
}

fn create_image(path: &Path, plan: Layout) -> Result<(), String> {
    if path.extension().is_none_or(|ext| ext != "img") {
        return Err("output must be a new .img regular file".into());
    }
    // A block-device path already exists; create_new refuses it and symlinks.
    // Do not allow creating files under pseudo/device filesystems either.
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let canonical_parent = parent.canonicalize().map_err(|e| e.to_string())?;
    if ["/dev", "/proc", "/sys"]
        .iter()
        .any(|root| canonical_parent.starts_with(root))
    {
        return Err("refusing output inside a device or pseudo-filesystem".into());
    }

    // Distinct fresh on-media identities matter for portable removable USB.
    // Fail instead of reusing hard-coded or guessed GUIDs without entropy.
    let mut random = File::open("/dev/urandom").map_err(|e| e.to_string())?;
    let disk = random_guid(&mut random).map_err(|e| e.to_string())?;
    let esp = random_guid(&mut random).map_err(|e| e.to_string())?;
    let data = random_guid(&mut random).map_err(|e| e.to_string())?;
    if disk == esp || disk == data || esp == data {
        return Err("GUID entropy collision".into());
    }
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("refusing to overwrite or create output: {e}"))?;
    let result = (|| -> io::Result<()> {
        if !output.metadata()?.is_file() {
            return Err(io::Error::other("refusing non-regular output"));
        }
        output.set_len(plan.bytes)?;
        write_gpt(&mut output, plan, disk, esp, data)?;
        output.sync_all()
    })();
    drop(output);
    if let Err(error) = result {
        let _ = fs::remove_file(path); // remove only the file created by create_new
        return Err(error.to_string());
    }
    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if !(3..=4).contains(&args.len()) {
        eprintln!("usage: create-usb-image <new-file.img> <size-MiB> [512|4096]");
        eprintln!("Creates GPT metadata and blank partitions only; NOT bootable.");
        std::process::exit(2);
    }
    let size_mib = args[2].parse::<u64>().unwrap_or(0);
    let sector_size = args
        .get(3)
        .map(String::as_str)
        .unwrap_or("512")
        .parse::<u64>()
        .unwrap_or(0);
    let plan = layout(size_mib, sector_size).unwrap_or_else(|error| {
        eprintln!("invalid image geometry: {error}");
        std::process::exit(2);
    });
    create_image(Path::new(&args[1]), plan).unwrap_or_else(|error| {
        eprintln!("could not create GPT image: {error}");
        std::process::exit(1);
    });
    println!(
        "Created NEW blank regular-file GPT image ({} MiB, {}-byte sectors): {}",
        size_mib, sector_size, args[1]
    );
    println!("Partition 1: 32 MiB ESP placeholder; partition 2: generic data placeholder.");
    println!(
        "Neither partition is formatted; do not write this image directly to USB as an installer."
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rd_u64(bytes: &[u8], at: usize) -> u64 {
        u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
    }

    #[test]
    fn known_crc_vector() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    }

    #[test]
    fn geometry_for_both_sector_sizes_and_limit_failures() {
        for sector in [512, 4096] {
            let p = layout(64, sector).unwrap();
            assert_eq!(p.bytes, 64 * MIB);
            assert_eq!(p.esp_first * sector, MIB);
            assert_eq!((p.esp_last + 1 - p.esp_first) * sector, 32 * MIB);
            assert_eq!(p.data_first * sector, 33 * MIB);
            assert!(p.data_last <= p.last_usable);
            assert!(p.backup_entries_lba > p.last_usable);
        }
        assert!(layout(63, 512).is_err());
        assert!(layout(u64::MAX, 512).is_err());
        assert!(layout(64, 2048).is_err());
        assert!(layout(64, 0).is_err());
    }

    #[test]
    fn independent_headers_are_reciprocal_and_crc_valid() {
        for sector in [512, 4096] {
            let p = layout(64, sector).unwrap();
            let a = header(p, [1; 16], 0x1122_3344, true);
            let b = header(p, [1; 16], 0x1122_3344, false);
            assert_eq!(&a[..8], b"EFI PART");
            assert_eq!(rd_u64(&a, 24), 1);
            assert_eq!(rd_u64(&b, 24), p.last_lba);
            assert_eq!(rd_u64(&a, 32), p.last_lba);
            assert_eq!(rd_u64(&b, 32), 1);
            assert_eq!(rd_u64(&a, 72), 2);
            assert_eq!(rd_u64(&b, 72), p.backup_entries_lba);
            for mut hdr in [a, b] {
                let saved = u32::from_le_bytes(hdr[16..20].try_into().unwrap());
                hdr[16..20].fill(0);
                assert_eq!(crc32(&hdr[..92]), saved);
            }
        }
    }

    #[test]
    fn fresh_guids_are_versioned_and_distinct() {
        let mut bytes = std::io::Cursor::new((0u8..48).collect::<Vec<u8>>());
        let a = random_guid(&mut bytes).unwrap();
        let b = random_guid(&mut bytes).unwrap();
        assert_ne!(a, b);
        assert_eq!(a[7] & 0xf0, 0x40);
        assert_eq!(a[8] & 0xc0, 0x80);
    }

    #[test]
    fn refusing_an_existing_file_preserves_contents() {
        let base =
            std::env::temp_dir().join(format!("vibrix-gpt-no-overwrite-{}", std::process::id()));
        let file = base.with_extension("img");
        std::fs::write(&file, b"not a disk").unwrap();
        let result = create_image(&file, layout(64, 512).unwrap());
        assert!(result.unwrap_err().contains("refusing to overwrite"));
        assert_eq!(std::fs::read(&file).unwrap(), b"not a disk");
        std::fs::remove_file(file).unwrap();
    }

    #[test]
    fn rejects_paths_that_look_like_raw_devices() {
        let plan = layout(64, 512).unwrap();
        assert!(create_image(Path::new("/dev/vibrix-test.img"), plan).is_err());
        assert!(create_image(Path::new("something.device"), plan).is_err());
    }
}
