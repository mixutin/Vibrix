//! Allocation-free synchronization primitives for SMP groundwork.

use core::{
    cell::UnsafeCell,
    hint::spin_loop,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    TicketExhausted,
    InvalidParticipants,
    BarrierOverrun,
}

pub struct TicketLock<T> {
    next: AtomicU64,
    owner: AtomicU64,
    value: UnsafeCell<T>,
}

// SAFETY: access to T is serialized by unique served tickets.
unsafe impl<T: Send> Sync for TicketLock<T> {}
unsafe impl<T: Send> Send for TicketLock<T> {}

impl<T> TicketLock<T> {
    pub const fn new(value: T) -> Self {
        Self {
            next: AtomicU64::new(0),
            owner: AtomicU64::new(0),
            value: UnsafeCell::new(value),
        }
    }

    pub fn lock(&self) -> Result<TicketGuard<'_, T>, Error> {
        let ticket = self
            .next
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                (next != u64::MAX).then_some(next + 1)
            })
            .map_err(|_| Error::TicketExhausted)?;

        while self.owner.load(Ordering::Acquire) != ticket {
            spin_loop();
        }
        Ok(TicketGuard { lock: self })
    }

    pub fn is_locked(&self) -> bool {
        self.owner.load(Ordering::Acquire) != self.next.load(Ordering::Acquire)
    }
}

pub struct TicketGuard<'a, T> {
    lock: &'a TicketLock<T>,
}

impl<T> Deref for TicketGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        // SAFETY: this guard owns the currently served ticket.
        unsafe { &*self.lock.value.get() }
    }
}

impl<T> DerefMut for TicketGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: this guard uniquely owns the currently served ticket.
        unsafe { &mut *self.lock.value.get() }
    }
}

impl<T> Drop for TicketGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.owner.fetch_add(1, Ordering::Release);
    }
}

pub struct BootBarrier {
    participants: usize,
    arrived: AtomicUsize,
    released: AtomicBool,
}

impl BootBarrier {
    pub const fn new(participants: usize) -> Result<Self, Error> {
        if participants == 0 {
            return Err(Error::InvalidParticipants);
        }
        Ok(Self {
            participants,
            arrived: AtomicUsize::new(0),
            released: AtomicBool::new(false),
        })
    }

    pub fn wait(&self) -> Result<(), Error> {
        if self.released.load(Ordering::Acquire) {
            return Err(Error::BarrierOverrun);
        }
        let arrival = self
            .arrived
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                (current < self.participants).then_some(current + 1)
            })
            .map_err(|_| Error::BarrierOverrun)?
            + 1;

        if arrival == self.participants {
            self.released.store(true, Ordering::Release);
            return Ok(());
        }
        while !self.released.load(Ordering::Acquire) {
            spin_loop();
        }
        Ok(())
    }

    pub fn released(&self) -> bool {
        self.released.load(Ordering::Acquire)
    }
}

pub fn self_test() -> Result<(), Error> {
    let lock = TicketLock::new(41u64);
    {
        let mut value = lock.lock()?;
        if !lock.is_locked() {
            return Err(Error::TicketExhausted);
        }
        *value += 1;
    }
    if lock.is_locked() || *lock.lock()? != 42 {
        return Err(Error::TicketExhausted);
    }
    let barrier = BootBarrier::new(1)?;
    barrier.wait()?;
    if !barrier.released() || barrier.wait() != Err(Error::BarrierOverrun) {
        return Err(Error::BarrierOverrun);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use std::{sync::Arc, thread};

    #[test]
    fn ticket_lock_serializes_real_host_threads() {
        let lock = Arc::new(TicketLock::new(0u64));
        let mut joins = std::vec::Vec::new();
        for _ in 0..8 {
            let lock = Arc::clone(&lock);
            joins.push(thread::spawn(move || {
                for _ in 0..2_000 {
                    *lock.lock().unwrap() += 1;
                }
            }));
        }
        for join in joins {
            join.join().unwrap();
        }
        assert_eq!(*lock.lock().unwrap(), 16_000);
    }

    #[test]
    fn barrier_releases_all_declared_participants_once() {
        let barrier = Arc::new(BootBarrier::new(8).unwrap());
        let mut joins = std::vec::Vec::new();
        for _ in 0..8 {
            let barrier = Arc::clone(&barrier);
            joins.push(thread::spawn(move || barrier.wait().unwrap()));
        }
        for join in joins {
            join.join().unwrap();
        }
        assert!(barrier.released());
        assert_eq!(barrier.wait(), Err(Error::BarrierOverrun));
    }

    #[test]
    fn zero_participant_barrier_is_invalid() {
        assert!(matches!(
            BootBarrier::new(0),
            Err(Error::InvalidParticipants)
        ));
    }

    #[test]
    fn production_self_test_passes() {
        self_test().unwrap();
    }
}
