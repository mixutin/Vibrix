//! Host-only regular-file VibrixFS v1 formatter and inspector.
//!
//! This tool deliberately refuses paths that look like raw devices and the
//! formatter uses create_new: it cannot overwrite an existing image. Recovery
//! is limited to regular-file development images; this is not a raw-device
//! provisioner or a kernel filesystem implementation.

use std::env;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

#[allow(dead_code)]
#[path = "vibrixfs-wire.rs"]
mod wire;
#[allow(dead_code)]
#[path = "vibrixfs-journal.rs"]
mod journal;

use wire::{
    BLOCK, Extent, INODE_BYTES, Inode, Range, Superblock, encode_dir_record, encode_inode,
    encode_superblock, immutable_superblock_fields_match, parse_dir_record, parse_inode,
    parse_superblock,
};

const TOTAL_INODES: u64 = 1024;
const INODE_BITMAP_BLOCKS: u64 = 1;
const INODE_TABLE_BLOCKS: u64 = (TOTAL_INODES * INODE_BYTES as u64).div_ceil(BLOCK as u64);
const JOURNAL_BLOCKS: u64 = 66; // manifest + 64 full-block after-images + commit.
const MAX_FORMAT_BLOCKS: u64 = 262_144; // 1 GiB early host-tool bound.
const DIRECTORY_BYTES: usize = 80;
const WELCOME_NAME: &str = "welcome.txt";
const WELCOME_BYTES: &[u8] = b"Welcome to VibrixFS.\n";
const ROOT_MODE: u16 = 0o755;
const WELCOME_MODE: u16 = 0o640;
const WELCOME_UID: u32 = 1000;
const WELCOME_GID: u32 = 1000;
const ROOT_TIME_SEC: i64 = 1_700_000_000;
const WELCOME_ATIME_SEC: i64 = 1_700_000_101;
const WELCOME_MTIME_SEC: i64 = 1_700_000_202;
const WELCOME_CTIME_SEC: i64 = 1_700_000_303;
const WELCOME_ATIME_NSEC: u32 = 123_456_789;
const WELCOME_MTIME_NSEC: u32 = 234_567_890;
const WELCOME_CTIME_NSEC: u32 = 345_678_901;

fn raw_device_like(path: &Path) -> bool {
    let s = path.to_string_lossy();
    s.starts_with("/dev/")
        || s.starts_with("\\\\.\\")
        || s.starts_with("\\\\?\\GLOBALROOT")
        || s.starts_with("/proc/")
        || s.starts_with("/sys/")
}

fn hex16(raw: &str) -> Result<[u8; 16], String> {
    let compact: String = raw.chars().filter(|&c| c != '-').collect();
    if compact.len() != 32 || !compact.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("identity must be 16 bytes encoded as 32 hexadecimal digits".into());
    }
    let mut out = [0u8; 16];
    for (index, byte) in out.iter_mut().enumerate() {
        let at = index * 2;
        *byte = u8::from_str_radix(&compact[at..at + 2], 16)
            .map_err(|_| "invalid hexadecimal identity".to_string())?;
    }
    if out.iter().all(|&byte| byte == 0) {
        return Err("identity must be nonzero".into());
    }
    Ok(out)
}

fn sector_size(raw: &str) -> Result<u64, String> {
    match raw.parse::<u64>() {
        Ok(512) => Ok(512),
        Ok(4096) => Ok(4096),
        _ => Err("logical sector size must be 512 or 4096".into()),
    }
}

fn byte_offset(block: u64) -> Result<u64, String> {
    block
        .checked_mul(BLOCK as u64)
        .ok_or_else(|| "filesystem byte offset overflow".into())
}

fn write_at(file: &mut File, block: u64, bytes: &[u8]) -> Result<(), String> {
    file.seek(SeekFrom::Start(byte_offset(block)?))
        .map_err(|e| format!("seek block {block}: {e}"))?;
    file.write_all(bytes)
        .map_err(|e| format!("write block {block}: {e}"))
}

fn read_at(file: &mut File, block: u64, bytes: &mut [u8]) -> Result<(), String> {
    file.seek(SeekFrom::Start(byte_offset(block)?))
        .map_err(|e| format!("seek block {block}: {e}"))?;
    file.read_exact(bytes)
        .map_err(|e| format!("read block {block}: {e}"))
}

fn set_bit(bitmap: &mut [u8], bit: u64) -> Result<(), String> {
    let byte = usize::try_from(bit / 8).map_err(|_| "bitmap index overflow")?;
    let shift = u8::try_from(bit % 8).map_err(|_| "bitmap bit overflow")?;
    let slot = bitmap
        .get_mut(byte)
        .ok_or_else(|| "bitmap index outside metadata".to_string())?;
    *slot |= 1u8 << shift;
    Ok(())
}

fn bit_is_set(bitmap: &[u8], bit: u64) -> Result<bool, String> {
    let byte = usize::try_from(bit / 8).map_err(|_| "bitmap index overflow")?;
    let shift = u8::try_from(bit % 8).map_err(|_| "bitmap bit overflow")?;
    let slot = *bitmap
        .get(byte)
        .ok_or_else(|| "bitmap index outside metadata".to_string())?;
    Ok(slot & (1u8 << shift) != 0)
}

