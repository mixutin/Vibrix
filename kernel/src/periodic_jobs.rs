//! Fixed-capacity periodic job scheduler for M21 administration.
//!
//! Time is supplied as a monotonic tick by the caller. The scheduler owns only
//! job cadence/state; command execution and persistent configuration are
//! separate layers.

pub const MAX_PERIODIC_JOBS: usize = 16;
pub const JOB_NAME_BYTES: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    InvalidName,
    InvalidInterval,
    Capacity,
    Duplicate,
    NotFound,
    ClockWentBackwards,
    DeadlineOverflow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobName {
    bytes: [u8; JOB_NAME_BYTES],
    len: u8,
}

impl JobName {
    pub const EMPTY: Self = Self {
        bytes: [0; JOB_NAME_BYTES],
        len: 0,
    };

    pub fn new(name: &[u8]) -> Result<Self, Error> {
        if name.is_empty()
            || name.len() > JOB_NAME_BYTES
            || !name.iter().copied().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'-' | b'_' | b'.')
            })
        {
            return Err(Error::InvalidName);
        }
        let mut result = Self::EMPTY;
        result.bytes[..name.len()].copy_from_slice(name);
        result.len = name.len() as u8;
        Ok(result)
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Job {
    name: JobName,
    interval: u64,
    next_due: u64,
}

pub struct Scheduler {
    jobs: [Option<Job>; MAX_PERIODIC_JOBS],
    now: u64,
}

impl Scheduler {
    pub const fn new() -> Self {
        Self {
            jobs: [None; MAX_PERIODIC_JOBS],
            now: 0,
        }
    }

    pub fn add(&mut self, name: JobName, interval: u64) -> Result<(), Error> {
        if interval == 0 {
            return Err(Error::InvalidInterval);
        }
        if self.jobs.iter().flatten().any(|job| job.name == name) {
            return Err(Error::Duplicate);
        }
        let slot = self
            .jobs
            .iter()
            .position(Option::is_none)
            .ok_or(Error::Capacity)?;
        let next_due = self
            .now
            .checked_add(interval)
            .ok_or(Error::DeadlineOverflow)?;
        self.jobs[slot] = Some(Job {
            name,
            interval,
            next_due,
        });
        Ok(())
    }

    pub fn remove(&mut self, name: JobName) -> Result<(), Error> {
        let slot = self
            .jobs
            .iter()
            .position(|job| job.is_some_and(|job| job.name == name))
            .ok_or(Error::NotFound)?;
        self.jobs[slot] = None;
        Ok(())
    }

    pub fn advance(
        &mut self,
        now: u64,
        due: &mut [JobName; MAX_PERIODIC_JOBS],
    ) -> Result<usize, Error> {
        if now < self.now {
            return Err(Error::ClockWentBackwards);
        }

        // Calculate every new deadline before publishing any state so an
        // overflow leaves the scheduler unchanged.
        let mut next = self.jobs;
        let mut count = 0usize;
        for job in next.iter_mut().flatten() {
            if now < job.next_due {
                continue;
            }
            due[count] = job.name;
            count += 1;

            let elapsed = now - job.next_due;
            let intervals = elapsed / job.interval + 1;
            let delta = job
                .interval
                .checked_mul(intervals)
                .ok_or(Error::DeadlineOverflow)?;
            job.next_due = job
                .next_due
                .checked_add(delta)
                .ok_or(Error::DeadlineOverflow)?;
        }

        self.jobs = next;
        self.now = now;
        Ok(count)
    }

    pub const fn now(&self) -> u64 {
        self.now
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

pub fn self_test() -> Result<(), Error> {
    let cleanup = JobName::new(b"cleanup")?;
    let rotate = JobName::new(b"rotate")?;
    let mut scheduler = Scheduler::new();
    scheduler.add(cleanup, 5)?;
    scheduler.add(rotate, 10)?;
    let mut due = [JobName::EMPTY; MAX_PERIODIC_JOBS];
    if scheduler.advance(4, &mut due)? != 0 {
        return Err(Error::InvalidInterval);
    }
    let count = scheduler.advance(10, &mut due)?;
    if count != 2 || due[..count] != [cleanup, rotate] {
        return Err(Error::InvalidInterval);
    }
    let count = scheduler.advance(16, &mut due)?;
    if count != 1 || due[0] != cleanup {
        return Err(Error::InvalidInterval);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(value: &[u8]) -> JobName {
        JobName::new(value).unwrap()
    }

    #[test]
    fn jobs_fire_in_registration_order_and_catch_up_without_bursting() {
        let mut scheduler = Scheduler::new();
        scheduler.add(n(b"fast"), 5).unwrap();
        scheduler.add(n(b"slow"), 10).unwrap();
        let mut due = [JobName::EMPTY; MAX_PERIODIC_JOBS];

        assert_eq!(scheduler.advance(10, &mut due).unwrap(), 2);
        assert_eq!(&due[..2], &[n(b"fast"), n(b"slow")]);

        // Missing several periods yields one due notification and advances the
        // next deadline beyond the supplied time.
        assert_eq!(scheduler.advance(31, &mut due).unwrap(), 2);
        assert_eq!(scheduler.advance(31, &mut due).unwrap(), 0);
    }

    #[test]
    fn invalid_mutations_fail_without_corrupting_schedule() {
        let mut scheduler = Scheduler::new();
        let job = n(b"job");
        scheduler.add(job, 7).unwrap();
        assert_eq!(scheduler.add(job, 7), Err(Error::Duplicate));
        assert_eq!(scheduler.add(n(b"zero"), 0), Err(Error::InvalidInterval));
        assert_eq!(scheduler.remove(n(b"missing")), Err(Error::NotFound));
        assert_eq!(scheduler.now(), 0);

        let mut due = [JobName::EMPTY; MAX_PERIODIC_JOBS];
        scheduler.advance(6, &mut due).unwrap();
        assert_eq!(
            scheduler.advance(5, &mut due),
            Err(Error::ClockWentBackwards)
        );
        assert_eq!(scheduler.now(), 6);
    }

    #[test]
    fn removal_prevents_future_delivery() {
        let mut scheduler = Scheduler::new();
        let job = n(b"once");
        scheduler.add(job, 1).unwrap();
        scheduler.remove(job).unwrap();
        let mut due = [JobName::EMPTY; MAX_PERIODIC_JOBS];
        assert_eq!(scheduler.advance(100, &mut due).unwrap(), 0);
    }

    #[test]
    fn production_self_test_passes() {
        self_test().unwrap();
    }
}
