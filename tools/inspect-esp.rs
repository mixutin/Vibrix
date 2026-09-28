//! Read-only validator for the adopted Vibrix EFI System Partition layout.
//! It independently checks the FAT16 BPB, both FAT copies, short-name tree,
//! file chains and the PE/ELF signatures expected by the UEFI loader contract.

use std::env;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const MIB: u64 = 1024 * 1024;
const ESP_BYTES: u64 = 32 * MIB;
const ESP_TYPE: [u8; 16] = [
    0x28, 0x73, 0x2a, 0xc1, 0x1f, 0xf8, 0xd2, 0x11, 0xba, 0x4b, 0x00, 0xa0, 0xc9, 0x3e, 0xc9, 0x3b,
];
const MEDIA: u8 = 0xf8;

#[derive(Clone, Copy, Debug)]
struct Esp {
    first_lba: u64,
    sectors: u64,
    sector_size: u64,
}

#[derive(Clone, Copy, Debug)]
struct Fat16 {
    start: u64,
    bytes_per_sector: usize,
    fat_start: u64,
    fat_bytes: usize,
    root_start: u64,
    root_bytes: usize,
    data_start: u64,
    cluster_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Entry {
    attr: u8,
    cluster: u16,
    size: u32,
}

fn rd_u16(bytes: &[u8], at: usize) -> Result<u16, String> {
    Ok(u16::from_le_bytes(
        bytes
            .get(at..at + 2)
            .ok_or("truncated u16")?
            .try_into()
            .unwrap(),
    ))
}

fn rd_u32(bytes: &[u8], at: usize) -> Result<u32, String> {
    Ok(u32::from_le_bytes(
        bytes
            .get(at..at + 4)
            .ok_or("truncated u32")?
            .try_into()
            .unwrap(),
    ))
}

fn rd_u64(bytes: &[u8], at: usize) -> Result<u64, String> {
    Ok(u64::from_le_bytes(
        bytes
            .get(at..at + 8)
            .ok_or("truncated u64")?
            .try_into()
            .unwrap(),
    ))
}

fn read_at(file: &mut File, offset: u64, len: usize) -> Result<Vec<u8>, String> {
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| e.to_string())?;
    let mut bytes = vec![0u8; len];
    file.read_exact(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes)
}