fn set_padding_bits(bitmap: &mut [u8], first_padding: u64) -> Result<(), String> {
    let total_bits = u64::try_from(bitmap.len())
        .map_err(|_| "bitmap byte length overflow")?
        .checked_mul(8)
        .ok_or_else(|| "bitmap bit length overflow".to_string())?;
    if first_padding > total_bits {
        return Err("bitmap capacity smaller than declared object count".into());
    }
    for bit in first_padding..total_bits {
        set_bit(bitmap, bit)?;
    }
    Ok(())
}

fn require_padding_bits(bitmap: &[u8], first_padding: u64) -> Result<(), String> {
    let total_bits = u64::try_from(bitmap.len())
        .map_err(|_| "bitmap byte length overflow")?
        .checked_mul(8)
        .ok_or_else(|| "bitmap bit length overflow".to_string())?;
    if first_padding > total_bits {
        return Err("bitmap capacity smaller than declared object count".into());
    }
    for bit in first_padding..total_bits {
        if !bit_is_set(bitmap, bit)? {
            return Err(format!("bitmap padding bit {bit} is clear"));
        }
    }
    Ok(())
}

fn root_directory_block() -> Result<[u8; BLOCK], String> {
    let mut block = [0u8; BLOCK];
    let mut offset = 0usize;
    for (inode, file_type, name) in [(1, 2, "."), (1, 2, ".."), (2, 1, WELCOME_NAME)] {
        let used = encode_dir_record(&mut block[offset..], inode, file_type, name)
            .map_err(|e| format!("encode directory record: {e:?}"))?;
        offset = offset
            .checked_add(used)
            .ok_or_else(|| "directory length overflow".to_string())?;
    }
    if offset != DIRECTORY_BYTES {
        return Err("unexpected root directory framing".into());
    }
    Ok(block)
}

fn validate_root_directory(block: &[u8; BLOCK], size: u64) -> Result<(), String> {
    let size = usize::try_from(size).map_err(|_| "directory size overflow")?;
    if size != DIRECTORY_BYTES || size > block.len() {
        return Err("base-v1 root directory shape is invalid".into());
    }
    let expected = [(1u64, 2u8, "."), (1, 2, ".."), (2, 1, WELCOME_NAME)];
    let mut offset = 0usize;
    for (inode, file_type, name) in expected {
        let (entry, used) = parse_dir_record(&block[offset..size], TOTAL_INODES)
            .map_err(|e| format!("directory record: {e:?}"))?;
        if entry.inode != inode || entry.file_type != file_type || entry.name != name {
            return Err("root directory entry identity/type/name mismatch".into());
        }
        offset = offset
            .checked_add(used)
            .ok_or_else(|| "directory offset overflow".to_string())?;
    }
    if offset != size {
        return Err("unexpected trailing root directory records".into());
    }
    Ok(())
}

fn layout(
    total_blocks: u64,
    filesystem_uuid: [u8; 16],
    root_guid: [u8; 16],
) -> Result<(Superblock, u64), String> {
    if !(4096..=MAX_FORMAT_BLOCKS).contains(&total_blocks) {
        return Err(format!(
            "early formatter supports 4096..={MAX_FORMAT_BLOCKS} filesystem blocks"
        ));
    }
    let bitmap_bits = (BLOCK as u64)
        .checked_mul(8)
        .ok_or_else(|| "bitmap geometry overflow".to_string())?;
    let block_bitmap_blocks = total_blocks.div_ceil(bitmap_bits);
    let block_bitmap = Range {
        start: 1,
        blocks: block_bitmap_blocks,
    };
    let inode_bitmap = Range {
        start: block_bitmap
            .start
            .checked_add(block_bitmap.blocks)
            .ok_or_else(|| "metadata geometry overflow".to_string())?,
        blocks: INODE_BITMAP_BLOCKS,
    };
    let inode_table = Range {
        start: inode_bitmap
            .start
            .checked_add(inode_bitmap.blocks)
            .ok_or_else(|| "metadata geometry overflow".to_string())?,
        blocks: INODE_TABLE_BLOCKS,
    };
    let journal = Range {
        start: inode_table
            .start
            .checked_add(inode_table.blocks)
            .ok_or_else(|| "metadata geometry overflow".to_string())?,
        blocks: JOURNAL_BLOCKS,
    };
    let root_data = journal
        .start
        .checked_add(journal.blocks)
        .ok_or_else(|| "journal geometry overflow".to_string())?;
    let welcome_data = root_data
        .checked_add(1)
        .ok_or_else(|| "data geometry overflow".to_string())?;
    if welcome_data >= total_blocks.saturating_sub(1) {
        return Err("filesystem too small for base-v1 metadata".into());
    }
    let _ = welcome_data;
    Ok((
        Superblock {
            minor: 0,
            clean: true,
            generation: 1,
            total_blocks,
            total_inodes: TOTAL_INODES,
            block_bitmap,
            inode_bitmap,
            inode_table,
            journal: Some(journal),
            filesystem_uuid,
            root_partition_guid: root_guid,
        },
        root_data,
    ))
}

