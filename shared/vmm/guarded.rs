//! Exclusive guarded-allocation facade: guard pages cannot be mapped through it.
use super::Error;
use super::address::{PAGE_BYTES, Page, PageRange, Permissions};
use super::walk::{Memory, Translation, Vm};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GuardedLayout {
    whole: PageRange,
    payload: PageRange,
}

impl GuardedLayout {
    pub fn new(lower_guard: Page, payload_pages: usize) -> Result<Self, Error> {
        if payload_pages == 0 {
            return Err(Error::InvalidRange);
        }
        let count = payload_pages.checked_add(2).ok_or(Error::InvalidRange)?;
        let whole = PageRange::new(lower_guard, count)?;
        let payload = PageRange::new(
            Page::new(lower_guard.address() + PAGE_BYTES)?,
            payload_pages,
        )?;
        Ok(Self { whole, payload })
    }

    pub fn lower_guard(self) -> Page {
        self.whole.page(0).expect("nonempty guarded range")
    }

    pub fn upper_guard(self) -> Page {
        self.whole
            .page(self.whole.count() - 1)
            .expect("bounded guarded range")
    }

    pub const fn payload(self) -> PageRange {
        self.payload
    }

    /// Exclusive end of writable payload, suitable for a future stack top.
    /// This does not switch RSP, install an IST or grant a Rust reference.
    pub fn payload_end(self) -> u64 {
        self.upper_guard().address()
    }

    fn overlaps(self, other: Self) -> bool {
        self.lower_guard().address() < other.upper_guard().address() + PAGE_BYTES
            && other.lower_guard().address() < self.upper_guard().address() + PAGE_BYTES
    }
}

/// Numeric identity local to one GuardedVm, not a transferable capability.
/// Reusing a slot issues a new identity; old identities cannot free it again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GuardedId(u64);

pub struct GuardedVm<M: Memory, const N: usize, const G: usize> {
    vm: Vm<M, N>,
    slots: [Option<(GuardedId, GuardedLayout)>; G],
    next_id: u64,
}

impl<M: Memory, const N: usize, const G: usize> GuardedVm<M, N, G> {
    /// Takes exclusive ownership of an empty managed arena. No raw mapper or
    /// backend escapes, so every live allocation's two guards remain absent.
    pub fn new(vm: Vm<M, N>) -> Result<Self, Error> {
        if vm.table_frames() != 0 || vm.data_frames() != 0 {
            return Err(Error::ActiveAllocations);
        }
        Ok(Self {
            vm,
            slots: [None; G],
            next_id: 1,
        })
    }

    pub fn allocate(&mut self, layout: GuardedLayout) -> Result<GuardedId, Error> {
        if self
            .slots
            .iter()
            .flatten()
            .any(|(_, existing)| existing.overlaps(layout))
        {
            return Err(Error::AddressInUse);
        }
        let slot = self
            .slots
            .iter()
            .position(Option::is_none)
            .ok_or(Error::Capacity)?;
        let next_id = self.next_id.checked_add(1).ok_or(Error::Capacity)?;
        for index in 0..layout.whole.count() {
            let page = layout.whole.page(index).ok_or(Error::InvalidRange)?;
            if self.vm.query(page)?.is_some() {
                return Err(Error::AddressInUse);
            }
        }
        self.vm.map_region(layout.payload, Permissions::ReadWrite)?;
        let id = GuardedId(self.next_id);
        self.slots[slot] = Some((id, layout));
        self.next_id = next_id;
        Ok(id)
    }

    pub fn layout(&self, id: GuardedId) -> Result<GuardedLayout, Error> {
        self.slots
            .iter()
            .flatten()
            .find(|(candidate, _)| *candidate == id)
            .map(|(_, layout)| *layout)
            .ok_or(Error::InvalidAllocation)
    }

    pub fn release(&mut self, id: GuardedId) -> Result<(), Error> {
        let slot = self
            .slots
            .iter()
            .position(|entry| entry.is_some_and(|(candidate, _)| candidate == id))
            .ok_or(Error::InvalidAllocation)?;
        let layout = self.slots[slot].ok_or(Error::InvalidAllocation)?.1;
        if self.vm.query(layout.lower_guard())?.is_some()
            || self.vm.query(layout.upper_guard())?.is_some()
        {
            return Err(Error::CorruptEntry);
        }
        self.vm.unmap_region(layout.payload)?;
        self.slots[slot] = None;
        Ok(())
    }

    pub fn query(&mut self, page: Page) -> Result<Option<Translation>, Error> {
        self.vm.query(page)
    }

    pub fn free_frames(&self) -> usize {
        self.vm.free_frames()
    }

