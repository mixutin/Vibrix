//! Bounded early heap over loader-mapped kernel BSS; no physical dereferences.
use core::alloc::Layout;

const GRANULE: usize = 16;
const UNITS: usize = 4096;
pub const CAPACITY: usize = GRANULE * UNITS;
const CONTINUATION: u16 = u16::MAX;

/// Zero means free, a run head stores its length, remaining units are tagged.
/// The metadata lives outside the arena so allocation writes cannot overwrite it.
struct Runs {
    entries: [u16; UNITS],
}

impl Runs {
    const fn new() -> Self {
        Self {
            entries: [0; UNITS],
        }
    }

    fn allocate(&mut self, base: usize, layout: Layout) -> Option<usize> {
        base.checked_add(CAPACITY)?;
        let units = layout.size().max(1).checked_add(GRANULE - 1)? / GRANULE;
        if units > UNITS {
            return None;
        }
        for first in 0..=UNITS - units {
            let offset = first * GRANULE;
            if !(base + offset).is_multiple_of(layout.align())
                || self.entries[first..first + units].iter().any(|&v| v != 0)
            {
                continue;
            }
            self.entries[first] = units as u16;
            self.entries[first + 1..first + units].fill(CONTINUATION);
            return Some(offset);
        }
        None
    }

    fn release(&mut self, offset: usize) -> bool {
        if offset >= CAPACITY || !offset.is_multiple_of(GRANULE) {
            return false;
        }
        let first = offset / GRANULE;
        let units = self.entries[first] as usize;
        if units == 0 || units == CONTINUATION as usize || units > UNITS - first {
            return false;
        }
        self.entries[first..first + units].fill(0);
        true
    }
}

#[cfg(target_os = "none")]
pub mod runtime {
    use super::*;
    use core::cell::UnsafeCell;

    #[repr(C, align(4096))]
    struct Arena([u8; CAPACITY]);

    struct Heap {
        arena: UnsafeCell<Arena>,
        runs: UnsafeCell<Runs>,
    }

    // SAFETY: access is restricted to the sole boot CPU with IRQs disabled.
    // Replace this ownership policy before introducing asynchronous heap users.
    unsafe impl Sync for Heap {}

    static HEAP: Heap = Heap {
        arena: UnsafeCell::new(Arena([0; CAPACITY])),
        runs: UnsafeCell::new(Runs::new()),
    };

    /// Claim mapped bytes; null means exhaustion. Contents are unspecified.
    ///
    /// # Safety
    /// Sole boot CPU, interrupts disabled, no reentrant heap operations.
    /// Caller owns the returned layout-sized span until `deallocate` and must
    /// initialize bytes before reading them. No firmware or allocator calls occur.
    pub unsafe fn allocate(layout: Layout) -> *mut u8 {
        let base = HEAP.arena.get().cast::<u8>();
        // SAFETY: sole CPU/IRQs-off; metadata never aliases the returned arena.
        let runs = unsafe { &mut *HEAP.runs.get() };
        match runs.allocate(base as usize, layout) {
            // SAFETY: the checked offset is inside the permanent mapped arena.
            Some(offset) => unsafe { base.add(offset) },
            None => core::ptr::null_mut(),
        }
    }

    /// Release a complete live allocation; never free an interior pointer.
    ///
    /// # Safety
    /// Same CPU/IRQ restrictions as `allocate`. `pointer` must be the exact
    /// result of a live allocation from this heap. All its references and raw
    /// pointer users must be retired before freeing, and never used again.
    pub unsafe fn deallocate(pointer: *mut u8) -> bool {
        let base = HEAP.arena.get().cast::<u8>() as usize;
        let Some(offset) = (pointer as usize).checked_sub(base) else {
            return false;
        };
        // SAFETY: exclusive metadata access follows the caller's CPU contract.
        unsafe { (&mut *HEAP.runs.get()).release(offset) }
    }