fn format_image(
    path: &Path,
    total_blocks: u64,
    logical_sector: u64,
    filesystem_uuid: [u8; 16],
    root_guid: [u8; 16],
) -> Result<(), String> {
    if BLOCK as u64 % logical_sector != 0 {
        return Err("filesystem block is not an integer number of logical sectors".into());
    }
    if raw_device_like(path) {
        return Err("refusing a path that looks like a raw device".into());
    }
    let (sb, root_data) = layout(total_blocks, filesystem_uuid, root_guid)?;
    let welcome_data = root_data
        .checked_add(1)
        .ok_or_else(|| "data geometry overflow".to_string())?;
    let bytes = total_blocks
        .checked_mul(BLOCK as u64)
        .ok_or_else(|| "image length overflow".to_string())?;
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("refusing to overwrite/create image: {e}"))?;
    file.set_len(bytes)
        .map_err(|e| format!("size image: {e}"))?;

    let superblock = encode_superblock(&sb, total_blocks, &root_guid)
        .map_err(|e| format!("encode superblock: {e:?}"))?;

    let block_bitmap_len = usize::try_from(
        sb.block_bitmap
            .blocks
            .checked_mul(BLOCK as u64)
            .ok_or_else(|| "block bitmap length overflow".to_string())?,
    )
    .map_err(|_| "block bitmap too large for host")?;
    let mut block_bitmap = vec![0u8; block_bitmap_len];
    set_bit(&mut block_bitmap, 0)?;
    for block in sb.block_bitmap.start..sb.block_bitmap.start + sb.block_bitmap.blocks {
        set_bit(&mut block_bitmap, block)?;
    }
    for block in sb.inode_bitmap.start..sb.inode_bitmap.start + sb.inode_bitmap.blocks {
        set_bit(&mut block_bitmap, block)?;
    }
    for block in sb.inode_table.start..sb.inode_table.start + sb.inode_table.blocks {
        set_bit(&mut block_bitmap, block)?;
    }
    let journal = sb
        .journal
        .ok_or_else(|| "formatter requires journal-enabled superblock".to_string())?;
    for block in journal.start..journal.start + journal.blocks {
        set_bit(&mut block_bitmap, block)?;
    }
    set_bit(&mut block_bitmap, root_data)?;
    set_bit(&mut block_bitmap, welcome_data)?;
    set_bit(&mut block_bitmap, total_blocks - 1)?;
    set_padding_bits(&mut block_bitmap, total_blocks)?;

    let mut inode_bitmap = vec![0u8; BLOCK * INODE_BITMAP_BLOCKS as usize];
    set_bit(&mut inode_bitmap, 0)?;
    set_bit(&mut inode_bitmap, 1)?;
    set_bit(&mut inode_bitmap, 2)?;
    set_padding_bits(&mut inode_bitmap, TOTAL_INODES + 1)?;

    let mut extents = [Extent {
        start: 0,
        blocks: 0,
    }; 6];
    extents[0] = Extent {
        start: root_data,
        blocks: 1,
    };
    let root = Inode {
        number: 1,
        file_type: 2,
        mode: ROOT_MODE,
        uid: 0,
        gid: 0,
        links: 2,
        size: DIRECTORY_BYTES as u64,
        allocated_blocks: 1,
        atime_sec: ROOT_TIME_SEC,
        atime_nsec: 0,
        mtime_sec: ROOT_TIME_SEC,
        mtime_nsec: 0,
        ctime_sec: ROOT_TIME_SEC,
        ctime_nsec: 0,
        nonce: [0x5a; 16],
        extents,
        extent_count: 1,
        device: 0,
        flags: 0,
    };
    let encoded_root = encode_inode(&root, &sb).map_err(|e| format!("encode root inode: {e:?}"))?;
    let mut welcome_extents = [Extent {
        start: 0,
        blocks: 0,
    }; 6];
    welcome_extents[0] = Extent {
        start: welcome_data,
        blocks: 1,
    };
    let welcome = Inode {
        number: 2,
        file_type: 1,
        mode: WELCOME_MODE,
        uid: WELCOME_UID,
        gid: WELCOME_GID,
        links: 1,
        size: WELCOME_BYTES.len() as u64,
        allocated_blocks: 1,
        atime_sec: WELCOME_ATIME_SEC,
        atime_nsec: WELCOME_ATIME_NSEC,
        mtime_sec: WELCOME_MTIME_SEC,
        mtime_nsec: WELCOME_MTIME_NSEC,
        ctime_sec: WELCOME_CTIME_SEC,
        ctime_nsec: WELCOME_CTIME_NSEC,
        nonce: [0x6b; 16],
        extents: welcome_extents,
        extent_count: 1,
        device: 0,
        flags: 0,
    };
    let encoded_welcome =
        encode_inode(&welcome, &sb).map_err(|e| format!("encode welcome inode: {e:?}"))?;
    let inode_table_len = usize::try_from(
        sb.inode_table
            .blocks
            .checked_mul(BLOCK as u64)
            .ok_or_else(|| "inode table length overflow".to_string())?,
    )
    .map_err(|_| "inode table too large for host")?;
    let mut inode_table = vec![0u8; inode_table_len];
    inode_table[..INODE_BYTES].copy_from_slice(&encoded_root);
    inode_table[INODE_BYTES..INODE_BYTES * 2].copy_from_slice(&encoded_welcome);
    let directory = root_directory_block()?;
    let mut welcome_block = [0u8; BLOCK];
    welcome_block[..WELCOME_BYTES.len()].copy_from_slice(WELCOME_BYTES);

    write_at(&mut file, 0, &superblock)?;
    write_at(&mut file, sb.block_bitmap.start, &block_bitmap)?;
    write_at(&mut file, sb.inode_bitmap.start, &inode_bitmap)?;
    write_at(&mut file, sb.inode_table.start, &inode_table)?;
    write_at(&mut file, root_data, &directory)?;
    write_at(&mut file, welcome_data, &welcome_block)?;
    write_at(&mut file, total_blocks - 1, &superblock)?;
    file.sync_all().map_err(|e| format!("sync image: {e}"))?;
    Ok(())
}

