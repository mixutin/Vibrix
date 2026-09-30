//! Fail-closed audit log for security-sensitive process operations.
//!
//! Callers reserve an immutable slot before mutating authority. If the audit
//! log is full, reservation fails and the protected operation must not occur.

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

pub const AUDIT_CAPACITY: usize = 256;

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    SetResUid = 1,
    SetResGid = 2,
    EnableNoNewPrivileges = 3,
    RestrictPromises = 4,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Outcome {
    Allowed = 1,
    Denied = 2,
    Failed = 3,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Record {
    pub sequence: u64,
    pub subject_pid: u32,
    pub action: Action,
    pub outcome: Outcome,
    pub before: u64,
    pub requested_or_after: u64,
}

impl Record {
    const EMPTY: Self = Self {
        sequence: 0,
        subject_pid: 0,
        action: Action::SetResUid,
        outcome: Outcome::Failed,
        before: 0,
        requested_or_after: 0,
    };
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

// SAFETY: each successfully reserved slot has exactly one writer. Publication
// is Release/Acquire and committed records are never mutated.
unsafe impl Sync for Slot {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Full;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Reservation {
    index: usize,
}

pub struct Log<const N: usize> {
    next: AtomicUsize,
    slots: [Slot; N],
}

impl<const N: usize> Log<N> {
    pub const fn new() -> Self {
        Self {
            next: AtomicUsize::new(0),
            slots: [const { Slot::new() }; N],
        }
    }

    pub fn reserve(&self) -> Result<Reservation, Full> {
        let index = self
            .next
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                (current < N).then_some(current + 1)
            })
            .map_err(|_| Full)?;
        Ok(Reservation { index })
    }

    pub fn commit(
        &self,
        reservation: Reservation,
        subject_pid: u32,
        action: Action,
        outcome: Outcome,
        before: u64,
        requested_or_after: u64,
    ) -> u64 {
        let record = Record {
            sequence: reservation.index as u64,
            subject_pid,
            action,
            outcome,
            before,
            requested_or_after,
        };
        let slot = &self.slots[reservation.index];
        // SAFETY: Reservation owns this unique unpublished slot.
        unsafe { slot.record.get().write(record) };
        slot.published.store(true, Ordering::Release);
        record.sequence
    }

    pub fn get(&self, sequence: u64) -> Option<Record> {
        let index = usize::try_from(sequence).ok()?;
        let slot = self.slots.get(index)?;
        if !slot.published.load(Ordering::Acquire) {
            return None;
        }
        // SAFETY: Acquire observes the unique writer before immutable publish.
        Some(unsafe { slot.record.get().read() })
    }

    pub fn reserved(&self) -> usize {
        self.next.load(Ordering::Acquire).min(N)
    }
}

impl<const N: usize> Default for Log<N> {
    fn default() -> Self {
        Self::new()
    }
}

pub static SECURITY_AUDIT: Log<AUDIT_CAPACITY> = Log::new();

pub fn reserve() -> Result<Reservation, Full> {
    SECURITY_AUDIT.reserve()
}

pub fn commit(
    reservation: Reservation,
    subject_pid: u32,
    action: Action,
    outcome: Outcome,
    before: u64,
    requested_or_after: u64,
) -> u64 {
    SECURITY_AUDIT.commit(
        reservation,
        subject_pid,
        action,
        outcome,
        before,
        requested_or_after,
    )
}

pub fn self_test() -> Result<(), &'static str> {
    let log = Log::<2>::new();
    let first = log.reserve().map_err(|_| "audit reservation failed")?;
    let sequence = log.commit(
        first,
        7,
        Action::RestrictPromises,
        Outcome::Allowed,
        0xf,
        0x3,
    );
    if log.get(sequence)
        != Some(Record {
            sequence: 0,
            subject_pid: 7,
            action: Action::RestrictPromises,
            outcome: Outcome::Allowed,
            before: 0xf,
            requested_or_after: 0x3,
        })
    {
        return Err("audit record publication mismatch");
    }
    let held = log
        .reserve()
        .map_err(|_| "second audit reservation failed")?;
    if log.reserve() != Err(Full) || log.get(1).is_some() {
        return Err("audit capacity/reservation policy failed");
    }
    log.commit(
        held,
        7,
        Action::EnableNoNewPrivileges,
        Outcome::Denied,
        0,
        1,
    );
    if log.get(1).is_none() {
        return Err("reserved audit slot did not publish");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_policy_self_test_passes() {
        self_test().unwrap();
    }

    #[test]
    fn capacity_is_reserved_before_publication_and_never_overwritten() {
        let log = Log::<1>::new();
        let reservation = log.reserve().unwrap();
        assert_eq!(log.reserved(), 1);
        assert_eq!(log.get(0), None);
        assert_eq!(log.reserve(), Err(Full));
        let sequence = log.commit(reservation, 1, Action::SetResUid, Outcome::Allowed, 0, 1000);
        let saved = log.get(sequence).unwrap();
        assert_eq!(log.reserve(), Err(Full));
        assert_eq!(log.get(sequence), Some(saved));
    }
}
