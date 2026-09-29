//! Read-only VibrixFS v1 VFS backend.
//!
//! The driver consumes a block device that represents exactly one already
//! selected Vibrix root partition. It never discovers disks, mutates media or
//! replays the journal. Dirty checkpoints fail closed; writable mounting stays
//! blocked on the native USB durability contract.

use core::cell::UnsafeCell;

use crate::block::{BlockDevice, Transport};

use super::{Entry, Error, Filesystem, Kind, Metadata, Name, NodeId, Result};

#[allow(dead_code)]
#[path = "../../../shared/vibrixfs_wire.rs"]
mod wire;

const MAX_RECORD_BYTES: usize = 272;

struct DeviceCell<T>(UnsafeCell<BlockDevice<T>>);

impl<T: Transport> DeviceCell<T> {
    fn new(device: BlockDevice<T>) -> Self {
        Self(UnsafeCell::new(device))
    }

    fn read_blocks(
        &self,
        lba: u64,
        output: &mut [u8],
    ) -> core::result::Result<(), crate::block::Error> {
        // SAFETY: VibrixFs is owned exclusively by one VFS mount. The current
        // VFS never calls a backend from interrupts or concurrently, and the
        // mutable transport reference remains inside this one operation.
        unsafe { (&mut *self.0.get()).read_blocks(lba, output) }
    }
}

pub struct VibrixFs<T> {
    device: DeviceCell<T>,
    superblock: wire::Superblock,
    sectors_per_block: u64,
}

fn backend<T>(_error: T) -> Error {
    Error::BackendContract
}

fn kind(file_type: u8) -> Result<Kind> {
    match file_type {
        1 => Ok(Kind::File),
        2 => Ok(Kind::Directory),
        4 | 5 => Ok(Kind::Device),
        _ => Err(Error::Unsupported),
    }
}

impl<T: Transport> VibrixFs<T> {
    pub fn mount_read_only(device: BlockDevice<T>, expected_root_guid: [u8; 16]) -> Result<Self> {
        if !device.is_read_only() {
            return Err(Error::AccessDenied);
        }
        let sector = u64::from(device.geometry().sector_bytes());
        if !(wire::BLOCK as u64).is_multiple_of(sector) {
            return Err(Error::BackendContract);
        }
        let sectors_per_block = wire::BLOCK as u64 / sector;
        let sectors = device.geometry().sectors();
        if !sectors.is_multiple_of(sectors_per_block) {
            return Err(Error::BackendContract);
        }
        let partition_blocks = sectors / sectors_per_block;
        if partition_blocks < 2 {
            return Err(Error::BackendContract);
        }

        let cell = DeviceCell::new(device);
        let mut primary = [0u8; wire::BLOCK];
        let mut secondary = [0u8; wire::BLOCK];
        Self::read_block_cell(&cell, sectors_per_block, 0, &mut primary)?;
        Self::read_block_cell(
            &cell,
            sectors_per_block,
            partition_blocks - 1,
            &mut secondary,
        )?;
        let a = wire::parse_superblock(&primary, partition_blocks, &expected_root_guid)
            .map_err(backend)?;
        let b = wire::parse_superblock(&secondary, partition_blocks, &expected_root_guid)
            .map_err(backend)?;
        if !wire::immutable_superblock_fields_match(&a, &b) || !a.clean || !b.clean {
            return Err(Error::BackendContract);
        }
        let superblock = if b.generation > a.generation { b } else { a };
        let fs = Self {
            device: cell,
            superblock,
            sectors_per_block,
        };
        let root = fs.inode(1)?;
        if root.file_type != 2 {
            return Err(Error::NotDirectory);
        }
        Ok(fs)
    }

    fn read_block_cell(
        device: &DeviceCell<T>,
        sectors_per_block: u64,
        block: u64,
        output: &mut [u8; wire::BLOCK],
    ) -> Result<()> {
        let lba = block
            .checked_mul(sectors_per_block)
            .ok_or(Error::BackendContract)?;
        device.read_blocks(lba, output).map_err(backend)
    }

    fn read_block(&self, block: u64, output: &mut [u8; wire::BLOCK]) -> Result<()> {
        if block >= self.superblock.total_blocks {
            return Err(Error::BackendContract);
        }
        Self::read_block_cell(&self.device, self.sectors_per_block, block, output)
    }

