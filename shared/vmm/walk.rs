//! Ownership-checked traversal of one initially empty supervisor PML4 slot.
use super::Error;
use super::address::{ARENA_SLOT, Page, Permissions, PhysicalFrame};
use super::frames::{FrameUse, Frames};

pub const ADDRESS_MASK: u64 = 0x000f_ffff_ffff_f000;
pub const PRESENT: u64 = 1;
pub const WRITE: u64 = 2;
pub const ACCESSED: u64 = 1 << 5;
pub const DIRTY: u64 = 1 << 6;
pub const NX: u64 = 1 << 63;

/// Transport operations, not an allocator. Implementations must bounds-check
/// indices and backing, and must never dereference an unvalidated number.
/// Native construction must establish exclusive RAM/scratch ownership and a
/// single CPU with interrupts disabled. An invalid access must fail-stop.
/// `zero_frame` completes before publication; `invalidate` completes before
/// any detached backing can be recycled. No backend reference may escape.
pub trait Memory {
    fn read_entry(&mut self, frame: u64, index: usize) -> u64;
    fn write_entry(&mut self, frame: u64, index: usize, value: u64);
    fn zero_frame(&mut self, frame: u64);
    fn invalidate(&mut self, page: u64);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Translation {
    pub physical: u64,
    pub permissions: Permissions,
    pub accessed: bool,
    pub dirty: bool,
}

/// Exclusive logical owner of the arena. There is no implicit unmap on Drop:
/// dropping the manager leaks retained backing rather than reclaiming live
/// mappings. This API returns numbers, never safe Rust memory references.
pub struct Vm<M: Memory, const N: usize> {
    pub(super) memory: M,
    pub(super) frames: Frames<N>,
    pub(super) root: u64,
}

impl<M: Memory, const N: usize> Vm<M, N> {
    pub fn new(root: u64, mut memory: M, frames: Frames<N>) -> Result<Self, Error> {
        PhysicalFrame::new(root, frames.physical_bits())?;
        if frames.contains(root) || frames.available() != frames.total() {
            return Err(Error::InvalidRoot);
        }
        if memory.read_entry(root, ARENA_SLOT) != 0 {
            return Err(Error::AlreadyMapped);
        }
        Ok(Self {
            memory,
            frames,
            root,
        })
    }

    pub const fn root_address(&self) -> u64 {
        self.root
    }

    pub fn free_frames(&self) -> usize {
        self.frames.available()
    }

    pub fn table_frames(&self) -> usize {
        self.frames.in_use(FrameUse::Table)
    }

    pub fn data_frames(&self) -> usize {
        self.frames.in_use(FrameUse::Data)
    }

    pub(super) fn table_frame(&self, entry: u64) -> Result<u64, Error> {
        let allowed = ADDRESS_MASK | PRESENT | WRITE | ACCESSED;
        if entry & !allowed != 0 || entry & (PRESENT | WRITE) != PRESENT | WRITE {
            return Err(Error::CorruptEntry);
        }
        let frame = PhysicalFrame::new(entry & ADDRESS_MASK, self.frames.physical_bits())?
            .address();
        if self.frames.kind(frame) != Some(FrameUse::Table) {
            return Err(Error::ForeignTable);
        }
        Ok(frame)
    }

    pub(super) fn path(&mut self, page: Page) -> Result<Option<[u64; 4]>, Error> {
        let indices = page.indices();
        let mut tables = [0; 4];
        tables[0] = self.root;
        for level in 0..3 {
            let entry = self.memory.read_entry(tables[level], indices[level]);
            if entry == 0 {
                return Ok(None);
            }
            let next = self.table_frame(entry)?;
            if tables[..=level].contains(&next) {
                return Err(Error::CorruptEntry);
            }
            tables[level + 1] = next;
        }
        Ok(Some(tables))
    }

    pub(super) fn decode_leaf(&self, entry: u64) -> Result<Translation, Error> {
        let allowed = ADDRESS_MASK | PRESENT | WRITE | ACCESSED | DIRTY | NX;
        if entry & !allowed != 0
            || entry & PRESENT == 0
            || (entry & WRITE != 0 && entry & NX == 0)
        {
            return Err(Error::CorruptEntry);
        }
        let physical = PhysicalFrame::new(entry & ADDRESS_MASK, self.frames.physical_bits())?
            .address();
        if self.frames.kind(physical) != Some(FrameUse::Data) {
            return Err(Error::WrongFrameUse);
        }
        let permissions = if entry & WRITE != 0 {
            Permissions::ReadWrite
        } else if entry & NX == 0 {
            Permissions::ReadExecute
        } else {
            Permissions::ReadOnly
        };
        Ok(Translation {
            physical,
            permissions,
            accessed: entry & ACCESSED != 0,
            dirty: entry & DIRTY != 0,
        })
    }