fn read_region(file: &mut File, start: u64, blocks: u64) -> Result<Vec<u8>, String> {
    let bytes = usize::try_from(
        blocks
            .checked_mul(BLOCK as u64)
            .ok_or_else(|| "region byte length overflow".to_string())?,
    )
    .map_err(|_| "region too large for host")?;
    let mut out = vec![0u8; bytes];
    read_at(file, start, &mut out)?;
    Ok(out)
}

fn require_allocated_range(bitmap: &[u8], range: Range) -> Result<(), String> {
    let end = range
        .start
        .checked_add(range.blocks)
        .ok_or_else(|| "metadata range overflow".to_string())?;
    for block in range.start..end {
        if !bit_is_set(bitmap, block)? {
            return Err(format!("required filesystem block {block} is marked free"));
        }
    }
    Ok(())
}

fn inspect_image(path: &Path, logical_sector: u64, root_guid: [u8; 16]) -> Result<(), String> {
    if BLOCK as u64 % logical_sector != 0 {
        return Err("filesystem block is not an integer number of logical sectors".into());
    }
    if raw_device_like(path) {
        return Err("refusing a path that looks like a raw device".into());
    }
    let mut file = File::open(path).map_err(|e| format!("open image: {e}"))?;
    let meta = file.metadata().map_err(|e| format!("stat image: {e}"))?;
    if !meta.file_type().is_file() || meta.len() % BLOCK as u64 != 0 {
        return Err("input must be a regular file with a 4096-byte-aligned length".into());
    }
    let partition_blocks = meta.len() / BLOCK as u64;
    if partition_blocks < 4096 {
        return Err("filesystem image is below the v1 minimum size".into());
    }

    let mut primary = [0u8; BLOCK];
    let mut secondary = [0u8; BLOCK];
    read_at(&mut file, 0, &mut primary)?;
    read_at(&mut file, partition_blocks - 1, &mut secondary)?;
    let sb = parse_superblock(&primary, partition_blocks, &root_guid)
        .map_err(|e| format!("primary superblock: {e:?}"))?;
    let backup = parse_superblock(&secondary, partition_blocks, &root_guid)
        .map_err(|e| format!("secondary superblock: {e:?}"))?;
    if !immutable_superblock_fields_match(&sb, &backup) || primary != secondary {
        return Err("primary/secondary superblocks do not exactly agree".into());
    }
    if sb.total_blocks != partition_blocks {
        return Err("filesystem total_blocks does not match regular-file extent".into());
    }

    let block_bitmap = read_region(&mut file, sb.block_bitmap.start, sb.block_bitmap.blocks)?;
    require_allocated_range(
        &block_bitmap,
        Range {
            start: 0,
            blocks: 1,
        },
    )?;
    require_allocated_range(&block_bitmap, sb.block_bitmap)?;
    require_allocated_range(&block_bitmap, sb.inode_bitmap)?;
    require_allocated_range(&block_bitmap, sb.inode_table)?;
    let journal = sb
        .journal
        .ok_or_else(|| "journal-enabled VibrixFS image required".to_string())?;
    require_allocated_range(&block_bitmap, journal)?;
    require_allocated_range(
        &block_bitmap,
        Range {
            start: sb.total_blocks - 1,
            blocks: 1,
        },
    )?;
    require_padding_bits(&block_bitmap, sb.total_blocks)?;

    let inode_bitmap = read_region(&mut file, sb.inode_bitmap.start, sb.inode_bitmap.blocks)?;
    if !bit_is_set(&inode_bitmap, 0)?
        || !bit_is_set(&inode_bitmap, 1)?
        || !bit_is_set(&inode_bitmap, 2)?
    {
        return Err("reserved/root/welcome inode bitmap bits are not allocated".into());
    }
    require_padding_bits(&inode_bitmap, sb.total_inodes + 1)?;

    let inode_table = read_region(&mut file, sb.inode_table.start, sb.inode_table.blocks)?;
    let root = parse_inode(
        inode_table
            .get(..INODE_BYTES)
            .ok_or_else(|| "truncated root inode slot".to_string())?,
        &sb,
    )
    .map_err(|e| format!("root inode: {e:?}"))?;
    let welcome = parse_inode(
        inode_table
            .get(INODE_BYTES..INODE_BYTES * 2)
            .ok_or_else(|| "truncated welcome inode slot".to_string())?,
        &sb,
    )
    .map_err(|e| format!("welcome inode: {e:?}"))?;
    if root.number != 1
        || root.file_type != 2
        || root.extent_count != 1
        || root.allocated_blocks != 1
        || root.mode != ROOT_MODE
        || root.uid != 0
        || root.gid != 0
        || root.atime_sec != ROOT_TIME_SEC
        || root.mtime_sec != ROOT_TIME_SEC
        || root.ctime_sec != ROOT_TIME_SEC
        || root.atime_nsec != 0
        || root.mtime_nsec != 0
        || root.ctime_nsec != 0
    {
        return Err("base-v1 root inode shape is invalid".into());
    }
    let root_extent = root.extents[0];
    if !bit_is_set(&block_bitmap, root_extent.start)? {
        return Err("root directory data block is marked free".into());
    }
    let mut directory = [0u8; BLOCK];
    read_at(&mut file, root_extent.start, &mut directory)?;
    validate_root_directory(&directory, root.size)?;

    if welcome.number != 2
        || welcome.file_type != 1
        || welcome.extent_count != 1
        || welcome.allocated_blocks != 1
        || welcome.size != WELCOME_BYTES.len() as u64
        || welcome.mode != WELCOME_MODE
        || welcome.uid != WELCOME_UID
        || welcome.gid != WELCOME_GID
        || welcome.atime_sec != WELCOME_ATIME_SEC
        || welcome.atime_nsec != WELCOME_ATIME_NSEC
        || welcome.mtime_sec != WELCOME_MTIME_SEC
        || welcome.mtime_nsec != WELCOME_MTIME_NSEC
        || welcome.ctime_sec != WELCOME_CTIME_SEC
        || welcome.ctime_nsec != WELCOME_CTIME_NSEC
    {
        return Err("base-v1 welcome file inode shape is invalid".into());
    }
    let welcome_extent = welcome.extents[0];
    if welcome_extent.start == root_extent.start
        || !bit_is_set(&block_bitmap, welcome_extent.start)?
    {
        return Err("welcome file data block is unallocated or aliases root directory".into());
    }
    let mut welcome_block = [0u8; BLOCK];
    read_at(&mut file, welcome_extent.start, &mut welcome_block)?;
    let welcome_size =
        usize::try_from(welcome.size).map_err(|_| "welcome file size overflow".to_string())?;
    if welcome_block.get(..welcome_size) != Some(WELCOME_BYTES)
        || welcome_block[welcome_size..].iter().any(|&byte| byte != 0)
    {
        return Err("welcome file payload or zero tail is invalid".into());
    }

    println!(
        "VibrixFS v1 valid: blocks={} logical_sector={} inodes={} journal_start={} journal_blocks={} root_block={} file={} file_block={} mode={:04o} uid={} gid={} mtime={}.{:09} generation={} clean={}",
        sb.total_blocks,
        logical_sector,
        sb.total_inodes,
        journal.start,
        journal.blocks,
        root_extent.start,
        WELCOME_NAME,
        welcome_extent.start,
        welcome.mode,
        welcome.uid,
        welcome.gid,
        welcome.mtime_sec,
        welcome.mtime_nsec,
        sb.generation,
        sb.clean
    );
    Ok(())
}


