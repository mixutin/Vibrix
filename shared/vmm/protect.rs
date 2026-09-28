//! In-place W^X-preserving transitions, followed by synchronous invalidation.
use super::Error;
use super::address::{Page, Permissions};
use super::map::permission_flags;
use super::walk::{Memory, NX, Translation, Vm, WRITE};

impl<M: Memory, const N: usize> Vm<M, N> {
    /// Change only W/NX, preserving physical ownership and hardware A/D bits.
    /// Return the previous translation. Callers must retire incompatible raw
    /// pointer accesses before changing protection; no safe references escape
    /// this numeric API. Unchanged protection does not write or flush.
    pub fn protect(&mut self, page: Page, permissions: Permissions) -> Result<Translation, Error> {
        let tables = self.path(page)?.ok_or(Error::NotMapped)?;
        let index = page.indices()[3];
        let current = self.memory.read_entry(tables[3], index);
        if current == 0 {
            return Err(Error::NotMapped);
        }
        let previous = self.decode_leaf(current)?;
        let updated = (current & !(WRITE | NX))
            | (permission_flags(permissions, previous.privilege) & (WRITE | NX));
        if updated != current {
            self.memory.write_entry(tables[3], index, updated);
            self.memory.invalidate(page.address());
        }
        Ok(previous)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vmm::address::ARENA_BASE;
    use crate::vmm::test_support::{Event, vm};
    use crate::vmm::walk::{ACCESSED, DIRTY};

    #[test]
    fn all_permission_transitions_preserve_backing_and_access_bits() {
        let permissions = [
            Permissions::ReadOnly,
            Permissions::ReadWrite,
            Permissions::ReadExecute,
        ];
        for before in permissions {
            for after in permissions {
                let mut vm = vm::<8>(8);
                let page = Page::new(ARENA_BASE).unwrap();
                let physical = vm.map_zeroed(page, before).unwrap();
                let table = vm.path(page).unwrap().unwrap()[3];
                let entry = vm.memory.read_entry(table, 0) | ACCESSED | DIRTY;
                vm.memory.write_entry(table, 0, entry);
                let events = vm.memory.events.len();
                let previous = vm.protect(page, after).unwrap();
                assert_eq!(previous.permissions, before);
                assert_eq!(previous.physical, physical);
                let current = vm.query(page).unwrap().unwrap();
                assert_eq!(current.permissions, after);
                assert_eq!(current.physical, physical);
                assert!(current.accessed && current.dirty);
                assert_eq!(vm.free_frames(), 4);
                if before == after {
                    assert_eq!(vm.memory.events.len(), events);
                } else {
                    assert_eq!(vm.memory.events.len(), events + 2);
                    assert_eq!(
                        vm.memory.events.last(),
                        Some(&Event::Invalidate(page.address()))
                    );
                }
                let leaf = vm.memory.read_entry(table, 0);
                assert!(!(leaf & WRITE != 0 && leaf & NX == 0));
            }
        }
    }

    #[test]
    fn missing_or_corrupt_mapping_is_not_changed() {
        let mut vm = vm::<8>(8);
        let page = Page::new(ARENA_BASE).unwrap();
        assert_eq!(
            vm.protect(page, Permissions::ReadOnly),
            Err(Error::NotMapped)
        );
        assert!(vm.memory.events.is_empty());
        let physical = vm.map_zeroed(page, Permissions::ReadWrite).unwrap();
        let table = vm.path(page).unwrap().unwrap()[3];
        vm.memory.write_entry(table, 0, physical | 3);
        let events = vm.memory.events.len();
        assert_eq!(
            vm.protect(page, Permissions::ReadOnly),
            Err(Error::CorruptEntry)
        );
        assert_eq!(vm.memory.events.len(), events);
        assert_eq!(vm.memory.read_entry(table, 0), physical | 3);
    }

    #[test]
    fn protection_changes_preserve_user_access() {
        let mut vm = vm::<8>(8);
        let page = Page::new(ARENA_BASE).unwrap();
        vm.map_user_zeroed(page, Permissions::ReadWrite).unwrap();
        let before = vm.protect(page, Permissions::ReadExecute).unwrap();
        assert_eq!(before.privilege, crate::vmm::address::Privilege::User);
        let after = vm.query(page).unwrap().unwrap();
        assert_eq!(after.privilege, crate::vmm::address::Privilege::User);
        assert_eq!(after.permissions, Permissions::ReadExecute);
    }

    #[test]
    fn adjacent_mapping_keeps_its_original_permissions() {
        let mut vm = vm::<8>(8);
        let first = Page::new(ARENA_BASE).unwrap();
        let second = Page::new(ARENA_BASE + 4096).unwrap();
        vm.map_zeroed(first, Permissions::ReadWrite).unwrap();
        vm.map_zeroed(second, Permissions::ReadWrite).unwrap();
        vm.protect(first, Permissions::ReadExecute).unwrap();
        assert_eq!(
            vm.query(second).unwrap().unwrap().permissions,
            Permissions::ReadWrite
        );
    }
}