fn locate_esp(file: &mut File, sector_size: u64) -> Result<Esp, String> {
    if !matches!(sector_size, 512 | 4096) {
        return Err("sector size must be 512 or 4096".into());
    }
    let len = file.metadata().map_err(|e| e.to_string())?.len();
    if len < 64 * MIB || len % sector_size != 0 {
        return Err("invalid image geometry".into());
    }
    let mbr = read_at(file, 0, 512)?;
    if mbr[510..512] != [0x55, 0xaa] || mbr[450] != 0xee {
        return Err("invalid Protective MBR".into());
    }
    let header = read_at(file, sector_size, sector_size as usize)?;
    if header.get(..8) != Some(b"EFI PART") || rd_u64(&header, 24)? != 1 {
        return Err("invalid primary GPT header".into());
    }
    let entries_lba = rd_u64(&header, 72)?;
    let count = rd_u32(&header, 80)? as usize;
    let entry_size = rd_u32(&header, 84)? as usize;
    if entries_lba != 2 || count == 0 || count > 128 || !(128..=4096).contains(&entry_size) {
        return Err("unsupported GPT entry array".into());
    }
    let entries = read_at(
        file,
        entries_lba
            .checked_mul(sector_size)
            .ok_or("entry offset overflow")?,
        count
            .checked_mul(entry_size)
            .ok_or("entry array overflow")?,
    )?;
    let mut found = None;
    for entry in entries.chunks_exact(entry_size) {
        if entry[..16] != ESP_TYPE {
            continue;
        }
        if found.is_some() {
            return Err("multiple ESPs".into());
        }
        let first = rd_u64(entry, 32)?;
        let last = rd_u64(entry, 40)?;
        let sectors = last
            .checked_sub(first)
            .and_then(|n| n.checked_add(1))
            .ok_or("invalid ESP extent")?;
        if first
            .checked_mul(sector_size)
            .ok_or("ESP offset overflow")?
            != MIB
            || sectors
                .checked_mul(sector_size)
                .ok_or("ESP length overflow")?
                != ESP_BYTES
        {
            return Err("unexpected ESP extent".into());
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
    found.ok_or_else(|| "ESP not found".into())
}

fn open_fat16(file: &mut File, esp: Esp) -> Result<(Fat16, Vec<u8>), String> {
    let start = esp
        .first_lba
        .checked_mul(esp.sector_size)
        .ok_or("ESP offset overflow")?;
    let boot = read_at(file, start, esp.sector_size as usize)?;
    if boot[510..512] != [0x55, 0xaa]
        || boot.get(3..11) != Some(b"VIBRIX  ")
        || boot.get(43..54) != Some(b"VIBRIX ESP ")
        || boot.get(54..62) != Some(b"FAT16   ")
    {
        return Err("unexpected FAT16 boot-sector identity".into());
    }
    let bps = usize::from(rd_u16(&boot, 11)?);
    let spc = boot[13];
    let reserved = usize::from(rd_u16(&boot, 14)?);
    let fats = usize::from(boot[16]);
    let root_entries = usize::from(rd_u16(&boot, 17)?);
    let total16 = usize::from(rd_u16(&boot, 19)?);
    let media = boot[21];
    let fat_sectors = usize::from(rd_u16(&boot, 22)?);
    let hidden = u64::from(rd_u32(&boot, 28)?);
    let total32 = rd_u32(&boot, 32)? as usize;
    if bps != esp.sector_size as usize
        || spc != 1
        || reserved != 1
        || fats != 2
        || root_entries != 512
        || media != MEDIA
        || fat_sectors == 0
        || hidden != esp.first_lba
        || boot[38] != 0x29
    {
        return Err("unsupported FAT16 BPB".into());
    }
    let total = if total16 != 0 { total16 } else { total32 };
    if total != esp.sectors as usize {
        return Err("FAT volume length does not match GPT ESP".into());
    }
    let root_bytes = root_entries
        .checked_mul(32)
        .ok_or("root directory overflow")?;
    let root_sectors = root_bytes.div_ceil(bps);
    let overhead = reserved
        .checked_add(fats * fat_sectors)
        .and_then(|n| n.checked_add(root_sectors))
        .ok_or("FAT geometry overflow")?;
    let cluster_count = total.checked_sub(overhead).ok_or("invalid FAT geometry")?;
    if !(4085..=65524).contains(&cluster_count) {
        return Err("cluster count is not FAT16".into());
    }
    let fat_start = start + (reserved * bps) as u64;
    let fat_bytes = fat_sectors
        .checked_mul(bps)
        .ok_or("FAT byte length overflow")?;
    let first = read_at(file, fat_start, fat_bytes)?;
    let second = read_at(file, fat_start + fat_bytes as u64, fat_bytes)?;
    if first != second {
        return Err("FAT copies differ".into());
    }
    if rd_u16(&first, 0)? != 0xff00 | u16::from(MEDIA) || rd_u16(&first, 2)? < 0xfff8 {
        return Err("invalid FAT reserved entries".into());
    }
    let root_start = start + ((reserved + fats * fat_sectors) * bps) as u64;
    let data_start = start + ((overhead) * bps) as u64;
    Ok((
        Fat16 {
            start,
            bytes_per_sector: bps,
            fat_start,
            fat_bytes,
            root_start,
            root_bytes: root_sectors * bps,
            data_start,
            cluster_count,
        },
        first,
    ))
}

fn find_entry(dir: &[u8], name: &[u8; 11]) -> Result<Entry, String> {
    let mut found = None;
    for raw in dir.chunks_exact(32) {
        if raw[0] == 0 {
            break;
        }
        if raw[0] == 0xe5 {
            continue;
        }
        if raw[11] == 0x0f {
            return Err("long filename entries are outside the adopted ESP layout".into());
        }
        if &raw[..11] != name {
            continue;
        }
        if found.is_some() {
            return Err("duplicate short-name entry".into());
        }
        if rd_u16(raw, 20)? != 0 {
            return Err("unexpected high cluster bits in FAT16 entry".into());
        }
        found = Some(Entry {
            attr: raw[11],
            cluster: rd_u16(raw, 26)?,
            size: rd_u32(raw, 28)?,
        });
    }
    found.ok_or_else(|| {
        format!(
            "missing FAT entry {}",
            String::from_utf8_lossy(name).trim_end()
        )
    })
}

fn fat_entry(fat: &[u8], cluster: u16) -> Result<u16, String> {
    rd_u16(fat, usize::from(cluster) * 2)
}

fn cluster_offset(layout: Fat16, cluster: u16) -> Result<u64, String> {
    if cluster < 2 || usize::from(cluster) >= layout.cluster_count + 2 {
        return Err("cluster outside FAT data region".into());
    }
    layout
        .data_start
        .checked_add(
            (usize::from(cluster) - 2)
                .checked_mul(layout.bytes_per_sector)
                .ok_or("cluster offset overflow")? as u64,
        )
        .ok_or_else(|| "cluster offset overflow".into())
}

fn read_directory(
    file: &mut File,
    layout: Fat16,
    fat: &[u8],
    cluster: u16,
) -> Result<Vec<u8>, String> {
    if fat_entry(fat, cluster)? < 0xfff8 {
        return Err("adopted ESP directory unexpectedly spans multiple clusters".into());
    }
    read_at(
        file,
        cluster_offset(layout, cluster)?,
        layout.bytes_per_sector,
    )
}

fn read_file_chain(
    file: &mut File,
    layout: Fat16,
    fat: &[u8],
    entry: Entry,
) -> Result<Vec<u8>, String> {
    if entry.attr != 0x20 || entry.cluster < 2 || entry.size == 0 {
        return Err("invalid regular-file entry".into());
    }
    let mut output = Vec::with_capacity(entry.size as usize);
    let mut cluster = entry.cluster;
    let mut seen = 0usize;
    while output.len() < entry.size as usize {
        seen += 1;
        if seen > layout.cluster_count {
            return Err("FAT chain cycle".into());
        }
        let block = read_at(
            file,
            cluster_offset(layout, cluster)?,
            layout.bytes_per_sector,
        )?;
        let needed = entry.size as usize - output.len();
        output.extend_from_slice(&block[..needed.min(block.len())]);
        let next = fat_entry(fat, cluster)?;
        if output.len() == entry.size as usize {
            if next < 0xfff8 {
                return Err("file chain has trailing clusters".into());
            }
            break;
        }
        if next < 2 || next >= 0xfff8 {
            return Err("file chain terminates early".into());
        }
        cluster = next;
    }
    Ok(output)
}

fn inspect(path: &Path, sector_size: u64) -> Result<(usize, usize), String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_file() {
        return Err("image is not a regular file".into());
    }
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let esp = locate_esp(&mut file, sector_size)?;
    let (fat16, fat) = open_fat16(&mut file, esp)?;

    // Numeric fields are kept in the struct to make the independently checked
    // on-disk extents explicit even though only the directory/data offsets
    // are needed below.
    let _ = (fat16.start, fat16.fat_start, fat16.fat_bytes);

    let root = read_at(&mut file, fat16.root_start, fat16.root_bytes)?;
    let label = find_entry(&root, b"VIBRIX ESP ")?;
    if label.attr != 0x08 || label.cluster != 0 || label.size != 0 {
        return Err("invalid root volume-label entry".into());
    }
    let efi = find_entry(&root, b"EFI        ")?;
    let vibrix = find_entry(&root, b"VIBRIX     ")?;
    if efi.attr != 0x10 || vibrix.attr != 0x10 {
        return Err("required root entries are not directories".into());
    }

    let efi_dir = read_directory(&mut file, fat16, &fat, efi.cluster)?;
    let boot_dir_entry = find_entry(&efi_dir, b"BOOT       ")?;
    if boot_dir_entry.attr != 0x10 {
        return Err("EFI/BOOT is not a directory".into());
    }
    let boot_dir = read_directory(&mut file, fat16, &fat, boot_dir_entry.cluster)?;
    let boot_entry = find_entry(&boot_dir, b"BOOTX64 EFI")?;

    let vibrix_dir = read_directory(&mut file, fat16, &fat, vibrix.cluster)?;
    let kernel_entry = find_entry(&vibrix_dir, b"KERNEL  ELF")?;

    let boot = read_file_chain(&mut file, fat16, &fat, boot_entry)?;
    let kernel = read_file_chain(&mut file, fat16, &fat, kernel_entry)?;
    if !boot.starts_with(b"MZ") {
        return Err("EFI/BOOT/BOOTX64.EFI is not PE/COFF".into());
    }
    if !kernel.starts_with(b"\x7fELF") {
        return Err("vibrix/kernel.elf is not ELF".into());
    }
    Ok((boot.len(), kernel.len()))
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if !(2..=3).contains(&args.len()) {
        eprintln!("usage: inspect-esp <image.img> [512|4096]");
        std::process::exit(2);
    }
    let sector_size = args
        .get(2)
        .map(String::as_str)
        .unwrap_or("512")
        .parse()
        .unwrap_or(0);
    match inspect(Path::new(&args[1]), sector_size) {
        Ok((boot, kernel)) => {
            println!(
                "Valid Vibrix ESP: /EFI/BOOT/BOOTX64.EFI={boot} bytes /vibrix/kernel.elf={kernel} bytes"
            );
        }
        Err(error) => {
            eprintln!("invalid Vibrix ESP: {error}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_entry_parser_is_exact_and_rejects_lfn() {
        let mut dir = [0u8; 96];
        dir[..11].copy_from_slice(b"EFI        ");
        dir[11] = 0x10;
        dir[26..28].copy_from_slice(&2u16.to_le_bytes());
        assert_eq!(
            find_entry(&dir, b"EFI        ").unwrap(),
            Entry {
                attr: 0x10,
                cluster: 2,
                size: 0
            }
        );
        assert!(find_entry(&dir, b"BOOT       ").is_err());

        let mut lfn = [0u8; 32];
        lfn[0] = 1;
        lfn[11] = 0x0f;
        assert!(find_entry(&lfn, b"ANY        ").is_err());
    }

    #[test]
    fn fat_entry_reads_little_endian_cluster_links() {
        let fat = [0xf8, 0xff, 0xff, 0xff, 7, 0, 0xff, 0xff];
        assert_eq!(fat_entry(&fat, 0).unwrap(), 0xfff8);
        assert_eq!(fat_entry(&fat, 1).unwrap(), 0xffff);
        assert_eq!(fat_entry(&fat, 2).unwrap(), 7);
        assert_eq!(fat_entry(&fat, 3).unwrap(), 0xffff);
    }
}