fn write_checkpoint(
    file: &mut File,
    mut sb: Superblock,
    root_guid: &[u8; 16],
    generation: u64,
) -> Result<(), String> {
    sb.generation = generation;
    sb.clean = true;
    let encoded = encode_superblock(&sb, sb.total_blocks, root_guid)
        .map_err(|e| format!("encode recovery checkpoint: {e:?}"))?;
    write_at(file, sb.total_blocks - 1, &encoded)?;
    file.sync_all()
        .map_err(|e| format!("sync secondary checkpoint: {e}"))?;
    write_at(file, 0, &encoded)?;
    file.sync_all()
        .map_err(|e| format!("sync primary checkpoint: {e}"))?;
    Ok(())
}

fn zero_journal(file: &mut File, journal_range: Range) -> Result<(), String> {
    let zero = [0u8; BLOCK];
    for block in journal_range.start..journal_range.start + journal_range.blocks {
        write_at(file, block, &zero)?;
    }
    file.sync_all()
        .map_err(|e| format!("sync retired journal: {e}"))
}

fn recover_image(path: &Path, logical_sector: u64, root_guid: [u8; 16]) -> Result<(), String> {
    if BLOCK as u64 % logical_sector != 0 {
        return Err("filesystem block is not an integer number of logical sectors".into());
    }
    if raw_device_like(path) {
        return Err("refusing a path that looks like a raw device".into());
    }
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|e| format!("open recovery image: {e}"))?;
    let meta = file.metadata().map_err(|e| format!("stat recovery image: {e}"))?;
    if !meta.file_type().is_file() || meta.len() % BLOCK as u64 != 0 {
        return Err("recovery input must be a regular 4096-byte-aligned file".into());
    }
    let total_blocks = meta.len() / BLOCK as u64;
    if total_blocks < 4096 {
        return Err("filesystem image is below the v1 minimum size".into());
    }

    let mut primary_raw = [0u8; BLOCK];
    let mut secondary_raw = [0u8; BLOCK];
    read_at(&mut file, 0, &mut primary_raw)?;
    read_at(&mut file, total_blocks - 1, &mut secondary_raw)?;
    let primary = parse_superblock(&primary_raw, total_blocks, &root_guid);
    let secondary = parse_superblock(&secondary_raw, total_blocks, &root_guid);

    let checkpoint = match (primary, secondary) {
        (Ok(a), Ok(b)) => {
            if !immutable_superblock_fields_match(&a, &b) {
                return Err("valid superblocks disagree on immutable geometry or identity".into());
            }
            if a.generation >= b.generation { a } else { b }
        }
        (Ok(a), Err(_)) => a,
        (Err(_), Ok(b)) => b,
        (Err(a), Err(b)) => {
            return Err(format!(
                "both superblocks are invalid: primary={a:?} secondary={b:?}"
            ));
        }
    };
    if checkpoint.total_blocks != total_blocks {
        return Err("checkpoint geometry does not match image extent".into());
    }
    let journal_range = checkpoint
        .journal
        .ok_or_else(|| "filesystem does not advertise the VibrixFS journal feature".to_string())?;

    let mut manifest_block = [0u8; BLOCK];
    read_at(&mut file, journal_range.start, &mut manifest_block)?;
    if manifest_block.iter().all(|&byte| byte == 0) {
        if !checkpoint.clean {
            return Err("dirty checkpoint has no recoverable journal transaction".into());
        }
        // Rewriting both copies is safe and repairs a stale/invalid peer while
        // preserving the already-clean checkpoint generation.
        write_checkpoint(&mut file, checkpoint, &root_guid, checkpoint.generation)?;
        println!(
            "VibrixFS recovery: no committed journal transaction; checkpoint generation={}",
            checkpoint.generation
        );
        return Ok(());
    }

    let manifest = journal::parse_manifest(&manifest_block)
        .map_err(|e| format!("journal manifest is not recoverable: {e:?}"))?;
    if manifest.journal_start != journal_range.start || manifest.journal_blocks != journal_range.blocks {
        return Err("journal manifest geometry does not match superblock".into());
    }
    let count = usize::from(manifest.entry_count);
    let mut payloads = Vec::with_capacity(count);
    for index in 0..count {
        let mut block = [0u8; BLOCK];
        let block_number = journal_range
            .start
            .checked_add(1)
            .and_then(|value| value.checked_add(index as u64))
            .ok_or_else(|| "journal payload offset overflow".to_string())?;
        read_at(&mut file, block_number, &mut block)?;
        payloads.push(block);
    }
    let commit_block_number = journal_range
        .start
        .checked_add(1)
        .and_then(|value| value.checked_add(count as u64))
        .ok_or_else(|| "journal commit offset overflow".to_string())?;
    let mut commit_block = [0u8; BLOCK];
    read_at(&mut file, commit_block_number, &mut commit_block)?;

    if commit_block.iter().all(|&byte| byte == 0) {
        // Publish a clean checkpoint before retiring the only recovery record.
        // A crash after checkpointing but before journal retirement is
        // idempotently recoverable; the reverse order can strand a dirty image
        // with no journal record at all.
        write_checkpoint(&mut file, checkpoint, &root_guid, checkpoint.generation)?;
        zero_journal(&mut file, journal_range)?;
        println!(
            "VibrixFS recovery: ignored incomplete uncommitted transaction; checkpoint generation={}",
            checkpoint.generation
        );
        return Ok(());
    }

    let validated = journal::validate_transaction(
        &manifest_block,
        &payloads,
        &commit_block,
        total_blocks,
    )
    .map_err(|e| format!("committed journal transaction failed validation: {e:?}"))?;

    if checkpoint.generation < validated.previous_generation {
        return Err(format!(
            "checkpoint generation {} is older than transaction prerequisite {}",
            checkpoint.generation, validated.previous_generation
        ));
    }

    let mut replayed = false;
    let target_generation = if checkpoint.generation < validated.new_generation {
        if checkpoint.generation != validated.previous_generation {
            return Err("transaction generation is not the next checkpoint".into());
        }
        for (index, payload) in payloads.iter().enumerate() {
            write_at(&mut file, validated.descriptors[index].home_block, payload)?;
        }
        file.sync_all()
            .map_err(|e| format!("sync replayed home blocks: {e}"))?;
        replayed = true;
        validated.new_generation
    } else {
        checkpoint.generation
    };

    write_checkpoint(&mut file, checkpoint, &root_guid, target_generation)?;
    zero_journal(&mut file, journal_range)?;
    println!(
        "VibrixFS recovery: transaction={} replayed={} generation={}",
        validated.transaction_id, replayed, target_generation
    );
    Ok(())
}