    pub fn active_allocations(&self) -> usize {
        self.slots.iter().filter(|entry| entry.is_some()).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vmm::address::{ARENA_BASE, ARENA_BYTES};
    use crate::vmm::test_support::vm;

    fn layout(offset: u64, pages: usize) -> GuardedLayout {
        GuardedLayout::new(Page::new(ARENA_BASE + offset).unwrap(), pages).unwrap()
    }

    #[test]
    fn layout_bounds_include_both_guards() {
        let page = Page::new(ARENA_BASE).unwrap();
        for count in [0, 63, usize::MAX] {
            assert_eq!(GuardedLayout::new(page, count), Err(Error::InvalidRange));
        }
        let maximum = GuardedLayout::new(page, 62).unwrap();
        assert_eq!(maximum.payload().count(), 62);
        assert_eq!(maximum.lower_guard().address(), ARENA_BASE);
        assert_eq!(maximum.payload_end(), ARENA_BASE + 63 * PAGE_BYTES);
        let end = Page::new(ARENA_BASE + ARENA_BYTES - 2 * PAGE_BYTES).unwrap();
        assert_eq!(GuardedLayout::new(end, 1), Err(Error::InvalidRange));
    }

    #[test]
    fn guard_overlap_is_reserved_and_payload_is_the_only_mapped_region() {
        let mut guarded = GuardedVm::<_, 16, 2>::new(vm::<16>(16)).unwrap();
        let first = layout(0, 2);
        let id = guarded.allocate(first).unwrap();
        assert_eq!(guarded.layout(id), Ok(first));
        assert_eq!(guarded.query(first.lower_guard()), Ok(None));
        assert_eq!(guarded.query(first.upper_guard()), Ok(None));
        for index in 0..first.payload().count() {
            let page = first.payload().page(index).unwrap();
            assert_eq!(
                guarded.query(page).unwrap().unwrap().permissions,
                Permissions::ReadWrite
            );
        }
        assert_eq!(
            guarded.allocate(layout(3 * PAGE_BYTES, 1)),
            Err(Error::AddressInUse)
        );
        let second = guarded.allocate(layout(4 * PAGE_BYTES, 1)).unwrap();
        assert_eq!(guarded.active_allocations(), 2);
        assert_eq!(
            guarded.allocate(layout(10 * PAGE_BYTES, 1)),
            Err(Error::Capacity)
        );
        guarded.release(id).unwrap();
        guarded.release(second).unwrap();
        assert_eq!(guarded.free_frames(), 16);
        assert_eq!(guarded.active_allocations(), 0);
    }

    #[test]
    fn reused_slot_rejects_old_identity_without_touching_new_allocation() {
        let mut guarded = GuardedVm::<_, 8, 1>::new(vm::<8>(8)).unwrap();
        let allocation = layout(0, 1);
        let old = guarded.allocate(allocation).unwrap();
        guarded.release(old).unwrap();
        let new = guarded.allocate(allocation).unwrap();
        assert_ne!(old, new);
        let events = guarded.vm.memory.events.len();
        assert_eq!(guarded.release(old), Err(Error::InvalidAllocation));
        assert_eq!(guarded.vm.memory.events.len(), events);
        assert_eq!(guarded.active_allocations(), 1);
        guarded.release(new).unwrap();
        assert_eq!(guarded.free_frames(), 8);
    }

    #[test]
    fn failed_allocation_leaves_no_reservation_or_partial_mapping() {
        let mut guarded = GuardedVm::<_, 4, 1>::new(vm::<4>(4)).unwrap();
        assert_eq!(guarded.allocate(layout(0, 2)), Err(Error::OutOfFrames));
        assert_eq!(guarded.active_allocations(), 0);
        assert_eq!(guarded.free_frames(), 4);
        let id = guarded.allocate(layout(0, 1)).unwrap();
        guarded.release(id).unwrap();
        assert_eq!(guarded.free_frames(), 4);
    }

    #[test]
    fn identity_exhaustion_and_corrupted_guard_fail_closed() {
        let mut guarded = GuardedVm::<_, 8, 1>::new(vm::<8>(8)).unwrap();
        guarded.next_id = u64::MAX;
        assert_eq!(guarded.allocate(layout(0, 1)), Err(Error::Capacity));
        assert_eq!(guarded.free_frames(), 8);
        guarded.next_id = 1;
        let allocation = layout(0, 1);
        let id = guarded.allocate(allocation).unwrap();
        // Only this private fixture can bypass the facade to simulate corruption.
        guarded
            .vm
            .map_zeroed(allocation.lower_guard(), Permissions::ReadOnly)
            .unwrap();
        let events = guarded.vm.memory.events.len();
        assert_eq!(guarded.release(id), Err(Error::CorruptEntry));
        assert_eq!(guarded.vm.memory.events.len(), events);
        assert_eq!(guarded.active_allocations(), 1);
    }
}