    pub fn query(&mut self, page: Page) -> Result<Option<Translation>, Error> {
        let Some(tables) = self.path(page)? else {
            return Ok(None);
        };
        let entry = self.memory.read_entry(tables[3], page.indices()[3]);
        if entry == 0 {
            return Ok(None);
        }
        self.decode_leaf(entry).map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vmm::test_support::{ROOT, memory_and_frames, vm};

    fn populate() -> (Vm<crate::vmm::test_support::Ram, 8>, Page, [u64; 3], u64) {
        let mut vm = vm::<8>(8);
        let page = Page::new(super::super::address::ARENA_BASE).unwrap();
        let tables = core::array::from_fn(|_| vm.frames.acquire(FrameUse::Table).unwrap());
        let data = vm.frames.acquire(FrameUse::Data).unwrap();
        vm.memory.write_entry(ROOT, ARENA_SLOT, tables[0] | 3);
        vm.memory.write_entry(tables[0], 0, tables[1] | 3);
        vm.memory.write_entry(tables[1], 0, tables[2] | 3);
        vm.memory.write_entry(tables[2], 0, data | 3 | NX);
        (vm, page, tables, data)
    }

    #[test]
    fn empty_owned_arena_has_no_translation() {
        let mut vm = vm::<8>(8);
        let page = Page::new(super::super::address::ARENA_BASE).unwrap();
        assert_eq!(vm.query(page), Ok(None));
        assert_eq!(vm.root_address(), ROOT);
        assert_eq!(vm.free_frames(), 8);
        assert_eq!(vm.table_frames(), 0);
        assert_eq!(vm.data_frames(), 0);
    }

    #[test]
    fn inherited_root_and_root_in_supply_are_rejected() {
        let (mut memory, frames) = memory_and_frames::<8>(8);
        memory.write_entry(ROOT, ARENA_SLOT, 1);
        assert!(matches!(Vm::new(ROOT, memory, frames), Err(Error::AlreadyMapped)));
        let (memory, mut frames) = memory_and_frames::<8>(7);
        frames.register(ROOT).unwrap();
        assert!(matches!(Vm::new(ROOT, memory, frames), Err(Error::InvalidRoot)));
    }

    #[test]
    fn reads_exact_owned_translation_and_hardware_access_bits() {
        let (mut vm, page, tables, physical) = populate();
        vm.memory
            .write_entry(tables[2], 0, physical | 3 | NX | ACCESSED | DIRTY);
        assert_eq!(
            vm.query(page),
            Ok(Some(Translation {
                physical,
                permissions: Permissions::ReadWrite,
                accessed: true,
                dirty: true,
            }))
        );
    }

    #[test]
    fn rejects_foreign_tables_cycles_and_huge_or_user_entries() {
        for extra in [1 << 2, 1 << 7, NX, 1 << 62] {
            let (mut vm, page, tables, _) = populate();
            vm.memory.write_entry(ROOT, ARENA_SLOT, tables[0] | 3 | extra);
            assert_eq!(vm.query(page), Err(Error::CorruptEntry));
        }
        let (mut vm, page, tables, _) = populate();
        vm.memory.write_entry(tables[0], 0, tables[0] | 3);
        assert_eq!(vm.query(page), Err(Error::CorruptEntry));
        vm.memory.write_entry(ROOT, ARENA_SLOT, 0x9000 | 3);
        assert_eq!(vm.query(page), Err(Error::ForeignTable));
    }

    #[test]
    fn rejects_writable_executable_unowned_and_nonpresent_leaf_metadata() {
        let (mut vm, page, tables, physical) = populate();
        for entry in [physical | 3, physical | NX, physical | 3 | NX | (1 << 8)] {
            vm.memory.write_entry(tables[2], 0, entry);
            assert_eq!(vm.query(page), Err(Error::CorruptEntry));
        }
        vm.memory.write_entry(tables[2], 0, 0x9000 | 3 | NX);
        assert_eq!(vm.query(page), Err(Error::WrongFrameUse));
    }
}