    fn bitmap_allocated(&self, range: wire::Range, bit: u64) -> Result<bool> {
        let byte = bit / 8;
        let block_delta = byte / wire::BLOCK as u64;
        if block_delta >= range.blocks {
            return Err(Error::BackendContract);
        }
        let byte_in_block =
            usize::try_from(byte % wire::BLOCK as u64).map_err(|_| Error::BackendContract)?;
        let mut raw = [0u8; wire::BLOCK];
        self.read_block(
            range
                .start
                .checked_add(block_delta)
                .ok_or(Error::BackendContract)?,
            &mut raw,
        )?;
        Ok(raw[byte_in_block] & (1 << (bit & 7)) != 0)
    }

    fn inode(&self, number: u64) -> Result<wire::Inode> {
        if number == 0
            || number > self.superblock.total_inodes
            || !self.bitmap_allocated(self.superblock.inode_bitmap, number)?
        {
            return Err(Error::NotFound);
        }
        let offset = (number - 1)
            .checked_mul(wire::INODE_BYTES as u64)
            .ok_or(Error::BackendContract)?;
        let table_block = offset / wire::BLOCK as u64;
        if table_block >= self.superblock.inode_table.blocks {
            return Err(Error::BackendContract);
        }
        let within =
            usize::try_from(offset % wire::BLOCK as u64).map_err(|_| Error::BackendContract)?;
        let mut raw = [0u8; wire::BLOCK];
        self.read_block(
            self.superblock
                .inode_table
                .start
                .checked_add(table_block)
                .ok_or(Error::BackendContract)?,
            &mut raw,
        )?;
        let end = within
            .checked_add(wire::INODE_BYTES)
            .ok_or(Error::BackendContract)?;
        let inode = wire::parse_inode(
            raw.get(within..end).ok_or(Error::BackendContract)?,
            &self.superblock,
        )
        .map_err(backend)?;
        if inode.number != number {
            return Err(Error::BackendContract);
        }
        for extent in inode.extents[..usize::from(inode.extent_count)].iter() {
            for block in extent.start..extent.start + u64::from(extent.blocks) {
                if !self.bitmap_allocated(self.superblock.block_bitmap, block)? {
                    return Err(Error::BackendContract);
                }
            }
        }
        Ok(inode)
    }

    fn data_block(inode: &wire::Inode, logical: u64) -> Result<u64> {
        let mut remaining = logical;
        for extent in inode.extents[..usize::from(inode.extent_count)].iter() {
            let blocks = u64::from(extent.blocks);
            if remaining < blocks {
                return extent
                    .start
                    .checked_add(remaining)
                    .ok_or(Error::BackendContract);
            }
            remaining -= blocks;
        }
        Err(Error::BackendContract)
    }

    fn read_inode_data(
        &self,
        inode: &wire::Inode,
        offset: usize,
        output: &mut [u8],
    ) -> Result<usize> {
        let size = usize::try_from(inode.size).map_err(|_| Error::BackendContract)?;
        if offset >= size || output.is_empty() {
            return Ok(0);
        }
        let wanted = output.len().min(size - offset);
        let mut done = 0usize;
        let mut block = [0u8; wire::BLOCK];
        while done < wanted {
            let absolute = offset.checked_add(done).ok_or(Error::InvalidOffset)?;
            let logical =
                u64::try_from(absolute / wire::BLOCK).map_err(|_| Error::InvalidOffset)?;
            let within = absolute % wire::BLOCK;
            self.read_block(Self::data_block(inode, logical)?, &mut block)?;
            let count = (wire::BLOCK - within).min(wanted - done);
            output[done..done + count].copy_from_slice(&block[within..within + count]);
            done += count;
        }
        Ok(done)
    }

    fn directory_record(
        &self,
        directory: &wire::Inode,
        offset: usize,
        storage: &mut [u8; MAX_RECORD_BYTES],
    ) -> Result<Option<(u64, u8, Name, usize)>> {
        let size = usize::try_from(directory.size).map_err(|_| Error::BackendContract)?;
        if offset == size {
            return Ok(None);
        }
        if offset > size || size - offset < 16 {
            return Err(Error::BackendContract);
        }
        let mut header = [0u8; 16];
        if self.read_inode_data(directory, offset, &mut header)? != header.len() {
            return Err(Error::BackendContract);
        }
        let record = usize::from(u16::from_le_bytes([header[8], header[9]]));
        if !(16..=MAX_RECORD_BYTES).contains(&record)
            || !record.is_multiple_of(8)
            || offset.checked_add(record).is_none_or(|end| end > size)
        {
            return Err(Error::BackendContract);
        }
        if self.read_inode_data(directory, offset, &mut storage[..record])? != record {
            return Err(Error::BackendContract);
        }
        let (entry, consumed) =
            wire::parse_dir_record(&storage[..record], self.superblock.total_inodes)
                .map_err(backend)?;
        if consumed != record {
            return Err(Error::BackendContract);
        }
        let target = self.inode(entry.inode)?;
        if target.file_type != entry.file_type {
            return Err(Error::BackendContract);
        }
        let name = Name::new(entry.name)?;
        Ok(Some((entry.inode, entry.file_type, name, record)))
    }
}

