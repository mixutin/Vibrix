//! Populate the blank ESP in a Vibrix GPT regular-file image with a minimal FAT16 layout.
//! This tool refuses raw devices, symlinks, non-blank ESPs and unexpected GPT geometry.

use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

const MIB: u64 = 1024 * 1024;
const ESP_BYTES: u64 = 32 * MIB;
const ESP_TYPE: [u8; 16] = [
    0x28, 0x73, 0x2a, 0xc1, 0x1f, 0xf8, 0xd2, 0x11, 0xba, 0x4b, 0x00, 0xa0, 0xc9, 0x3e, 0xc9, 0x3b,
];
const ROOT_ENTRIES: u16 = 512;
const RESERVED_SECTORS: u16 = 1;
const FAT_COPIES: u8 = 2;
const MEDIA: u8 = 0xf8;
const EOC: u16 = 0xffff;
const MAX_INPUT_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Esp {
    first_lba: u64,
    sectors: u64,
    sector_size: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FatLayout {
    bytes_per_sector: usize,
    total_sectors: usize,
    root_dir_sectors: usize,
    fat_sectors: usize,
    first_root_sector: usize,
    first_data_sector: usize,
    cluster_count: usize,
}

fn rd_u32(bytes: &[u8], at: usize) -> Result<u32, String> {
    Ok(u32::from_le_bytes(
        bytes
            .get(at..at + 4)
            .ok_or("truncated integer")?
            .try_into()
            .unwrap(),
    ))
}

fn rd_u64(bytes: &[u8], at: usize) -> Result<u64, String> {
    Ok(u64::from_le_bytes(
        bytes
            .get(at..at + 8)
            .ok_or("truncated integer")?
            .try_into()
            .unwrap(),
    ))
}

fn find_esp(file: &mut File, sector_size: u64) -> Result<Esp, String> {
    if !matches!(sector_size, 512 | 4096) {
        return Err("sector size must be 512 or 4096".into());
    }
    let len = file.metadata().map_err(|e| e.to_string())?.len();
    if len < 64 * MIB || len % sector_size != 0 {
        return Err("image length is not supported GPT geometry".into());
    }
    let mut mbr = [0u8; 512];
    file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    file.read_exact(&mut mbr).map_err(|e| e.to_string())?;
    if mbr[510..512] != [0x55, 0xaa] || mbr[450] != 0xee {
        return Err("missing Protective MBR".into());
    }
    let mut header = vec![0u8; sector_size as usize];
    file.seek(SeekFrom::Start(sector_size))
        .map_err(|e| e.to_string())?;
    file.read_exact(&mut header).map_err(|e| e.to_string())?;
    if header.get(..8) != Some(b"EFI PART") || rd_u64(&header, 24)? != 1 {
        return Err("missing primary GPT header".into());
    }
    let entries_lba = rd_u64(&header, 72)?;
    let count = rd_u32(&header, 80)? as usize;
    let entry_size = rd_u32(&header, 84)? as usize;
    if entries_lba != 2 || count == 0 || count > 128 || entry_size < 128 || entry_size > 4096 {
        return Err("unsupported GPT entry layout".into());
    }
    let bytes = count
        .checked_mul(entry_size)
        .ok_or("GPT entry size overflow")?;
    let mut entries = vec![0u8; bytes];
    file.seek(SeekFrom::Start(entries_lba * sector_size))
        .map_err(|e| e.to_string())?;
    file.read_exact(&mut entries).map_err(|e| e.to_string())?;
    let mut found = None;
    for entry in entries.chunks_exact(entry_size) {
        if entry[..16] != ESP_TYPE {
            continue;
        }
        if found.is_some() {
            return Err("multiple EFI System Partitions are not supported".into());
        }
        let first = rd_u64(entry, 32)?;
        let last = rd_u64(entry, 40)?;
        if first == 0 || last < first {
            return Err("invalid ESP extent".into());
        }
        let sectors = last
            .checked_sub(first)
            .and_then(|n| n.checked_add(1))
            .ok_or("ESP extent overflow")?;
        if first
            .checked_mul(sector_size)
            .ok_or("ESP offset overflow")?
            != MIB
            || sectors
                .checked_mul(sector_size)
                .ok_or("ESP length overflow")?
                != ESP_BYTES
        {
            return Err("ESP does not match adopted 1 MiB / 32 MiB layout".into());
        }
        let end = last
            .checked_add(1)
            .and_then(|n| n.checked_mul(sector_size))
            .ok_or("ESP end overflow")?;
        if end > len {
            return Err("ESP exceeds image".into());
        }
        found = Some(Esp {
            first_lba: first,
            sectors,
            sector_size,
        });
    }
    found.ok_or_else(|| "EFI System Partition not found".into())
}

fn fat_layout(esp: Esp) -> Result<FatLayout, String> {
    let bps = usize::try_from(esp.sector_size).map_err(|_| "sector too large")?;
    let total = usize::try_from(esp.sectors).map_err(|_| "ESP too large")?;
    let root_bytes = usize::from(ROOT_ENTRIES) * 32;
    let root_dir_sectors = root_bytes.div_ceil(bps);
    let mut fat_sectors = 1usize;
    loop {
        let overhead = usize::from(RESERVED_SECTORS)
            .checked_add(usize::from(FAT_COPIES) * fat_sectors)
            .and_then(|n| n.checked_add(root_dir_sectors))
            .ok_or("FAT geometry overflow")?;
        let data_sectors = total
            .checked_sub(overhead)
            .ok_or("ESP too small for FAT16")?;
        let clusters = data_sectors; // exactly one sector per cluster
        let needed = (clusters + 2)
            .checked_mul(2)
            .ok_or("FAT size overflow")?
            .div_ceil(bps);
        if needed == fat_sectors {
            if !(4085..=65524).contains(&clusters) {
                return Err("32 MiB ESP does not produce a FAT16 cluster count".into());
            }
            let first_root_sector =
                usize::from(RESERVED_SECTORS) + usize::from(FAT_COPIES) * fat_sectors;
            let first_data_sector = first_root_sector + root_dir_sectors;
            return Ok(FatLayout {
                bytes_per_sector: bps,
                total_sectors: total,
                root_dir_sectors,
                fat_sectors,
                first_root_sector,
                first_data_sector,
                cluster_count: clusters,
            });
        }
        fat_sectors = needed;
    }
}

fn short_entry(name: &[u8; 11], attr: u8, cluster: u16, size: u32) -> [u8; 32] {
    let mut entry = [0u8; 32];
    entry[..11].copy_from_slice(name);
    entry[11] = attr;
    let date = 0x0021u16; // 1980-01-01
    entry[16..18].copy_from_slice(&date.to_le_bytes());
    entry[18..20].copy_from_slice(&date.to_le_bytes());
    entry[24..26].copy_from_slice(&date.to_le_bytes());
    entry[26..28].copy_from_slice(&cluster.to_le_bytes());
    entry[28..32].copy_from_slice(&size.to_le_bytes());
    entry
}

fn dot_entry(parent: bool, cluster: u16) -> [u8; 32] {
    let name = if parent {
        *b"..         "
    } else {
        *b".          "
    };
    short_entry(&name, 0x10, cluster, 0)
}

fn cluster_offset(layout: FatLayout, cluster: u16) -> Result<usize, String> {
    if cluster < 2 || usize::from(cluster) >= layout.cluster_count + 2 {
        return Err("cluster outside FAT data area".into());
    }
    Ok((layout.first_data_sector + (usize::from(cluster) - 2)) * layout.bytes_per_sector)
}

fn allocate_chain(
    next: &mut u16,
    bytes: usize,
    cluster_bytes: usize,
    fat: &mut [u16],
) -> Result<u16, String> {
    if bytes == 0 {
        return Err("refusing zero-length boot file".into());
    }
    let count = bytes.div_ceil(cluster_bytes);
    let first = *next;
    for i in 0..count {
        let cluster = usize::from(first) + i;
        if cluster >= fat.len() || cluster > u16::MAX as usize {
            return Err("boot files do not fit ESP".into());
        }
        fat[cluster] = if i + 1 == count {
            EOC
        } else {
            (cluster + 1) as u16
        };
    }
    *next = first
        .checked_add(u16::try_from(count).map_err(|_| "file chain too long")?)
        .ok_or("cluster number overflow")?;
    Ok(first)
}

fn copy_file(
    fs: &mut [u8],
    layout: FatLayout,
    fat: &[u16],
    start: u16,
    data: &[u8],
) -> Result<(), String> {
    let mut cluster = start;
    let mut copied = 0usize;
    let mut guard = 0usize;
    while copied < data.len() {
        guard += 1;
        if guard > layout.cluster_count {
            return Err("FAT chain cycle".into());
        }
        let off = cluster_offset(layout, cluster)?;
        let take = (data.len() - copied).min(layout.bytes_per_sector);
        fs[off..off + take].copy_from_slice(&data[copied..copied + take]);
        copied += take;
        if copied == data.len() {
            break;
        }
        let n = *fat
            .get(usize::from(cluster))
            .ok_or("FAT chain outside table")?;
        if n >= 0xfff8 || n < 2 {
            return Err("FAT chain terminated early".into());
        }
        cluster = n;
    }
    Ok(())
}

fn build_fat16(esp: Esp, boot: &[u8], kernel: &[u8]) -> Result<Vec<u8>, String> {
    let layout = fat_layout(esp)?;
    let bytes = layout
        .total_sectors
        .checked_mul(layout.bytes_per_sector)
        .ok_or("ESP allocation overflow")?;
    let mut fs = vec![0u8; bytes];

    // BPB / FAT16 EBPB.
    fs[..3].copy_from_slice(&[0xeb, 0x3c, 0x90]);
    fs[3..11].copy_from_slice(b"VIBRIX  ");
    fs[11..13].copy_from_slice(&(layout.bytes_per_sector as u16).to_le_bytes());
    fs[13] = 1; // sectors per cluster
    fs[14..16].copy_from_slice(&RESERVED_SECTORS.to_le_bytes());
    fs[16] = FAT_COPIES;
    fs[17..19].copy_from_slice(&ROOT_ENTRIES.to_le_bytes());
    if layout.total_sectors < 65536 {
        fs[19..21].copy_from_slice(&(layout.total_sectors as u16).to_le_bytes());
    } else {
        fs[19..21].fill(0);
        fs[32..36].copy_from_slice(&(layout.total_sectors as u32).to_le_bytes());
    }
    fs[21] = MEDIA;
    fs[22..24].copy_from_slice(&(layout.fat_sectors as u16).to_le_bytes());
    fs[24..26].copy_from_slice(&63u16.to_le_bytes());
    fs[26..28].copy_from_slice(&255u16.to_le_bytes());
    fs[28..32].copy_from_slice(
        &u32::try_from(esp.first_lba)
            .map_err(|_| "ESP LBA exceeds FAT hidden-sector field")?
            .to_le_bytes(),
    );
    fs[36] = 0x80;
    fs[38] = 0x29;
    fs[39..43].copy_from_slice(&0x5649_4252u32.to_le_bytes());
    fs[43..54].copy_from_slice(b"VIBRIX ESP ");
    fs[54..62].copy_from_slice(b"FAT16   ");
    fs[510..512].copy_from_slice(&[0x55, 0xaa]);

    let mut fat = vec![0u16; layout.cluster_count + 2];
    fat[0] = 0xff00 | u16::from(MEDIA);
    fat[1] = EOC;
    fat[2] = EOC; // EFI
    fat[3] = EOC; // EFI/BOOT
    fat[4] = EOC; // VIBRIX
    let mut next = 5u16;
    let boot_cluster = allocate_chain(&mut next, boot.len(), layout.bytes_per_sector, &mut fat)?;
    let kernel_cluster =
        allocate_chain(&mut next, kernel.len(), layout.bytes_per_sector, &mut fat)?;

    let fat_bytes = layout.fat_sectors * layout.bytes_per_sector;
    let mut encoded = vec![0u8; fat_bytes];
    for (i, value) in fat.iter().enumerate() {
        let at = i * 2;
        if at + 2 > encoded.len() {
            break;
        }
        encoded[at..at + 2].copy_from_slice(&value.to_le_bytes());
    }
    for copy in 0..usize::from(FAT_COPIES) {
        let off =
            (usize::from(RESERVED_SECTORS) + copy * layout.fat_sectors) * layout.bytes_per_sector;
        fs[off..off + fat_bytes].copy_from_slice(&encoded);
    }

    let root = layout.first_root_sector * layout.bytes_per_sector;
    fs[root..root + 32].copy_from_slice(&short_entry(b"VIBRIX ESP ", 0x08, 0, 0));
    fs[root + 32..root + 64].copy_from_slice(&short_entry(b"EFI        ", 0x10, 2, 0));
    fs[root + 64..root + 96].copy_from_slice(&short_entry(b"VIBRIX     ", 0x10, 4, 0));

    let efi = cluster_offset(layout, 2)?;
    fs[efi..efi + 32].copy_from_slice(&dot_entry(false, 2));
    fs[efi + 32..efi + 64].copy_from_slice(&dot_entry(true, 0));
    fs[efi + 64..efi + 96].copy_from_slice(&short_entry(b"BOOT       ", 0x10, 3, 0));

    let boot_dir = cluster_offset(layout, 3)?;
    fs[boot_dir..boot_dir + 32].copy_from_slice(&dot_entry(false, 3));
    fs[boot_dir + 32..boot_dir + 64].copy_from_slice(&dot_entry(true, 2));
    fs[boot_dir + 64..boot_dir + 96].copy_from_slice(&short_entry(
        b"BOOTX64 EFI",
        0x20,
        boot_cluster,
        u32::try_from(boot.len()).map_err(|_| "bootloader too large")?,
    ));

    let vibrix = cluster_offset(layout, 4)?;
    fs[vibrix..vibrix + 32].copy_from_slice(&dot_entry(false, 4));
    fs[vibrix + 32..vibrix + 64].copy_from_slice(&dot_entry(true, 0));
    fs[vibrix + 64..vibrix + 96].copy_from_slice(&short_entry(
        b"KERNEL  ELF",
        0x20,
        kernel_cluster,
        u32::try_from(kernel.len()).map_err(|_| "kernel too large")?,
    ));

    copy_file(&mut fs, layout, &fat, boot_cluster, boot)?;
    copy_file(&mut fs, layout, &fat, kernel_cluster, kernel)?;
    Ok(fs)
}

fn read_input(path: &Path, what: &str) -> Result<Vec<u8>, String> {
    let meta = fs::metadata(path).map_err(|e| format!("{what}: {e}"))?;
    if !meta.is_file() || meta.len() == 0 || meta.len() > MAX_INPUT_BYTES as u64 {
        return Err(format!(
            "{what} must be a non-empty regular file <= {MAX_INPUT_BYTES} bytes"
        ));
    }
    fs::read(path).map_err(|e| format!("{what}: {e}"))
}

fn populate(
    image: &Path,
    boot_path: &Path,
    kernel_path: &Path,
    sector_size: u64,
) -> Result<(usize, usize), String> {
    if image.extension().is_none_or(|ext| ext != "img") {
        return Err("image must have .img extension".into());
    }
    if fs::symlink_metadata(image)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("refusing symlink image".into());
    }
    let canonical = image.canonicalize().map_err(|e| e.to_string())?;
    if ["/dev", "/proc", "/sys"]
        .iter()
        .any(|root| canonical.starts_with(root))
    {
        return Err("refusing device or pseudo-filesystem image".into());
    }
    let boot = read_input(boot_path, "BOOTX64.EFI")?;
    let kernel = read_input(kernel_path, "kernel.elf")?;
    if !boot.starts_with(b"MZ") {
        return Err("BOOTX64.EFI is not a PE/COFF image".into());
    }
    if !kernel.starts_with(b"\x7fELF") {
        return Err("kernel.elf is not an ELF image".into());
    }
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(image)
        .map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("refusing non-regular image".into());
    }
    let esp = find_esp(&mut file, sector_size)?;
    let start = esp
        .first_lba
        .checked_mul(sector_size)
        .ok_or("ESP offset overflow")?;
    let length = esp
        .sectors
        .checked_mul(sector_size)
        .ok_or("ESP length overflow")?;
    file.seek(SeekFrom::Start(start))
        .map_err(|e| e.to_string())?;
    let mut buffer = vec![0u8; 1024 * 1024];
    let mut remaining = length;
    while remaining != 0 {
        let take = usize::try_from(remaining.min(buffer.len() as u64)).unwrap();
        file.read_exact(&mut buffer[..take])
            .map_err(|e| e.to_string())?;
        if buffer[..take].iter().any(|&b| b != 0) {
            return Err("ESP is not blank; refusing overwrite".into());
        }
        remaining -= take as u64;
    }
    let fs = build_fat16(esp, &boot, &kernel)?;
    if fs.len() as u64 != length {
        return Err("internal FAT image length mismatch".into());
    }
    file.seek(SeekFrom::Start(start))
        .map_err(|e| e.to_string())?;
    file.write_all(&fs).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    Ok((boot.len(), kernel.len()))
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if !(4..=5).contains(&args.len()) {
        eprintln!("usage: populate-esp <image.img> <BOOTX64.EFI> <kernel.elf> [512|4096]");
        eprintln!("Writes only a blank 32 MiB ESP in an existing regular-file Vibrix GPT image.");
        std::process::exit(2);
    }
    let sector_size = args
        .get(4)
        .map(String::as_str)
        .unwrap_or("512")
        .parse()
        .unwrap_or(0);
    match populate(
        Path::new(&args[1]),
        Path::new(&args[2]),
        Path::new(&args[3]),
        sector_size,
    ) {
        Ok((boot, kernel)) => {
            println!("Populated FAT16 ESP: BOOTX64.EFI={boot} bytes kernel.elf={kernel} bytes")
        }
        Err(error) => {
            eprintln!("could not populate ESP: {error}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fat16_geometry_is_valid_for_both_supported_sector_sizes() {
        for sector_size in [512, 4096] {
            let esp = Esp {
                first_lba: MIB / sector_size,
                sectors: ESP_BYTES / sector_size,
                sector_size,
            };
            let p = fat_layout(esp).unwrap();
            assert!((4085..=65524).contains(&p.cluster_count));
            assert_eq!(
                p.first_data_sector,
                1 + 2 * p.fat_sectors + p.root_dir_sectors
            );
        }
    }

    #[test]
    fn builds_exact_short_name_tree_and_matching_fats() {
        let esp = Esp {
            first_lba: 2048,
            sectors: ESP_BYTES / 512,
            sector_size: 512,
        };
        let mut boot = b"MZ".to_vec();
        boot.resize(900, 0x42);
        let mut kernel = b"\x7fELF".to_vec();
        kernel.resize(1800, 0x24);
        let fs = build_fat16(esp, &boot, &kernel).unwrap();
        let p = fat_layout(esp).unwrap();
        assert_eq!(&fs[510..512], &[0x55, 0xaa]);
        assert_eq!(&fs[43..54], b"VIBRIX ESP ");
        assert_eq!(&fs[54..62], b"FAT16   ");
        let fat_bytes = p.fat_sectors * p.bytes_per_sector;
        let a = p.bytes_per_sector;
        let b = a + fat_bytes;
        assert_eq!(&fs[a..a + fat_bytes], &fs[b..b + fat_bytes]);
        let root = p.first_root_sector * p.bytes_per_sector;
        assert_eq!(&fs[root + 32..root + 43], b"EFI        ");
        assert_eq!(&fs[root + 64..root + 75], b"VIBRIX     ");
        let boot_dir = cluster_offset(p, 3).unwrap();
        assert_eq!(&fs[boot_dir + 64..boot_dir + 75], b"BOOTX64 EFI");
        let vibrix = cluster_offset(p, 4).unwrap();
        assert_eq!(&fs[vibrix + 64..vibrix + 75], b"KERNEL  ELF");
    }
}
