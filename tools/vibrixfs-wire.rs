//! VibrixFS v1 wire-format conformance helper.
//!
//! This host-only tool encodes/validates synthetic metadata in memory. It does
//! not open a path, create a filesystem, format a partition, or write a device.

pub const BLOCK: usize = 4096;
const HEADER: u32 = 256;
pub const INODE_BYTES: usize = 256;
const MIN_BLOCKS: u64 = 4096;
const MAX_BLOCKS: u64 = 1u64 << 48;
const MAGIC: &[u8; 8] = b"VIBRIXFS";
const MAJOR: u16 = 1;
const CLEAN: u32 = 1;
const INCOMPAT_JOURNAL: u32 = 1;
const INODE_EXTENTS: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Range {
    pub start: u64,
    pub blocks: u64,
}

impl Range {
    fn end(self) -> Option<u64> {
        self.start.checked_add(self.blocks)
    }

    fn valid_metadata(self, total: u64) -> bool {
        self.start > 0
            && self.blocks > 0
            && self.end().is_some_and(|end| end < total.saturating_sub(1))
    }

    fn overlaps(self, other: Self) -> bool {
        let Some(a) = self.end() else { return true };
        let Some(b) = other.end() else { return true };
        self.start < b && other.start < a
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Superblock {
    pub minor: u16,
    pub clean: bool,
    pub generation: u64,
    pub total_blocks: u64,
    pub total_inodes: u64,
    pub block_bitmap: Range,
    pub inode_bitmap: Range,
    pub inode_table: Range,
    pub journal: Option<Range>,
    pub filesystem_uuid: [u8; 16],
    pub root_partition_guid: [u8; 16],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Extent {
    pub start: u64,
    pub blocks: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Inode {
    pub number: u64,
    pub file_type: u8,
    pub mode: u16,
    pub uid: u32,
    pub gid: u32,
    pub links: u32,
    pub size: u64,
    pub allocated_blocks: u64,
    pub atime_sec: i64,
    pub atime_nsec: u32,
    pub mtime_sec: i64,
    pub mtime_nsec: u32,
    pub ctime_sec: i64,
    pub ctime_nsec: u32,
    pub nonce: [u8; 16],
    pub extents: [Extent; INODE_EXTENTS],
    pub extent_count: u8,
    pub device: u64,
    pub flags: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Truncated,
    Magic,
    Version,
    Layout,
    Checksum,
    Reserved,
    Features,
    Identity,
    Geometry,
    Inode,
    Extent,
    Directory,
}

fn u16_at(data: &[u8], at: usize) -> Result<u16, Error> {
    Ok(u16::from_le_bytes(
        data.get(at..at + 2)
            .ok_or(Error::Truncated)?
            .try_into()
            .map_err(|_| Error::Truncated)?,
    ))
}

fn u32_at(data: &[u8], at: usize) -> Result<u32, Error> {
    Ok(u32::from_le_bytes(
        data.get(at..at + 4)
            .ok_or(Error::Truncated)?
            .try_into()
            .map_err(|_| Error::Truncated)?,
    ))
}

fn u64_at(data: &[u8], at: usize) -> Result<u64, Error> {
    Ok(u64::from_le_bytes(
        data.get(at..at + 8)
            .ok_or(Error::Truncated)?
            .try_into()
            .map_err(|_| Error::Truncated)?,
    ))
}

fn i64_at(data: &[u8], at: usize) -> Result<i64, Error> {
    Ok(i64::from_le_bytes(
        data.get(at..at + 8)
            .ok_or(Error::Truncated)?
            .try_into()
            .map_err(|_| Error::Truncated)?,
    ))
}

fn u48_at(data: &[u8], at: usize) -> Result<u64, Error> {
    let bytes = data.get(at..at + 6).ok_or(Error::Truncated)?;
    Ok(bytes.iter().enumerate().fold(0u64, |value, (shift, byte)| {
        value | (u64::from(*byte) << (shift * 8))
    }))
}

fn put_u48(data: &mut [u8], at: usize, value: u64) -> Result<(), Error> {
    if value >= MAX_BLOCKS {
        return Err(Error::Extent);
    }
    for i in 0..6 {
        data[at + i] = (value >> (i * 8)) as u8;
    }
    Ok(())
}

/// Reflected IEEE CRC-32, same polynomial as GPT tooling.
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

fn block_crc(block: &[u8; BLOCK], field: core::ops::Range<usize>) -> u32 {
    let mut copy = *block;
    copy[field].fill(0);
    crc32(&copy)
}

fn nonzero(bytes: &[u8]) -> bool {
    bytes.iter().any(|&byte| byte != 0)
}

fn validate_superblock(
    info: &Superblock,
    partition_blocks: u64,
    expected_root_guid: &[u8; 16],
) -> Result<(), Error> {
    if !(MIN_BLOCKS..MAX_BLOCKS).contains(&info.total_blocks)
        || info.total_blocks > partition_blocks
        || info.total_inodes == 0
    {
        return Err(Error::Geometry);
    }
    let regions = [info.block_bitmap, info.inode_bitmap, info.inode_table];
    if regions
        .iter()
        .any(|region| !region.valid_metadata(info.total_blocks))
    {
        return Err(Error::Geometry);
    }
    for (index, region) in regions.iter().enumerate() {
        if regions
            .iter()
            .take(index)
            .any(|other| region.overlaps(*other))
        {
            return Err(Error::Geometry);
        }
    }
    if let Some(journal) = info.journal {
        if journal.blocks < 3
            || !journal.valid_metadata(info.total_blocks)
            || regions.iter().any(|region| journal.overlaps(*region))
        {
            return Err(Error::Geometry);
        }
    }
    let bits_per_block = (BLOCK as u64) * 8;
    if info
        .block_bitmap
        .blocks
        .checked_mul(bits_per_block)
        .is_none_or(|n| n < info.total_blocks)
        || info
            .inode_bitmap
            .blocks
            .checked_mul(bits_per_block)
            .is_none_or(|n| n < info.total_inodes.saturating_add(1))
        || info
            .inode_table
            .blocks
            .checked_mul((BLOCK / INODE_BYTES) as u64)
            .is_none_or(|n| n < info.total_inodes)
    {
        return Err(Error::Geometry);
    }
    if !nonzero(&info.filesystem_uuid)
        || !nonzero(&info.root_partition_guid)
        || &info.root_partition_guid != expected_root_guid
    {
        return Err(Error::Identity);
    }
    Ok(())
}

pub fn encode_superblock(
    info: &Superblock,
    partition_blocks: u64,
    expected_root_guid: &[u8; 16],
) -> Result<[u8; BLOCK], Error> {
    validate_superblock(info, partition_blocks, expected_root_guid)?;
    let mut out = [0u8; BLOCK];
    out[0..8].copy_from_slice(MAGIC);
    out[8..10].copy_from_slice(&MAJOR.to_le_bytes());
    out[10..12].copy_from_slice(&info.minor.to_le_bytes());
    out[12..16].copy_from_slice(&HEADER.to_le_bytes());
    out[16..20].copy_from_slice(&(BLOCK as u32).to_le_bytes());
    out[20..24].copy_from_slice(&(if info.clean { CLEAN } else { 0 }).to_le_bytes());
    out[24..32].copy_from_slice(&info.generation.to_le_bytes());
    out[32..40].copy_from_slice(&info.total_blocks.to_le_bytes());
    out[40..48].copy_from_slice(&info.total_inodes.to_le_bytes());
    out[48..56].copy_from_slice(&info.block_bitmap.start.to_le_bytes());
    out[56..64].copy_from_slice(&info.block_bitmap.blocks.to_le_bytes());
    out[64..72].copy_from_slice(&info.inode_bitmap.start.to_le_bytes());
    out[72..80].copy_from_slice(&info.inode_bitmap.blocks.to_le_bytes());
    out[80..88].copy_from_slice(&info.inode_table.start.to_le_bytes());
    out[88..96].copy_from_slice(&info.inode_table.blocks.to_le_bytes());
    out[96..104].copy_from_slice(&1u64.to_le_bytes());
    if let Some(journal) = info.journal {
        out[104..112].copy_from_slice(&journal.start.to_le_bytes());
        out[112..120].copy_from_slice(&journal.blocks.to_le_bytes());
        out[128..132].copy_from_slice(&INCOMPAT_JOURNAL.to_le_bytes());
    }
    out[136..152].copy_from_slice(&info.filesystem_uuid);
    out[152..168].copy_from_slice(&info.root_partition_guid);
    let checksum = block_crc(&out, 120..124);
    out[120..124].copy_from_slice(&checksum.to_le_bytes());
    Ok(out)
}

pub fn parse_superblock(
    data: &[u8],
    partition_blocks: u64,
    expected_root_guid: &[u8; 16],
) -> Result<Superblock, Error> {
    let block: &[u8; BLOCK] = data
        .get(..BLOCK)
        .ok_or(Error::Truncated)?
        .try_into()
        .map_err(|_| Error::Truncated)?;
    if &block[0..8] != MAGIC {
        return Err(Error::Magic);
    }
    if u16_at(block, 8)? != MAJOR {
        return Err(Error::Version);
    }
    if u32_at(block, 12)? != HEADER || u32_at(block, 16)? != BLOCK as u32 {
        return Err(Error::Layout);
    }
    let flags = u32_at(block, 20)?;
    if flags & !CLEAN != 0 {
        return Err(Error::Reserved);
    }
    if u64_at(block, 96)? != 1 {
        return Err(Error::Layout);
    }
    if u32_at(block, 124)? != 0 || u32_at(block, 132)? != 0 {
        return Err(Error::Features);
    }
    let incompat = u32_at(block, 128)?;
    if incompat & !INCOMPAT_JOURNAL != 0 {
        return Err(Error::Features);
    }
    let journal_start = u64_at(block, 104)?;
    let journal_blocks = u64_at(block, 112)?;
    let journal = match (
        incompat & INCOMPAT_JOURNAL != 0,
        journal_start,
        journal_blocks,
    ) {
        (false, 0, 0) => None,
        (true, start, blocks) if start != 0 && blocks != 0 => Some(Range { start, blocks }),
        _ => return Err(Error::Layout),
    };
    if block[168..].iter().any(|&byte| byte != 0) {
        return Err(Error::Reserved);
    }
    if u32_at(block, 120)? != block_crc(block, 120..124) {
        return Err(Error::Checksum);
    }
    let mut filesystem_uuid = [0u8; 16];
    filesystem_uuid.copy_from_slice(&block[136..152]);
    let mut root_partition_guid = [0u8; 16];
    root_partition_guid.copy_from_slice(&block[152..168]);
    let result = Superblock {
        minor: u16_at(block, 10)?,
        clean: flags & CLEAN != 0,
        generation: u64_at(block, 24)?,
        total_blocks: u64_at(block, 32)?,
        total_inodes: u64_at(block, 40)?,
        block_bitmap: Range {
            start: u64_at(block, 48)?,
            blocks: u64_at(block, 56)?,
        },
        inode_bitmap: Range {
            start: u64_at(block, 64)?,
            blocks: u64_at(block, 72)?,
        },
        inode_table: Range {
            start: u64_at(block, 80)?,
            blocks: u64_at(block, 88)?,
        },
        journal,
        filesystem_uuid,
        root_partition_guid,
    };
    validate_superblock(&result, partition_blocks, expected_root_guid)?;
    Ok(result)
}

pub fn immutable_superblock_fields_match(a: &Superblock, b: &Superblock) -> bool {
    a.minor == b.minor
        && a.total_blocks == b.total_blocks
        && a.total_inodes == b.total_inodes
        && a.block_bitmap == b.block_bitmap
        && a.inode_bitmap == b.inode_bitmap
        && a.inode_table == b.inode_table
        && a.journal == b.journal
        && a.filesystem_uuid == b.filesystem_uuid
        && a.root_partition_guid == b.root_partition_guid
}

pub fn encode_inode(inode: &Inode, fs: &Superblock) -> Result<[u8; INODE_BYTES], Error> {
    validate_inode(inode, fs)?;
    let mut out = [0u8; INODE_BYTES];
    out[0..8].copy_from_slice(&inode.number.to_le_bytes());
    out[8] = inode.file_type;
    out[9] = inode.extent_count;
    out[10..12].copy_from_slice(&inode.mode.to_le_bytes());
    out[12..16].copy_from_slice(&inode.uid.to_le_bytes());
    out[16..20].copy_from_slice(&inode.gid.to_le_bytes());
    out[20..24].copy_from_slice(&inode.links.to_le_bytes());
    out[24..32].copy_from_slice(&inode.size.to_le_bytes());
    out[32..40].copy_from_slice(&inode.allocated_blocks.to_le_bytes());
    out[40..48].copy_from_slice(&inode.atime_sec.to_le_bytes());
    out[48..52].copy_from_slice(&inode.atime_nsec.to_le_bytes());
    out[56..64].copy_from_slice(&inode.mtime_sec.to_le_bytes());
    out[64..68].copy_from_slice(&inode.mtime_nsec.to_le_bytes());
    out[72..80].copy_from_slice(&inode.ctime_sec.to_le_bytes());
    out[80..84].copy_from_slice(&inode.ctime_nsec.to_le_bytes());
    out[88..104].copy_from_slice(&inode.nonce);
    for (index, extent) in inode.extents.iter().enumerate() {
        let at = 104 + index * 12;
        if index < usize::from(inode.extent_count) {
            put_u48(&mut out, at, extent.start)?;
            out[at + 6..at + 10].copy_from_slice(&extent.blocks.to_le_bytes());
        }
    }
    out[176..184].copy_from_slice(&inode.device.to_le_bytes());
    out[188..192].copy_from_slice(&inode.flags.to_le_bytes());
    let checksum = {
        let mut copy = out;
        copy[184..188].fill(0);
        crc32(&copy)
    };
    out[184..188].copy_from_slice(&checksum.to_le_bytes());
    Ok(out)
}

pub fn parse_inode(data: &[u8], fs: &Superblock) -> Result<Inode, Error> {
    let raw: &[u8; INODE_BYTES] = data
        .get(..INODE_BYTES)
        .ok_or(Error::Truncated)?
        .try_into()
        .map_err(|_| Error::Truncated)?;
    if raw[52..56].iter().any(|&b| b != 0)
        || raw[68..72].iter().any(|&b| b != 0)
        || raw[84..88].iter().any(|&b| b != 0)
        || raw[192..].iter().any(|&b| b != 0)
    {
        return Err(Error::Reserved);
    }
    let saved = u32_at(raw, 184)?;
    let mut copy = *raw;
    copy[184..188].fill(0);
    if crc32(&copy) != saved {
        return Err(Error::Checksum);
    }
    let count = raw[9];
    if usize::from(count) > INODE_EXTENTS {
        return Err(Error::Inode);
    }
    let mut extents = [Extent {
        start: 0,
        blocks: 0,
    }; INODE_EXTENTS];
    for (index, extent) in extents.iter_mut().enumerate() {
        let at = 104 + index * 12;
        let flags = u16_at(raw, at + 10)?;
        if flags != 0 {
            return Err(Error::Features);
        }
        extent.start = u48_at(raw, at)?;
        extent.blocks = u32_at(raw, at + 6)?;
        if index >= usize::from(count) && (extent.start != 0 || extent.blocks != 0) {
            return Err(Error::Inode);
        }
    }
    let mut nonce = [0u8; 16];
    nonce.copy_from_slice(&raw[88..104]);
    let inode = Inode {
        number: u64_at(raw, 0)?,
        file_type: raw[8],
        mode: u16_at(raw, 10)?,
        uid: u32_at(raw, 12)?,
        gid: u32_at(raw, 16)?,
        links: u32_at(raw, 20)?,
        size: u64_at(raw, 24)?,
        allocated_blocks: u64_at(raw, 32)?,
        atime_sec: i64_at(raw, 40)?,
        atime_nsec: u32_at(raw, 48)?,
        mtime_sec: i64_at(raw, 56)?,
        mtime_nsec: u32_at(raw, 64)?,
        ctime_sec: i64_at(raw, 72)?,
        ctime_nsec: u32_at(raw, 80)?,
        nonce,
        extents,
        extent_count: count,
        device: u64_at(raw, 176)?,
        flags: u32_at(raw, 188)?,
    };
    validate_inode(&inode, fs)?;
    Ok(inode)
}

fn validate_inode(inode: &Inode, fs: &Superblock) -> Result<(), Error> {
    if inode.number == 0
        || inode.number > fs.total_inodes
        || !(1..=5).contains(&inode.file_type)
        || inode.mode & !0x0fff != 0
        || inode.links == 0
        || usize::from(inode.extent_count) > INODE_EXTENTS
        || !nonzero(&inode.nonce)
        || inode.atime_nsec >= 1_000_000_000
        || inode.mtime_nsec >= 1_000_000_000
        || inode.ctime_nsec >= 1_000_000_000
    {
        return Err(Error::Inode);
    }
    if inode.file_type >= 4 && (inode.extent_count != 0 || inode.allocated_blocks != 0) {
        return Err(Error::Inode);
    }
    let metadata = [fs.block_bitmap, fs.inode_bitmap, fs.inode_table];
    let mut sum = 0u64;
    for (index, extent) in inode
        .extents
        .iter()
        .take(usize::from(inode.extent_count))
        .enumerate()
    {
        if extent.start == 0 || extent.blocks == 0 {
            return Err(Error::Extent);
        }
        let range = Range {
            start: extent.start,
            blocks: u64::from(extent.blocks),
        };
        let Some(end) = range.end() else {
            return Err(Error::Extent);
        };
        if end >= fs.total_blocks
            || metadata.iter().any(|m| range.overlaps(*m))
            || fs.journal.is_some_and(|journal| range.overlaps(journal))
        {
            return Err(Error::Extent);
        }
        if index > 0 {
            let prev = inode.extents[index - 1];
            let previous_end = prev.start + u64::from(prev.blocks);
            if previous_end > extent.start {
                return Err(Error::Extent);
            }
        }
        sum = sum
            .checked_add(u64::from(extent.blocks))
            .ok_or(Error::Extent)?;
    }
    if sum != inode.allocated_blocks
        || (matches!(inode.file_type, 1 | 2 | 3)
            && inode
                .allocated_blocks
                .checked_mul(BLOCK as u64)
                .is_none_or(|bytes| bytes < inode.size))
    {
        return Err(Error::Inode);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirEntry<'a> {
    pub inode: u64,
    pub file_type: u8,
    pub name: &'a str,
}

/// Encode one 8-byte-aligned directory record into the supplied slice.
/// Returns the exact record byte length; callers concatenate records.
pub fn encode_dir_record(
    dst: &mut [u8],
    inode: u64,
    file_type: u8,
    name: &str,
) -> Result<usize, Error> {
    let bytes = name.as_bytes();
    if inode == 0
        || !(1..=5).contains(&file_type)
        || bytes.is_empty()
        || bytes.len() > 255
        || bytes.contains(&0)
        || bytes.contains(&b'/')
    {
        return Err(Error::Directory);
    }
    let used = 16usize.checked_add(bytes.len()).ok_or(Error::Directory)?;
    let record = used
        .checked_add(7)
        .map(|n| n & !7)
        .ok_or(Error::Directory)?;
    let out = dst.get_mut(..record).ok_or(Error::Truncated)?;
    out.fill(0);
    out[..8].copy_from_slice(&inode.to_le_bytes());
    out[8..10].copy_from_slice(
        &u16::try_from(record)
            .map_err(|_| Error::Directory)?
            .to_le_bytes(),
    );
    out[10] = u8::try_from(bytes.len()).map_err(|_| Error::Directory)?;
    out[11] = file_type;
    out[16..16 + bytes.len()].copy_from_slice(bytes);
    Ok(record)
}

/// Decode one directory record without following its inode reference.
/// The caller validates allocation and type agreement against the inode table.
pub fn parse_dir_record(data: &[u8], max_inode: u64) -> Result<(DirEntry<'_>, usize), Error> {
    if data.len() < 16 {
        return Err(Error::Truncated);
    }
    let inode = u64_at(data, 0)?;
    let record = usize::from(u16_at(data, 8)?);
    let name_len = usize::from(data[10]);
    let file_type = data[11];
    if inode == 0
        || inode > max_inode
        || !(1..=5).contains(&file_type)
        || record < 16
        || !record.is_multiple_of(8)
        || record > data.len()
        || 16usize
            .checked_add(name_len)
            .is_none_or(|used| used > record)
        || data[12..16].iter().any(|&byte| byte != 0)
    {
        return Err(Error::Directory);
    }
    let name_bytes = &data[16..16 + name_len];
    if name_bytes.is_empty() || name_bytes.contains(&0) || name_bytes.contains(&b'/') {
        return Err(Error::Directory);
    }
    let name = core::str::from_utf8(name_bytes).map_err(|_| Error::Directory)?;
    if data[16 + name_len..record].iter().any(|&byte| byte != 0) {
        return Err(Error::Directory);
    }
    Ok((
        DirEntry {
            inode,
            file_type,
            name,
        },
        record,
    ))
}

#[cfg(not(test))]
fn main() {
    eprintln!("VibrixFS wire conformance helper: run with rustc --test");
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOT_GUID: [u8; 16] = [0x22; 16];

    fn sample_superblock() -> Superblock {
        Superblock {
            minor: 0,
            clean: true,
            generation: 7,
            total_blocks: 8192,
            total_inodes: 1024,
            block_bitmap: Range {
                start: 1,
                blocks: 1,
            },
            inode_bitmap: Range {
                start: 2,
                blocks: 1,
            },
            inode_table: Range {
                start: 3,
                blocks: 64,
            },
            journal: None,
            filesystem_uuid: [0x11; 16],
            root_partition_guid: ROOT_GUID,
        }
    }

    fn sample_inode() -> Inode {
        let mut extents = [Extent {
            start: 0,
            blocks: 0,
        }; INODE_EXTENTS];
        extents[0] = Extent {
            start: 100,
            blocks: 2,
        };
        Inode {
            number: 1,
            file_type: 2,
            mode: 0o755,
            uid: 0,
            gid: 0,
            links: 2,
            size: 128,
            allocated_blocks: 2,
            atime_sec: 1,
            atime_nsec: 2,
            mtime_sec: 3,
            mtime_nsec: 4,
            ctime_sec: 5,
            ctime_nsec: 6,
            nonce: [0x33; 16],
            extents,
            extent_count: 1,
            device: 0,
            flags: 0,
        }
    }

    #[test]
    fn crc32_known_vector() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    }

    #[test]
    fn superblock_golden_offsets_crc_and_round_trip() {
        let sb = sample_superblock();
        let bytes = encode_superblock(&sb, 8192, &ROOT_GUID).unwrap();
        assert_eq!(&bytes[..8], b"VIBRIXFS");
        assert_eq!(&bytes[8..10], &1u16.to_le_bytes());
        assert_eq!(&bytes[16..20], &4096u32.to_le_bytes());
        assert_eq!(&bytes[32..40], &8192u64.to_le_bytes());
        assert_eq!(&bytes[96..104], &1u64.to_le_bytes());
        assert_eq!(&bytes[104..120], &[0u8; 16]);
        assert_eq!(&bytes[128..132], &0u32.to_le_bytes());
        assert_eq!(&bytes[136..152], &[0x11; 16]);
        assert_eq!(&bytes[152..168], &ROOT_GUID);
        assert_ne!(u32_at(&bytes, 120).unwrap(), 0);
        assert_eq!(parse_superblock(&bytes, 8192, &ROOT_GUID), Ok(sb));
    }

    #[test]
    fn journal_feature_round_trips_and_reserves_its_geometry() {
        let mut sb = sample_superblock();
        sb.journal = Some(Range {
            start: 67,
            blocks: 66,
        });
        let bytes = encode_superblock(&sb, 8192, &ROOT_GUID).unwrap();
        assert_eq!(&bytes[104..112], &67u64.to_le_bytes());
        assert_eq!(&bytes[112..120], &66u64.to_le_bytes());
        assert_eq!(&bytes[128..132], &INCOMPAT_JOURNAL.to_le_bytes());
        assert_eq!(parse_superblock(&bytes, 8192, &ROOT_GUID), Ok(sb));

        let mut inode = sample_inode();
        inode.extents[0] = Extent {
            start: 80,
            blocks: 1,
        };
        inode.allocated_blocks = 1;
        assert_eq!(encode_inode(&inode, &sb), Err(Error::Extent));
    }

    #[test]
    fn superblock_rejects_corruption_features_reserved_identity_and_geometry() {
        let sb = sample_superblock();
        let good = encode_superblock(&sb, 8192, &ROOT_GUID).unwrap();

        let mut bad = good;
        bad[300] = 1;
        // Recompute CRC to prove reserved-byte validation is independent.
        let crc = block_crc(&bad, 120..124);
        bad[120..124].copy_from_slice(&crc.to_le_bytes());
        assert_eq!(
            parse_superblock(&bad, 8192, &ROOT_GUID),
            Err(Error::Reserved)
        );

        let mut bad = good;
        bad[128] = 2; // unknown incompatible feature; bit 0 is the journal.
        let crc = block_crc(&bad, 120..124);
        bad[120..124].copy_from_slice(&crc.to_le_bytes());
        assert_eq!(
            parse_superblock(&bad, 8192, &ROOT_GUID),
            Err(Error::Features)
        );

        let mut bad = good;
        bad[128] = INCOMPAT_JOURNAL as u8;
        let crc = block_crc(&bad, 120..124);
        bad[120..124].copy_from_slice(&crc.to_le_bytes());
        assert_eq!(parse_superblock(&bad, 8192, &ROOT_GUID), Err(Error::Layout));

        let mut bad = good;
        bad[24] ^= 1;
        assert_eq!(
            parse_superblock(&bad, 8192, &ROOT_GUID),
            Err(Error::Checksum)
        );

        assert_eq!(
            parse_superblock(&good, 8192, &[0x44; 16]),
            Err(Error::Identity)
        );

        let mut broken = sb;
        broken.inode_table = broken.block_bitmap;
        assert_eq!(
            encode_superblock(&broken, 8192, &ROOT_GUID),
            Err(Error::Geometry)
        );
    }

    #[test]
    fn independent_superblock_copies_require_immutable_agreement() {
        let a = sample_superblock();
        let mut b = a;
        b.clean = false;
        b.generation += 1;
        assert!(immutable_superblock_fields_match(&a, &b));
        b.total_blocks -= 1;
        assert!(!immutable_superblock_fields_match(&a, &b));
    }

    #[test]
    fn inode_golden_offsets_crc_and_round_trip() {
        let fs = sample_superblock();
        let inode = sample_inode();
        let bytes = encode_inode(&inode, &fs).unwrap();
        assert_eq!(&bytes[0..8], &1u64.to_le_bytes());
        assert_eq!(bytes[8], 2);
        assert_eq!(bytes[9], 1);
        assert_eq!(&bytes[10..12], &0o755u16.to_le_bytes());
        assert_eq!(u48_at(&bytes, 104).unwrap(), 100);
        assert_eq!(u32_at(&bytes, 110).unwrap(), 2);
        assert_ne!(u32_at(&bytes, 184).unwrap(), 0);
        assert_eq!(parse_inode(&bytes, &fs), Ok(inode));
    }

    #[test]
    fn inode_rejects_checksum_reserved_nsec_extent_and_metadata_overlap() {
        let fs = sample_superblock();
        let inode = sample_inode();
        let good = encode_inode(&inode, &fs).unwrap();

        let mut bad = good;
        bad[200] = 1;
        let mut copy = bad;
        copy[184..188].fill(0);
        let crc = crc32(&copy);
        bad[184..188].copy_from_slice(&crc.to_le_bytes());
        assert_eq!(parse_inode(&bad, &fs), Err(Error::Reserved));

        let mut bad = inode;
        bad.mtime_nsec = 1_000_000_000;
        assert_eq!(encode_inode(&bad, &fs), Err(Error::Inode));

        let mut bad = inode;
        bad.extents[0].start = fs.inode_table.start;
        assert_eq!(encode_inode(&bad, &fs), Err(Error::Extent));

        let mut raw = good;
        raw[24] ^= 1;
        assert_eq!(parse_inode(&raw, &fs), Err(Error::Checksum));
    }

    #[test]
    fn inode_rejects_unsorted_overlap_and_mismatched_allocated_count() {
        let fs = sample_superblock();
        let mut inode = sample_inode();
        inode.extent_count = 2;
        inode.extents[0] = Extent {
            start: 100,
            blocks: 2,
        };
        inode.extents[1] = Extent {
            start: 101,
            blocks: 1,
        };
        inode.allocated_blocks = 3;
        assert_eq!(encode_inode(&inode, &fs), Err(Error::Extent));

        let mut inode = sample_inode();
        inode.allocated_blocks = 3;
        assert_eq!(encode_inode(&inode, &fs), Err(Error::Inode));
    }
    #[test]
    fn directory_record_round_trip_and_alignment() {
        let mut buf = [0u8; 64];
        let used = encode_dir_record(&mut buf, 2, 1, "welcome.txt").unwrap();
        assert_eq!(used % 8, 0);
        let (entry, consumed) = parse_dir_record(&buf[..used], 1024).unwrap();
        assert_eq!(consumed, used);
        assert_eq!(
            entry,
            DirEntry {
                inode: 2,
                file_type: 1,
                name: "welcome.txt",
            }
        );
    }

    #[test]
    fn directory_record_rejects_bad_names_bounds_types_and_padding() {
        let mut buf = [0u8; 64];
        assert_eq!(
            encode_dir_record(&mut buf, 0, 1, "x"),
            Err(Error::Directory)
        );
        assert_eq!(
            encode_dir_record(&mut buf, 1, 9, "x"),
            Err(Error::Directory)
        );
        assert_eq!(
            encode_dir_record(&mut buf, 1, 1, "a/b"),
            Err(Error::Directory)
        );
        assert_eq!(encode_dir_record(&mut buf, 1, 1, ""), Err(Error::Directory));

        let used = encode_dir_record(&mut buf, 2, 1, "ok").unwrap();
        buf[12] = 1;
        assert_eq!(parse_dir_record(&buf[..used], 1024), Err(Error::Directory));

        let mut buf = [0u8; 64];
        let used = encode_dir_record(&mut buf, 1025, 1, "ok").unwrap();
        assert_eq!(parse_dir_record(&buf[..used], 1024), Err(Error::Directory));

        let mut buf = [0u8; 64];
        let used = encode_dir_record(&mut buf, 2, 1, "ok").unwrap();
        buf[used - 1] = 1;
        assert_eq!(parse_dir_record(&buf[..used], 1024), Err(Error::Directory));
    }
}