impl<T: Transport> Filesystem for VibrixFs<T> {
    fn root(&self) -> NodeId {
        NodeId(1)
    }

    fn metadata(&self, id: NodeId) -> Result<Metadata> {
        let inode = self.inode(id.0)?;
        Ok(Metadata {
            kind: kind(inode.file_type)?,
            len: usize::try_from(inode.size).map_err(|_| Error::BackendContract)?,
        })
    }

    fn lookup(&self, dir: NodeId, name: &str) -> Result<NodeId> {
        Name::new(name)?;
        let inode = self.inode(dir.0)?;
        if inode.file_type != 2 {
            return Err(Error::NotDirectory);
        }
        let mut offset = 0usize;
        let mut raw = [0u8; MAX_RECORD_BYTES];
        while let Some((number, _, entry_name, used)) =
            self.directory_record(&inode, offset, &mut raw)?
        {
            if entry_name.as_str() == name {
                return Ok(NodeId(number));
            }
            offset = offset.checked_add(used).ok_or(Error::BackendContract)?;
        }
        Err(Error::NotFound)
    }

    fn entry(&self, dir: NodeId, index: usize) -> Result<Option<Entry>> {
        let inode = self.inode(dir.0)?;
        if inode.file_type != 2 {
            return Err(Error::NotDirectory);
        }
        let mut offset = 0usize;
        let mut visible = 0usize;
        let mut raw = [0u8; MAX_RECORD_BYTES];
        while let Some((number, file_type, name, used)) =
            self.directory_record(&inode, offset, &mut raw)?
        {
            offset = offset.checked_add(used).ok_or(Error::BackendContract)?;
            if matches!(name.as_str(), "." | "..") {
                continue;
            }
            if visible == index {
                return Ok(Some(Entry {
                    name,
                    id: NodeId(number),
                    kind: kind(file_type)?,
                }));
            }
            visible += 1;
        }
        Ok(None)
    }

    fn create(&mut self, _dir: NodeId, _name: &str, _kind: Kind) -> Result<NodeId> {
        Err(Error::ReadOnly)
    }

    fn remove(&mut self, _dir: NodeId, _name: &str) -> Result<()> {
        Err(Error::ReadOnly)
    }

    fn read(&mut self, id: NodeId, offset: usize, buffer: &mut [u8]) -> Result<usize> {
        let inode = self.inode(id.0)?;
        if inode.file_type == 2 {
            return Err(Error::IsDirectory);
        }
        if inode.file_type != 1 {
            return Err(Error::Unsupported);
        }
        self.read_inode_data(&inode, offset, buffer)
    }

    fn write(&mut self, _id: NodeId, _offset: usize, _buffer: &[u8]) -> Result<usize> {
        Err(Error::ReadOnly)
    }

