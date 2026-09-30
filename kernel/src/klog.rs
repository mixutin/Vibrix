//! Bounded structured kernel event log.
//!
//! Records are publish-once for the lifetime of the boot. Writers reserve a
//! unique slot atomically, initialize its record through UnsafeCell, then
//! publish with Release ordering. Readers use Acquire before copying a record.
//! Slots are never overwritten, so a published record cannot race with a later
//! mutation. Capacity exhaustion is explicit.

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, AtomicU16, AtomicUsize, Ordering};

pub const KERNEL_LOG_CAPACITY: usize = 128;
const ALL_SUBSYSTEMS: u8 = 0xff;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Trace = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Subsystem {
    Kernel = 0,
    Memory = 1,
    Process = 2,
    Vfs = 3,
    Device = 4,
    Network = 5,
    Usb = 6,
    Security = 7,
}

impl Subsystem {
    pub const fn bit(self) -> u8 {
        1u8 << self as u8
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Filter {
    pub minimum_level: Level,
    pub subsystem_mask: u8,
}

impl Filter {
    pub const ALL: Self = Self {
        minimum_level: Level::Trace,
        subsystem_mask: ALL_SUBSYSTEMS,
    };

    pub const fn new(minimum_level: Level, subsystem_mask: u8) -> Self {
        Self {
            minimum_level,
            subsystem_mask,
        }
    }

    pub const fn allows(self, level: Level, subsystem: Subsystem) -> bool {
        level as u8 >= self.minimum_level as u8 && self.subsystem_mask & subsystem.bit() != 0
    }

    const fn packed(self) -> u16 {
        ((self.minimum_level as u16) << 8) | self.subsystem_mask as u16
    }

    fn from_packed(value: u16) -> Self {
        let minimum_level = match ((value >> 8) & 0xff) as u8 {
            0 => Level::Trace,
            1 => Level::Debug,
            2 => Level::Info,
            3 => Level::Warn,
            _ => Level::Error,
        };
        Self {
            minimum_level,
            subsystem_mask: value as u8,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Record {
    pub sequence: u64,
    pub boot_id_hi: u64,
    pub boot_id_lo: u64,
    pub monotonic_ticks: u64,
    pub wall_clock_seconds: u64,
    pub level: Level,
    pub subsystem: Subsystem,
    pub event: u16,
    pub value0: u64,
    pub value1: u64,
}

impl Record {
    const EMPTY: Self = Self {
        sequence: 0,
        boot_id_hi: 0,
        boot_id_lo: 0,
        monotonic_ticks: 0,
        wall_clock_seconds: 0,
        level: Level::Trace,
        subsystem: Subsystem::Kernel,
        event: 0,
        value0: 0,
        value1: 0,
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Full;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogError {
    Filtered,
    Full,
}

struct Slot {
    published: AtomicBool,
    record: UnsafeCell<Record>,
}

impl Slot {
    const fn new() -> Self {
        Self {
            published: AtomicBool::new(false),
            record: UnsafeCell::new(Record::EMPTY),
        }
    }
}

// SAFETY: each slot has exactly one writer for the lifetime of the Buffer.
// Reservation assigns a unique index. That writer initializes the UnsafeCell
// before the Release publication. Readers copy only after an Acquire load and
// no published slot is ever mutated again.
unsafe impl Sync for Slot {}

pub struct Buffer<const N: usize> {
    next: AtomicUsize,
    slots: [Slot; N],
}

impl<const N: usize> Buffer<N> {
    pub const fn new() -> Self {
        Self {
            next: AtomicUsize::new(0),
            slots: [const { Slot::new() }; N],
        }
    }

    pub const fn capacity(&self) -> usize {
        N
    }

    /// Number of reserved slots. A concurrently reserved final slot may not yet
    /// be published, so callers must still use get to observe a record.
    pub fn reserved(&self) -> usize {
        self.next.load(Ordering::Acquire).min(N)
    }

    pub fn push(
        &self,
        level: Level,
        subsystem: Subsystem,
        event: u16,
        value0: u64,
        value1: u64,
    ) -> Result<u64, Full> {
        let index = self
            .next
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                (current < N).then_some(current + 1)
            })
            .map_err(|_| Full)?;

        let stamp = crate::boot_clock::stamp();
        let record = Record {
            sequence: index as u64,
            boot_id_hi: stamp.boot_id_hi,
            boot_id_lo: stamp.boot_id_lo,
            monotonic_ticks: stamp.monotonic_ticks,
            wall_clock_seconds: stamp.wall_clock_seconds,
            level,
            subsystem,
            event,
            value0,
            value1,
        };

        // SAFETY: successful reservation gives this caller exclusive ownership
        // of exactly one never-before-written slot. The slot is not observable
        // until the Release store below and is never mutated afterward.
        unsafe { self.slots[index].record.get().write(record) };
        self.slots[index].published.store(true, Ordering::Release);
        Ok(record.sequence)
    }

    pub fn get(&self, sequence: u64) -> Option<Record> {
        let index = usize::try_from(sequence).ok()?;
        let slot = self.slots.get(index)?;
        if !slot.published.load(Ordering::Acquire) {
            return None;
        }

        // SAFETY: Acquire observes the unique writer's initialization before
        // its Release publication. Published records are immutable forever.
        Some(unsafe { slot.record.get().read() })
    }
}

impl<const N: usize> Default for Buffer<N> {
    fn default() -> Self {
        Self::new()
    }
}

pub static KERNEL_LOG: Buffer<KERNEL_LOG_CAPACITY> = Buffer::new();
static FILTER: AtomicU16 = AtomicU16::new(Filter::ALL.packed());

pub fn filter() -> Filter {
    Filter::from_packed(FILTER.load(Ordering::Acquire))
}

pub fn configure_filter(value: Filter) {
    FILTER.store(value.packed(), Ordering::Release);
}

pub fn log(
    level: Level,
    subsystem: Subsystem,
    event: u16,
    value0: u64,
    value1: u64,
) -> Result<u64, LogError> {
    if !filter().allows(level, subsystem) {
        return Err(LogError::Filtered);
    }
    KERNEL_LOG
        .push(level, subsystem, event, value0, value1)
        .map_err(|_| LogError::Full)
}

pub fn self_test() -> Result<(), &'static str> {
    const EVENT_SELF_TEST: u16 = 0x1801;
    let original_filter = filter();
    configure_filter(Filter::new(
        Level::Warn,
        Subsystem::Kernel.bit() | Subsystem::Security.bit(),
    ));
    let info_filtered =
        log(Level::Info, Subsystem::Kernel, EVENT_SELF_TEST, 0, 0) == Err(LogError::Filtered);
    let subsystem_filtered =
        log(Level::Error, Subsystem::Network, EVENT_SELF_TEST, 0, 0) == Err(LogError::Filtered);
    let accepted = log(
        Level::Error,
        Subsystem::Kernel,
        EVENT_SELF_TEST,
        0x56,
        0x4258,
    );
    configure_filter(original_filter);
    if !info_filtered || !subsystem_filtered {
        return Err("structured log filter policy accepted a rejected event");
    }
    let sequence = accepted.map_err(|_| "global structured log is unexpectedly full")?;
    let record = KERNEL_LOG
        .get(sequence)
        .ok_or("published structured record is not readable")?;
    if record.sequence != sequence
        || record.level != Level::Error
        || record.subsystem != Subsystem::Kernel
        || record.event != EVENT_SELF_TEST
        || record.value0 != 0x56
        || record.value1 != 0x4258
    {
        return Err("structured record changed after publication");
    }
    let stamp = crate::boot_clock::stamp();
    if crate::boot_clock::initialized()
        && (record.boot_id_hi == 0 && record.boot_id_lo == 0
            || record.wall_clock_seconds == 0
            || record.boot_id_hi != stamp.boot_id_hi
            || record.boot_id_lo != stamp.boot_id_lo)
    {
        return Err("structured record lost boot identity or wall-clock metadata");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_enforces_level_and_subsystem_mask() {
        let filter = Filter::new(
            Level::Warn,
            Subsystem::Kernel.bit() | Subsystem::Security.bit(),
        );
        assert!(!filter.allows(Level::Info, Subsystem::Kernel));
        assert!(filter.allows(Level::Warn, Subsystem::Kernel));
        assert!(filter.allows(Level::Error, Subsystem::Security));
        assert!(!filter.allows(Level::Error, Subsystem::Network));
        assert!(Filter::ALL.allows(Level::Trace, Subsystem::Usb));
    }

    #[test]
    fn records_keep_structure_and_monotonic_sequence() {
        let log = Buffer::<4>::new();
        let first = log
            .push(Level::Info, Subsystem::Vfs, 7, 11, 13)
            .expect("first record");
        let second = log
            .push(Level::Warn, Subsystem::Network, 9, 17, 19)
            .expect("second record");

        assert_eq!(first, 0);
        assert_eq!(second, 1);
        assert_eq!(
            log.get(first),
            Some(Record {
                sequence: 0,
                boot_id_hi: 0,
                boot_id_lo: 0,
                monotonic_ticks: 0,
                wall_clock_seconds: 0,
                level: Level::Info,
                subsystem: Subsystem::Vfs,
                event: 7,
                value0: 11,
                value1: 13,
            })
        );
        assert_eq!(
            log.get(second),
            Some(Record {
                sequence: 1,
                boot_id_hi: 0,
                boot_id_lo: 0,
                monotonic_ticks: 0,
                wall_clock_seconds: 0,
                level: Level::Warn,
                subsystem: Subsystem::Network,
                event: 9,
                value0: 17,
                value1: 19,
            })
        );
    }

    #[test]
    fn unpublished_and_out_of_range_records_are_absent() {
        let log = Buffer::<2>::new();
        assert_eq!(log.get(0), None);
        assert_eq!(log.get(2), None);
        assert_eq!(log.get(u64::MAX), None);
    }

    #[test]
    fn full_buffer_fails_closed_without_overwrite() {
        let log = Buffer::<2>::new();
        let first = log
            .push(Level::Debug, Subsystem::Memory, 1, 2, 3)
            .expect("slot 0");
        let second = log
            .push(Level::Error, Subsystem::Security, 4, 5, 6)
            .expect("slot 1");
        let saved_first = log.get(first).expect("first record");
        let saved_second = log.get(second).expect("second record");

        assert_eq!(
            log.push(Level::Trace, Subsystem::Kernel, 8, 9, 10),
            Err(Full)
        );
        assert_eq!(log.reserved(), 2);
        assert_eq!(log.get(first), Some(saved_first));
        assert_eq!(log.get(second), Some(saved_second));
    }

    #[test]
    fn zero_capacity_is_explicitly_full() {
        let log = Buffer::<0>::new();
        assert_eq!(log.capacity(), 0);
        assert_eq!(log.push(Level::Info, Subsystem::Kernel, 1, 0, 0), Err(Full));
    }
}