    /// Exercise independent aligned allocations, real RAM writes and reuse.
    ///
    /// # Safety
    /// Same sole-CPU and interrupts-disabled preconditions as `allocate`.
    pub unsafe fn smoke_test() -> Result<(), ()> {
        let small = Layout::from_size_align(37, 64).map_err(|_| ())?;
        let page = Layout::from_size_align(4096, 4096).map_err(|_| ())?;
        // SAFETY: inherited early-boot allocation ownership contract.
        let (a, b) = unsafe { (allocate(small), allocate(page)) };
        if a.is_null() || b.is_null() || a == b {
            return Err(());
        }
        if !(a as usize).is_multiple_of(64) || !(b as usize).is_multiple_of(4096) {
            return Err(());
        }
        // SAFETY: a and b own disjoint mapped spans of these exact lengths.
        // Volatile operations make the QEMU check exercise actual backing RAM.
        unsafe {
            for i in 0..37 {
                a.add(i).write_volatile(0xa5);
            }
            for i in 0..4096 {
                b.add(i).write_volatile(0x5a);
            }
            for i in 0..37 {
                if a.add(i).read_volatile() != 0xa5 {
                    return Err(());
                }
            }
            for i in 0..4096 {
                if b.add(i).read_volatile() != 0x5a {
                    return Err(());
                }
            }
            if !deallocate(a) {
                return Err(());
            }
            let reused = allocate(small);
            if reused != a {
                return Err(());
            }
            reused.write_volatile(0x3c);
            if reused.read_volatile() != 0x3c || b.read_volatile() != 0x5a {
                return Err(());
            }
            if !deallocate(reused) || !deallocate(b) {
                return Err(());
            }
        }
        Ok(())
    }
}

#[cfg(target_os = "none")]
pub use runtime::smoke_test;

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(size: usize, align: usize) -> Layout {
        Layout::from_size_align(size, align).unwrap()
    }

    #[test]
    fn alignment_uses_actual_arena_base() {
        let mut runs = Runs::new();
        assert_eq!(runs.allocate(0x1010, layout(1, 4096)), Some(4080));
        assert_eq!(runs.allocate(0x1010, layout(17, 16)), Some(0));
    }

    #[test]
    fn exhaustion_free_and_reuse() {
        let mut runs = Runs::new();
        assert_eq!(runs.allocate(0x1000, layout(CAPACITY, 4096)), Some(0));
        assert_eq!(runs.allocate(0x1000, layout(1, 1)), None);
        assert!(runs.release(0));
        assert_eq!(runs.allocate(0x1000, layout(CAPACITY, 4096)), Some(0));
    }

    #[test]
    fn fragmentation_never_overlaps_live_allocations() {
        let mut runs = Runs::new();
        let chunk = layout(CAPACITY / 4, 16);
        for i in 0..4 {
            assert_eq!(runs.allocate(0x1000, chunk), Some(i * CAPACITY / 4));
        }
        assert!(runs.release(0));
        assert!(runs.release(CAPACITY / 2));
        assert_eq!(runs.allocate(0x1000, layout(CAPACITY / 2, 16)), None);
        assert!(runs.release(CAPACITY / 4));
        assert_eq!(runs.allocate(0x1000, layout(CAPACITY / 2, 16)), Some(0));
        assert!(!runs.release(CAPACITY - 16));
    }

    #[test]
    fn invalid_release_does_not_free_live_run() {
        let mut runs = Runs::new();
        assert_eq!(runs.allocate(0x1000, layout(32, 16)), Some(0));
        for bad in [1, 16, 32, CAPACITY, usize::MAX] {
            assert!(!runs.release(bad));
        }
        assert_eq!(runs.allocate(0x1000, layout(16, 16)), Some(32));
        assert!(runs.release(0));
        assert!(!runs.release(0));
    }

    #[test]
    fn oversized_and_overflow_requests_leave_state_unchanged() {
        let mut runs = Runs::new();
        assert_eq!(runs.allocate(0x1000, layout(CAPACITY + 1, 1)), None);
        assert_eq!(runs.allocate(usize::MAX - 15, layout(1, 1)), None);
        assert_eq!(runs.allocate(0x1000, layout(isize::MAX as usize, 1)), None);
        assert_eq!(runs.allocate(0x1000, layout(16, 16)), Some(0));
    }

    #[test]
    fn zero_size_reserves_one_granule() {
        let mut runs = Runs::new();
        assert_eq!(runs.allocate(0x1000, layout(0, 16)), Some(0));
        assert_eq!(runs.allocate(0x1000, layout(0, 16)), Some(16));
        assert!(runs.release(0));
    }
}