    fn truncate(&mut self, _id: NodeId) -> Result<()> {
        Err(Error::ReadOnly)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block;
    use std::{vec, vec::Vec};

    const ROOT_GUID: [u8; 16] = [0x42; 16];

    fn set_bit(bitmap: &mut [u8], bit: usize) {
        bitmap[bit / 8] |= 1 << (bit & 7);
    }

    fn fixture() -> Vec<u8> {
        let blocks = 4096usize;
        let mut image = vec![0u8; blocks * wire::BLOCK];
        let sb = wire::Superblock {
            minor: 0,
            clean: true,
            generation: 1,
            total_blocks: blocks as u64,
            total_inodes: 16,
            block_bitmap: wire::Range {
                start: 1,
                blocks: 1,
            },
            inode_bitmap: wire::Range {
                start: 2,
                blocks: 1,
            },
            inode_table: wire::Range {
                start: 3,
                blocks: 1,
            },
            journal: None,
            filesystem_uuid: [0x24; 16],
            root_partition_guid: ROOT_GUID,
        };
        let superblock = wire::encode_superblock(&sb, blocks as u64, &ROOT_GUID).unwrap();
        image[..wire::BLOCK].copy_from_slice(&superblock);
        image[(blocks - 1) * wire::BLOCK..blocks * wire::BLOCK].copy_from_slice(&superblock);

        for bit in [0usize, 1, 2, 3, 4, 5, blocks - 1] {
            set_bit(&mut image[wire::BLOCK..2 * wire::BLOCK], bit);
        }
        set_bit(&mut image[2 * wire::BLOCK..3 * wire::BLOCK], 0);
        set_bit(&mut image[2 * wire::BLOCK..3 * wire::BLOCK], 1);
        set_bit(&mut image[2 * wire::BLOCK..3 * wire::BLOCK], 2);

        let empty = wire::Extent {
            start: 0,
            blocks: 0,
        };
        let mut root_extents = [empty; 6];
        root_extents[0] = wire::Extent {
            start: 4,
            blocks: 1,
        };
        let root = wire::Inode {
            number: 1,
            file_type: 2,
            mode: 0o755,
            uid: 0,
            gid: 0,
            links: 2,
            size: 72,
            allocated_blocks: 1,
            atime_sec: 1,
            atime_nsec: 0,
            mtime_sec: 1,
            mtime_nsec: 0,
            ctime_sec: 1,
            ctime_nsec: 0,
            nonce: [1; 16],
            extents: root_extents,
            extent_count: 1,
            device: 0,
            flags: 0,
        };
        let mut file_extents = [empty; 6];
        file_extents[0] = wire::Extent {
            start: 5,
            blocks: 1,
        };
        let file = wire::Inode {
            number: 2,
            file_type: 1,
            mode: 0o644,
            uid: 0,
            gid: 0,
            links: 1,
            size: 6,
            allocated_blocks: 1,
            atime_sec: 1,
            atime_nsec: 0,
            mtime_sec: 1,
            mtime_nsec: 0,
            ctime_sec: 1,
            ctime_nsec: 0,
            nonce: [2; 16],
            extents: file_extents,
            extent_count: 1,
            device: 0,
            flags: 0,
        };
        let root_raw = wire::encode_inode(&root, &sb).unwrap();
        let file_raw = wire::encode_inode(&file, &sb).unwrap();
        image[3 * wire::BLOCK..3 * wire::BLOCK + wire::INODE_BYTES].copy_from_slice(&root_raw);
        image[3 * wire::BLOCK + wire::INODE_BYTES..3 * wire::BLOCK + 2 * wire::INODE_BYTES]
            .copy_from_slice(&file_raw);

        let dir = &mut image[4 * wire::BLOCK..5 * wire::BLOCK];
        let mut at = 0;
        at += wire::encode_dir_record(&mut dir[at..], 1, 2, ".").unwrap();
        at += wire::encode_dir_record(&mut dir[at..], 1, 2, "..").unwrap();
        at += wire::encode_dir_record(&mut dir[at..], 2, 1, "hello").unwrap();
        assert_eq!(at, root.size as usize);
        image[5 * wire::BLOCK..5 * wire::BLOCK + 6].copy_from_slice(b"Vibrix");
        image
    }

    #[test]
    fn mounts_and_reads_named_file_through_vfs_contract() {
        let mut image = fixture();
        let device = block::ram_device(&mut image, 512, true).unwrap();
        let mut fs = VibrixFs::mount_read_only(device, ROOT_GUID).unwrap();
        assert_eq!(fs.metadata(fs.root()).unwrap().kind, Kind::Directory);
        let file = fs.lookup(fs.root(), "hello").unwrap();
        assert_eq!(
            fs.metadata(file).unwrap(),
            Metadata {
                kind: Kind::File,
                len: 6
            }
        );
        let mut bytes = [0u8; 8];
        assert_eq!(fs.read(file, 0, &mut bytes).unwrap(), 6);
        assert_eq!(&bytes[..6], b"Vibrix");
        let entry = fs.entry(fs.root(), 0).unwrap().unwrap();
        assert_eq!(entry.name.as_str(), "hello");
        assert_eq!(entry.id, file);
        assert_eq!(fs.entry(fs.root(), 1).unwrap(), None);
        assert_eq!(fs.write(file, 0, b"x"), Err(Error::ReadOnly));
    }

    #[test]
    fn refuses_dirty_or_wrong_identity_media() {
        let mut image = fixture();
        let mut sb =
            wire::parse_superblock(&image[4095 * wire::BLOCK..], 4096, &ROOT_GUID).unwrap();
        sb.clean = false;
        let crc = wire::encode_superblock(&sb, 4096, &ROOT_GUID).unwrap();
        image[..wire::BLOCK].copy_from_slice(&crc);
        image[4095 * wire::BLOCK..4096 * wire::BLOCK].copy_from_slice(&crc);
        let device = block::ram_device(&mut image, 4096, true).unwrap();
        assert!(matches!(
            VibrixFs::mount_read_only(device, ROOT_GUID),
            Err(Error::BackendContract)
        ));

        let mut image = fixture();
        let device = block::ram_device(&mut image, 4096, true).unwrap();
        assert!(matches!(
            VibrixFs::mount_read_only(device, [0x99; 16]),
            Err(Error::BackendContract)
        ));
    }
}