fn usage() -> String {
    "usage: vibrixfs-image format <new-regular-file> <blocks> <logical-sector:512|4096> <fs-uuid-hex32> <root-guid-wire-hex32>\n       vibrixfs-image inspect <regular-file> <logical-sector:512|4096> <root-guid-wire-hex32>\n       vibrixfs-image recover <regular-file> <logical-sector:512|4096> <root-guid-wire-hex32>".into()
}

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("format") if args.len() == 7 => {
            let blocks = args[3]
                .parse::<u64>()
                .map_err(|_| "blocks must be an unsigned integer".to_string())?;
            let sector = sector_size(&args[4])?;
            format_image(
                Path::new(&args[2]),
                blocks,
                sector,
                hex16(&args[5])?,
                hex16(&args[6])?,
            )?;
            inspect_image(Path::new(&args[2]), sector, hex16(&args[6])?)
        }
        Some("inspect") if args.len() == 5 => inspect_image(
            Path::new(&args[2]),
            sector_size(&args[3])?,
            hex16(&args[4])?,
        ),
        Some("recover") if args.len() == 5 => recover_image(
            Path::new(&args[2]),
            sector_size(&args[3])?,
            hex16(&args[4])?,
        ),
        _ => Err(usage()),
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("vibrixfs-image: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identities_accept_hex_and_reject_zero_or_garbage() {
        assert_eq!(hex16("00112233445566778899aabbccddeeff").unwrap()[0], 0);
        assert_eq!(
            hex16("00112233-4455-6677-8899-aabbccddeeff").unwrap()[15],
            0xff
        );
        assert!(hex16("00").is_err());
        assert!(hex16("zz112233445566778899aabbccddeeff").is_err());
        assert!(hex16("00000000000000000000000000000000").is_err());
    }

    #[test]
    fn root_directory_has_exact_dot_records_and_zero_padding() {
        let block = root_directory_block().unwrap();
        validate_root_directory(&block, DIRECTORY_BYTES as u64).unwrap();
        let mut corrupt = block;
        corrupt[23] = 1;
        assert!(validate_root_directory(&corrupt, DIRECTORY_BYTES as u64).is_err());
    }

    #[test]
    fn early_layout_is_non_overlapping_and_root_precedes_backup() {
        let (sb, root) = layout(4096, [1; 16], [2; 16]).unwrap();
        assert_eq!(sb.block_bitmap.start, 1);
        assert!(sb.block_bitmap.start + sb.block_bitmap.blocks <= sb.inode_bitmap.start);
        assert!(sb.inode_bitmap.start + sb.inode_bitmap.blocks <= sb.inode_table.start);
        let journal = sb.journal.unwrap();
        assert_eq!(journal.start, sb.inode_table.start + sb.inode_table.blocks);
        assert_eq!(journal.blocks, JOURNAL_BLOCKS);
        assert_eq!(root, journal.start + journal.blocks);
        assert!(root < sb.total_blocks - 1);
    }

    #[test]
    fn adopted_permissions_and_timestamps_round_trip_through_wire_inode() {
        let (sb, root_data) = layout(4096, [1; 16], [2; 16]).unwrap();
        let mut extents = [Extent {
            start: 0,
            blocks: 0,
        }; 6];
        extents[0] = Extent {
            start: root_data + 1,
            blocks: 1,
        };
        let inode = Inode {
            number: 2,
            file_type: 1,
            mode: WELCOME_MODE,
            uid: WELCOME_UID,
            gid: WELCOME_GID,
            links: 1,
            size: WELCOME_BYTES.len() as u64,
            allocated_blocks: 1,
            atime_sec: WELCOME_ATIME_SEC,
            atime_nsec: WELCOME_ATIME_NSEC,
            mtime_sec: WELCOME_MTIME_SEC,
            mtime_nsec: WELCOME_MTIME_NSEC,
            ctime_sec: WELCOME_CTIME_SEC,
            ctime_nsec: WELCOME_CTIME_NSEC,
            nonce: [0x6b; 16],
            extents,
            extent_count: 1,
            device: 0,
            flags: 0,
        };
        let bytes = encode_inode(&inode, &sb).unwrap();
        assert_eq!(
            u16::from_le_bytes(bytes[10..12].try_into().unwrap()),
            WELCOME_MODE
        );
        assert_eq!(
            u32::from_le_bytes(bytes[12..16].try_into().unwrap()),
            WELCOME_UID
        );
        assert_eq!(
            u32::from_le_bytes(bytes[16..20].try_into().unwrap()),
            WELCOME_GID
        );
        assert_eq!(
            i64::from_le_bytes(bytes[56..64].try_into().unwrap()),
            WELCOME_MTIME_SEC
        );
        assert_eq!(
            u32::from_le_bytes(bytes[64..68].try_into().unwrap()),
            WELCOME_MTIME_NSEC
        );
        assert_eq!(parse_inode(&bytes, &sb), Ok(inode));
    }



    fn temp_image(label: &str) -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "vibrixfs-{label}-{}-{}.img",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        path
    }

    fn stage_committed_repair(path: &Path, root_guid: [u8; 16], corrupt_payload: bool) -> Vec<u8> {
        let mut file = OpenOptions::new().read(true).write(true).open(path).unwrap();
        let total_blocks = file.metadata().unwrap().len() / BLOCK as u64;
        let mut raw = [0u8; BLOCK];
        read_at(&mut file, 0, &mut raw).unwrap();
        let mut sb = parse_superblock(&raw, total_blocks, &root_guid).unwrap();
        let journal_range = sb.journal.unwrap();
        let home = sb.inode_bitmap.start;
        let mut payload = [0u8; BLOCK];
        read_at(&mut file, home, &mut payload).unwrap();
        let expected = payload.to_vec();

        sb.clean = false;
        let dirty = encode_superblock(&sb, total_blocks, &root_guid).unwrap();
        write_at(&mut file, 0, &dirty).unwrap();
        write_at(&mut file, total_blocks - 1, &dirty).unwrap();

        let manifest = journal::encode_manifest(
            9,
            sb.generation,
            sb.generation + 1,
            journal_range.start,
            journal_range.blocks,
            &[(home, &payload)],
        )
        .unwrap();
        let commit = journal::encode_commit(&manifest).unwrap();
        write_at(&mut file, journal_range.start, &manifest).unwrap();
        if corrupt_payload {
            payload[17] ^= 1;
        }
        write_at(&mut file, journal_range.start + 1, &payload).unwrap();
        write_at(&mut file, journal_range.start + 2, &commit).unwrap();
        write_at(&mut file, home, &[0u8; BLOCK]).unwrap();
        file.sync_all().unwrap();
        expected
    }

    #[test]
    fn committed_transaction_replays_then_checkpoints_and_retires_journal() {
        let path = temp_image("recover");
        let root_guid = [2u8; 16];
        format_image(&path, 4096, 512, [1u8; 16], root_guid).unwrap();
        let expected = stage_committed_repair(&path, root_guid, false);

        recover_image(&path, 512, root_guid).unwrap();
        inspect_image(&path, 512, root_guid).unwrap();

        let mut file = File::open(&path).unwrap();
        let mut primary = [0u8; BLOCK];
        let mut home = vec![0u8; BLOCK];
        read_at(&mut file, 0, &mut primary).unwrap();
        let sb = parse_superblock(&primary, 4096, &root_guid).unwrap();
        assert_eq!(sb.generation, 2);
        assert!(sb.clean);
        read_at(&mut file, sb.inode_bitmap.start, &mut home).unwrap();
        assert_eq!(home, expected);
        let mut journal_head = [0u8; BLOCK];
        read_at(&mut file, sb.journal.unwrap().start, &mut journal_head).unwrap();
        assert!(journal_head.iter().all(|&byte| byte == 0));
        drop(file);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn corrupt_committed_payload_fails_before_any_home_replay() {
        let path = temp_image("corrupt-recovery");
        let root_guid = [4u8; 16];
        format_image(&path, 4096, 4096, [3u8; 16], root_guid).unwrap();
        let _ = stage_committed_repair(&path, root_guid, true);
        assert!(recover_image(&path, 4096, root_guid).is_err());

        let mut file = File::open(&path).unwrap();
        let mut primary = [0u8; BLOCK];
        read_at(&mut file, 0, &mut primary).unwrap();
        let sb = parse_superblock(&primary, 4096, &root_guid).unwrap();
        let mut home = [0u8; BLOCK];
        read_at(&mut file, sb.inode_bitmap.start, &mut home).unwrap();
        assert!(home.iter().all(|&byte| byte == 0));
        drop(file);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn dirty_checkpoint_without_journal_fails_closed() {
        let path = temp_image("dirty-no-journal");
        let root_guid = [6u8; 16];
        format_image(&path, 4096, 512, [5u8; 16], root_guid).unwrap();

        let mut file = OpenOptions::new().read(true).write(true).open(&path).unwrap();
        let mut raw = [0u8; BLOCK];
        read_at(&mut file, 0, &mut raw).unwrap();
        let mut sb = parse_superblock(&raw, 4096, &root_guid).unwrap();
        sb.clean = false;
        let dirty = encode_superblock(&sb, 4096, &root_guid).unwrap();
        write_at(&mut file, 0, &dirty).unwrap();
        write_at(&mut file, 4095, &dirty).unwrap();
        file.sync_all().unwrap();
        drop(file);

        let error = recover_image(&path, 512, root_guid).unwrap_err();
        assert!(error.contains("dirty checkpoint has no recoverable journal transaction"));

        let mut file = File::open(&path).unwrap();
        read_at(&mut file, 0, &mut raw).unwrap();
        let after = parse_superblock(&raw, 4096, &root_guid).unwrap();
        assert!(!after.clean);
        let mut journal_head = [0u8; BLOCK];
        read_at(&mut file, after.journal.unwrap().start, &mut journal_head).unwrap();
        assert!(journal_head.iter().all(|&byte| byte == 0));
        drop(file);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn uncommitted_transaction_checkpoints_before_retirement() {
        let path = temp_image("uncommitted");
        let root_guid = [8u8; 16];
        format_image(&path, 4096, 4096, [7u8; 16], root_guid).unwrap();

        let mut file = OpenOptions::new().read(true).write(true).open(&path).unwrap();
        let mut raw = [0u8; BLOCK];
        read_at(&mut file, 0, &mut raw).unwrap();
        let mut sb = parse_superblock(&raw, 4096, &root_guid).unwrap();
        let journal_range = sb.journal.unwrap();
        let home = sb.inode_bitmap.start;
        let mut payload = [0u8; BLOCK];
        read_at(&mut file, home, &mut payload).unwrap();
        let expected_home = payload;

        sb.clean = false;
        let dirty = encode_superblock(&sb, 4096, &root_guid).unwrap();
        write_at(&mut file, 0, &dirty).unwrap();
        write_at(&mut file, 4095, &dirty).unwrap();
        let manifest = journal::encode_manifest(
            11,
            sb.generation,
            sb.generation + 1,
            journal_range.start,
            journal_range.blocks,
            &[(home, &payload)],
        )
        .unwrap();
        write_at(&mut file, journal_range.start, &manifest).unwrap();
        write_at(&mut file, journal_range.start + 1, &payload).unwrap();
        file.sync_all().unwrap();
        drop(file);

        recover_image(&path, 4096, root_guid).unwrap();
        inspect_image(&path, 4096, root_guid).unwrap();

        let mut file = File::open(&path).unwrap();
        read_at(&mut file, 0, &mut raw).unwrap();
        let after = parse_superblock(&raw, 4096, &root_guid).unwrap();
        assert_eq!(after.generation, 1);
        assert!(after.clean);
        let mut actual_home = [0u8; BLOCK];
        read_at(&mut file, home, &mut actual_home).unwrap();
        assert_eq!(actual_home, expected_home);
        let mut journal_head = [0u8; BLOCK];
        read_at(&mut file, journal_range.start, &mut journal_head).unwrap();
        assert!(journal_head.iter().all(|&byte| byte == 0));
        drop(file);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_raw_device_shapes_and_out_of_bound_sizes() {
        assert!(raw_device_like(Path::new("/dev/sda")));
        assert!(raw_device_like(Path::new("/proc/self/mem")));
        assert!(!raw_device_like(Path::new("build/vibrixfs.img")));
        assert!(layout(4095, [1; 16], [2; 16]).is_err());
        assert!(layout(MAX_FORMAT_BLOCKS + 1, [1; 16], [2; 16]).is_err());
        assert_eq!(sector_size("512"), Ok(512));
        assert_eq!(sector_size("4096"), Ok(4096));
        assert!(sector_size("2048").is_err());
    }
}
