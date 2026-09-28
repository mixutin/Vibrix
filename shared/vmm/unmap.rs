//! Detach first, invalidate second, recycle last. Never reclaim the root.
use super::Error;
use super::address::Page;
use super::frames::FrameUse;
use super::walk::{Memory, Translation, Vm};

impl<M: Memory, const N: usize> Vm<M, N> {
    /// Remove one owned mapping and reclaim empty child tables. The returned
    /// translation is historical information, not permission to use the frame.
    /// All caller raw accesses must be retired before this call. The native
    /// backend is single-CPU only; local invalidation is not an SMP shootdown.
    pub fn unmap(&mut self, page: Page) -> Result<Translation, Error> {
        let tables = self.path(page)?.ok_or(Error::NotMapped)?;
        let indices = page.indices();
        let leaf = self.memory.read_entry(tables[3], indices[3]);
        if leaf == 0 {
            return Err(Error::NotMapped);
        }
        let previous = self.decode_leaf(leaf)?;
        // Compute the entire reclaim plan before mutating any table. A
        // nonzero sibling, even nonpresent metadata, prevents reclamation.
        let mut reclaim = [false; 4];
        for level in (1..4).rev() {
            let empty_after_removal = (0..512).all(|index| {
                index == indices[level] || self.memory.read_entry(tables[level], index) == 0
            });
            if !empty_after_removal {
                break;
            }
            reclaim[level] = true;
        }
        self.memory.write_entry(tables[3], indices[3], 0);
        for level in (1..4).rev() {
            if reclaim[level] {
                self.memory
                    .write_entry(tables[level - 1], indices[level - 1], 0);
            }
        }
        // The backend must finish invalidating leaf and paging-structure
        // caches before any former data/table frame can be acquired again.
        self.memory.invalidate(page.address());
        self.frames
            .release(previous.physical, FrameUse::Data)
            .expect("validated data frame ownership changed during unmap");
        for level in (1..4).rev() {
            if reclaim[level] {
                self.frames
                    .release(tables[level], FrameUse::Table)
                    .expect("validated table ownership changed during unmap");
            }
        }
        Ok(previous)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vmm::address::{ARENA_BASE, ARENA_SLOT, Permissions};
    use crate::vmm::test_support::{Event, ROOT, vm};

    #[test]
    fn last_mapping_reclaims_data_and_all_three_tables_after_detachment() {
        let mut vm = vm::<8>(8);
        let page = Page::new(ARENA_BASE).unwrap();
        let physical = vm.map_zeroed(page, Permissions::ReadWrite).unwrap();
        let tables = vm.path(page).unwrap().unwrap();
        vm.memory.events.clear();
        assert_eq!(vm.unmap(page).unwrap().physical, physical);
        assert_eq!(vm.query(page), Ok(None));
        assert_eq!(vm.free_frames(), 8);
        assert_eq!(vm.table_frames(), 0);
        assert_eq!(vm.data_frames(), 0);
        assert_eq!(vm.memory.read_entry(ROOT, ARENA_SLOT), 0);
        assert_eq!(
            vm.memory.events,
            [
                Event::Write(tables[3], 0, 0),
                Event::Write(tables[2], 0, 0),
                Event::Write(tables[1], 0, 0),
                Event::Write(ROOT, ARENA_SLOT, 0),
                Event::Invalidate(ARENA_BASE),
            ]
        );
    }

    #[test]
    fn sibling_mapping_preserves_shared_tables_and_data() {
        let mut vm = vm::<8>(8);
        let first = Page::new(ARENA_BASE).unwrap();
        let second = Page::new(ARENA_BASE + 4096).unwrap();
        vm.map_zeroed(first, Permissions::ReadWrite).unwrap();
        let sibling = vm.map_zeroed(second, Permissions::ReadOnly).unwrap();
        vm.memory.pages.get_mut(&sibling).unwrap()[7] = 0x1234;
        vm.unmap(first).unwrap();
        assert_eq!(vm.table_frames(), 3);
        assert_eq!(vm.data_frames(), 1);
        assert_eq!(vm.query(second).unwrap().unwrap().physical, sibling);
        assert_eq!(vm.memory.pages[&sibling][7], 0x1234);
        vm.unmap(second).unwrap();
        assert_eq!(vm.free_frames(), 8);
    }

    #[test]
    fn cross_level_reclamation_retains_only_live_subtrees() {
        let mut vm = vm::<16>(16);
        let pages = [0, 1 << 21, 1 << 30].map(|offset| Page::new(ARENA_BASE + offset).unwrap());
        for page in pages {
            vm.map_zeroed(page, Permissions::ReadWrite).unwrap();
        }
        assert_eq!(vm.table_frames(), 6);
        vm.unmap(pages[2]).unwrap();
        assert_eq!(vm.table_frames(), 4);
        vm.unmap(pages[1]).unwrap();
        assert_eq!(vm.table_frames(), 3);
        vm.unmap(pages[0]).unwrap();
        assert_eq!(vm.free_frames(), 16);
    }

    #[test]
    fn absent_and_corrupt_mapping_never_free_or_mutate_anything() {
        let mut vm = vm::<8>(8);
        let page = Page::new(ARENA_BASE).unwrap();
        assert_eq!(vm.unmap(page), Err(Error::NotMapped));
        let physical = vm.map_zeroed(page, Permissions::ReadWrite).unwrap();
        let table = vm.path(page).unwrap().unwrap()[3];
        vm.memory.write_entry(table, 0, physical | 3);
        let events = vm.memory.events.len();
        assert_eq!(vm.unmap(page), Err(Error::CorruptEntry));
        assert_eq!(vm.memory.events.len(), events);
        assert_eq!(vm.free_frames(), 4);
    }

    #[test]
    fn reused_frames_are_zeroed_and_double_unmap_is_rejected() {
        let mut vm = vm::<4>(4);
        let page = Page::new(ARENA_BASE).unwrap();
        for _ in 0..100 {
            let physical = vm.map_zeroed(page, Permissions::ReadWrite).unwrap();
            assert!(vm.memory.pages[&physical].iter().all(|word| *word == 0));
            vm.memory.pages.get_mut(&physical).unwrap().fill(u64::MAX);
            vm.unmap(page).unwrap();
            let events = vm.memory.events.len();
            assert_eq!(vm.unmap(page), Err(Error::NotMapped));
            assert_eq!(vm.memory.events.len(), events);
            assert_eq!(vm.free_frames(), 4);
        }
    }
}
