//! Bounded region allocation with rollback using the production unmap path.
use super::Error;
use super::address::{PageRange, Permissions};
use super::walk::{Memory, Vm};

impl<M: Memory, const N: usize> Vm<M, N> {
    /// Map an entirely absent range. Recoverable failure rolls back exactly
    /// this operation's prefix; unrelated existing mappings remain unchanged.
    /// Backend faults are fail-stop and are not recoverable transaction errors.
    pub fn map_region(&mut self, range: PageRange, permissions: Permissions) -> Result<(), Error> {
        self.map_region_with_privilege(range, permissions, false)
    }

    /// Map an entirely absent user-accessible range inside the VM's already
    /// owned arena. This does not create or widen an address space.
    pub fn map_user_region(
        &mut self,
        range: PageRange,
        permissions: Permissions,
    ) -> Result<(), Error> {
        self.map_region_with_privilege(range, permissions, true)
    }

    fn map_region_with_privilege(
        &mut self,
        range: PageRange,
        permissions: Permissions,
        user: bool,
    ) -> Result<(), Error> {
        for index in 0..range.count() {
            let page = range.page(index).ok_or(Error::InvalidRange)?;
            if self.query(page)?.is_some() {
                return Err(Error::AlreadyMapped);
            }
        }
        for index in 0..range.count() {
            let page = range.page(index).ok_or(Error::InvalidRange)?;
            let mapped = if user {
                self.map_user_zeroed(page, permissions)
            } else {
                self.map_zeroed(page, permissions)
            };
            if let Err(error) = mapped {
                for prior in (0..index).rev() {
                    let mapped = range.page(prior).expect("validated region index");
                    self.unmap(mapped)
                        .expect("rollback owns every successfully mapped prefix page");
                }
                return Err(error);
            }
        }
        Ok(())
    }

    /// Validate the complete range before removing anything. Callers must
    /// retire all raw accesses first. No safe references are issued by VM.
    pub fn unmap_region(&mut self, range: PageRange) -> Result<(), Error> {
        for index in 0..range.count() {
            let page = range.page(index).ok_or(Error::InvalidRange)?;
            self.query(page)?.ok_or(Error::NotMapped)?;
        }
        for index in (0..range.count()).rev() {
            let page = range.page(index).expect("validated region index");
            self.unmap(page)
                .expect("prevalidated region ownership changed during removal");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vmm::address::{ARENA_BASE, ARENA_SLOT, Page};
    use crate::vmm::test_support::{ROOT, vm};

    #[test]
    fn every_partial_exhaustion_rolls_back_tables_and_data() {
        for start in [ARENA_BASE, ARENA_BASE + (1 << 21) - 4096] {
            for count in 0..8 {
                let mut vm = vm::<8>(count);
                let range = PageRange::new(Page::new(start).unwrap(), 5).unwrap();
                assert_eq!(
                    vm.map_region(range, Permissions::ReadWrite),
                    Err(Error::OutOfFrames)
                );
                assert_eq!(vm.free_frames(), count);
                assert_eq!(vm.table_frames(), 0);
                assert_eq!(vm.data_frames(), 0);
                assert_eq!(vm.memory.read_entry(ROOT, ARENA_SLOT), 0);
                for index in 0..range.count() {
                    assert_eq!(vm.query(range.page(index).unwrap()), Ok(None));
                }
            }
        }
    }

    #[test]
    fn user_region_keeps_all_payload_leaves_user_accessible() {
        let mut vm = vm::<12>(12);
        let range = PageRange::new(Page::new(ARENA_BASE).unwrap(), 3).unwrap();
        vm.map_user_region(range, Permissions::ReadWrite).unwrap();
        for index in 0..range.count() {
            let translation = vm.query(range.page(index).unwrap()).unwrap().unwrap();
            assert_eq!(
                translation.privilege,
                crate::vmm::address::Privilege::User
            );
        }
        vm.unmap_region(range).unwrap();
        assert_eq!(vm.free_frames(), 12);
    }

    #[test]
    fn rollback_preserves_an_existing_neighbor_and_shared_tables() {
        let mut vm = vm::<7>(7);
        let neighbor = Page::new(ARENA_BASE).unwrap();
        let physical = vm.map_zeroed(neighbor, Permissions::ReadOnly).unwrap();
        vm.memory.pages.get_mut(&physical).unwrap()[9] = 0xabcd;
        let range = PageRange::new(Page::new(ARENA_BASE + 4096).unwrap(), 4).unwrap();
        assert_eq!(
            vm.map_region(range, Permissions::ReadWrite),
            Err(Error::OutOfFrames)
        );
        assert_eq!(vm.query(neighbor).unwrap().unwrap().physical, physical);
        assert_eq!(vm.memory.pages[&physical][9], 0xabcd);
        assert_eq!(vm.table_frames(), 3);
        assert_eq!(vm.data_frames(), 1);
        assert_eq!(vm.free_frames(), 3);
    }

    #[test]
    fn collision_anywhere_is_detected_before_any_publication() {
        let mut vm = vm::<16>(16);
        let range = PageRange::new(Page::new(ARENA_BASE).unwrap(), 5).unwrap();
        vm.map_zeroed(range.page(3).unwrap(), Permissions::ReadOnly)
            .unwrap();
        let before = vm.memory.pages.clone();
        let events = vm.memory.events.len();
        assert_eq!(
            vm.map_region(range, Permissions::ReadWrite),
            Err(Error::AlreadyMapped)
        );
        assert_eq!(vm.memory.pages, before);
        assert_eq!(vm.memory.events.len(), events);
    }

    #[test]
    fn full_capacity_region_crosses_table_boundary_and_tears_down_cleanly() {
        let mut vm = vm::<80>(80);
        let start = Page::new(ARENA_BASE + (1 << 21) - 4096).unwrap();
        let range = PageRange::new(start, 64).unwrap();
        vm.map_region(range, Permissions::ReadWrite).unwrap();
        let mut seen = [0; 64];
        for index in 0..range.count() {
            let page = range.page(index).unwrap();
            let translation = vm.query(page).unwrap().unwrap();
            assert!(!seen[..index].contains(&translation.physical));
            seen[index] = translation.physical;
            assert!(
                vm.memory.pages[&translation.physical]
                    .iter()
                    .all(|word| *word == 0)
            );
        }
        assert_eq!(vm.table_frames(), 4);
        assert_eq!(vm.data_frames(), 64);
        vm.unmap_region(range).unwrap();
        assert_eq!(vm.free_frames(), 80);
        assert_eq!(vm.memory.read_entry(ROOT, ARENA_SLOT), 0);
    }

    #[test]
    fn missing_page_prevents_partial_region_removal() {
        let mut vm = vm::<16>(16);
        let range = PageRange::new(Page::new(ARENA_BASE).unwrap(), 4).unwrap();
        vm.map_region(range, Permissions::ReadWrite).unwrap();
        vm.unmap(range.page(2).unwrap()).unwrap();
        let before = vm.memory.pages.clone();
        let events = vm.memory.events.len();
        assert_eq!(vm.unmap_region(range), Err(Error::NotMapped));
        assert_eq!(vm.memory.pages, before);
        assert_eq!(vm.memory.events.len(), events);
    }
}
