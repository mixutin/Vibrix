//! Allocate zeroed data and missing hierarchy pages before publishing a leaf.
use super::Error;
use super::address::{Page, Permissions};
use super::frames::FrameUse;
use super::walk::{Memory, NX, PRESENT, Vm, WRITE};

pub(super) fn permission_flags(permissions: Permissions) -> u64 {
    PRESENT
        | if permissions.writable() { WRITE } else { 0 }
        | if permissions.executable() { 0 } else { NX }
}

impl<M: Memory, const N: usize> Vm<M, N> {
    /// Allocate a fresh zeroed page; never overwrite an existing mapping.
    /// Returns its physical number for diagnostics, not ownership or a pointer.
    /// Backend faults fail-stop; recoverable validation/exhaustion errors leave
    /// existing mappings and inventory unchanged.
    pub fn map_zeroed(&mut self, page: Page, permissions: Permissions) -> Result<u64, Error> {
        if self.query(page)?.is_some() {
            return Err(Error::AlreadyMapped);
        }
        let indices = page.indices();
        let mut tables = [0; 4];
        tables[0] = self.root;
        let mut missing = 3;
        for level in 0..3 {
            let entry = self.memory.read_entry(tables[level], indices[level]);
            if entry == 0 {
                missing = level;
                break;
            }
            let next = self.table_frame(entry)?;
            if tables[..=level].contains(&next) {
                return Err(Error::CorruptEntry);
            }
            tables[level + 1] = next;
        }
        let needed = 4 - missing;
        if self.frames.available() < needed {
            return Err(Error::OutOfFrames);
        }
        let mut acquired = [0; 4];
        for index in 0..needed {
            let kind = if index == 0 {
                FrameUse::Data
            } else {
                FrameUse::Table
            };
            match self.frames.acquire(kind) {
                Ok(frame) => acquired[index] = frame,
                Err(error) => {
                    for (prior, frame) in acquired[..index].iter().enumerate() {
                        let prior_kind = if prior == 0 {
                            FrameUse::Data
                        } else {
                            FrameUse::Table
                        };
                        self.frames.release(*frame, prior_kind)?;
                    }
                    return Err(error);
                }
            }
        }
        for frame in &acquired[..needed] {
            self.memory.zero_frame(*frame);
        }
        tables[missing + 1..4].copy_from_slice(&acquired[1..needed]);
        let data = acquired[0];
        self.memory
            .write_entry(tables[3], indices[3], data | permission_flags(permissions));
        // Children are complete before their parent link becomes reachable.
        // The final store publishes the new subtree into the existing hierarchy.
        for level in (missing..3).rev() {
            self.memory.write_entry(
                tables[level],
                indices[level],
                tables[level + 1] | PRESENT | WRITE,
            );
        }
        self.memory.invalidate(page.address());
        Ok(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vmm::address::{ARENA_BASE, ARENA_SLOT, PAGE_BYTES};
    use crate::vmm::test_support::{Event, ROOT, vm};

    #[test]
    fn zeroes_supply_and_publishes_children_before_root_then_flushes() {
        let mut vm = vm::<8>(8);
        let page = Page::new(ARENA_BASE).unwrap();
        let data = vm.map_zeroed(page, Permissions::ReadWrite).unwrap();
        assert!(vm.memory.pages[&data].iter().all(|word| *word == 0));
        assert_eq!(vm.table_frames(), 3);
        assert_eq!(vm.data_frames(), 1);
        assert_eq!(vm.free_frames(), 4);
        let translation = vm.query(page).unwrap().unwrap();
        assert_eq!(translation.physical, data);
        assert_eq!(translation.permissions, Permissions::ReadWrite);
        assert_eq!(
            vm.memory.events[..4]
                .iter()
                .filter(|event| matches!(event, Event::Zero(_)))
                .count(),
            4
        );
        assert!(matches!(
            vm.memory.events[vm.memory.events.len() - 2],
            Event::Write(ROOT, ARENA_SLOT, _)
        ));
        assert_eq!(vm.memory.events.last(), Some(&Event::Invalidate(ARENA_BASE)));
    }

    #[test]
    fn allocates_only_missing_levels_across_two_mib_and_one_gib_boundaries() {
        let mut vm = vm::<16>(16);
        for offset in [0, PAGE_BYTES, 1 << 21, 1 << 30] {
            let page = Page::new(ARENA_BASE + offset).unwrap();
            vm.map_zeroed(page, Permissions::ReadOnly).unwrap();
            assert_eq!(
                vm.query(page).unwrap().unwrap().permissions,
                Permissions::ReadOnly
            );
        }
        assert_eq!(vm.table_frames(), 6);
        assert_eq!(vm.data_frames(), 4);
        assert_eq!(vm.free_frames(), 6);
    }

    #[test]
    fn exhaustion_never_publishes_partial_hierarchy_or_leaks_inventory() {
        for count in 0..4 {
            let mut vm = vm::<8>(count);
            let before = vm.memory.pages.clone();
            let page = Page::new(ARENA_BASE).unwrap();
            assert_eq!(
                vm.map_zeroed(page, Permissions::ReadWrite),
                Err(Error::OutOfFrames)
            );
            assert_eq!(vm.memory.pages, before);
            assert!(vm.memory.events.is_empty());
            assert_eq!(vm.free_frames(), count);
            assert_eq!(vm.query(page), Ok(None));
        }
    }

    #[test]
    fn duplicate_mapping_preserves_bytes_permissions_and_accounting() {
        let mut vm = vm::<8>(8);
        let page = Page::new(ARENA_BASE).unwrap();
        let data = vm.map_zeroed(page, Permissions::ReadWrite).unwrap();
        vm.memory.pages.get_mut(&data).unwrap()[0] = 123;
        let events = vm.memory.events.len();
        assert_eq!(
            vm.map_zeroed(page, Permissions::ReadExecute),
            Err(Error::AlreadyMapped)
        );
        assert_eq!(vm.memory.pages[&data][0], 123);
        assert_eq!(vm.memory.events.len(), events);
        assert_eq!(vm.free_frames(), 4);
        assert_eq!(
            vm.query(page).unwrap().unwrap().permissions,
            Permissions::ReadWrite
        );
    }
}
