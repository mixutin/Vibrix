//! Boot-local identity and timestamp metadata for structured records.

use core::sync::atomic::{AtomicU8, AtomicU64, Ordering};

pub const PIT_HZ: u64 = 100;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    AlreadyInitialized,
    ZeroBootId,
    InvalidDate,
    BeforeUnixEpoch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Stamp {
    pub boot_id_hi: u64,
    pub boot_id_lo: u64,
    pub monotonic_ticks: u64,
    pub wall_clock_seconds: u64,
}

// 0 = uninitialized, 1 = initializing, 2 = published.
static STATE: AtomicU8 = AtomicU8::new(0);
static BOOT_ID_HI: AtomicU64 = AtomicU64::new(0);
static BOOT_ID_LO: AtomicU64 = AtomicU64::new(0);
static WALL_CLOCK_ANCHOR: AtomicU64 = AtomicU64::new(0);
static MONOTONIC_TICKS: AtomicU64 = AtomicU64::new(0);

pub fn initialize(boot_id: [u8; 16], wall_clock_seconds: u64) -> Result<(), Error> {
    let hi = u64::from_be_bytes(boot_id[..8].try_into().expect("fixed boot-id half"));
    let lo = u64::from_be_bytes(boot_id[8..].try_into().expect("fixed boot-id half"));
    if hi == 0 && lo == 0 {
        return Err(Error::ZeroBootId);
    }
    if STATE
        .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err(Error::AlreadyInitialized);
    }
    BOOT_ID_HI.store(hi, Ordering::Relaxed);
    BOOT_ID_LO.store(lo, Ordering::Relaxed);
    WALL_CLOCK_ANCHOR.store(wall_clock_seconds, Ordering::Relaxed);
    STATE.store(2, Ordering::Release);
    Ok(())
}

pub fn initialized() -> bool {
    STATE.load(Ordering::Acquire) == 2
}

pub fn record_tick() {
    MONOTONIC_TICKS.fetch_add(1, Ordering::Relaxed);
}

pub fn monotonic_ticks() -> u64 {
    MONOTONIC_TICKS.load(Ordering::Relaxed)
}

pub fn stamp() -> Stamp {
    let ticks = monotonic_ticks();
    if !initialized() {
        return Stamp {
            boot_id_hi: 0,
            boot_id_lo: 0,
            monotonic_ticks: ticks,
            wall_clock_seconds: 0,
        };
    }
    Stamp {
        boot_id_hi: BOOT_ID_HI.load(Ordering::Relaxed),
        boot_id_lo: BOOT_ID_LO.load(Ordering::Relaxed),
        monotonic_ticks: ticks,
        wall_clock_seconds: WALL_CLOCK_ANCHOR
            .load(Ordering::Relaxed)
            .saturating_add(ticks / PIT_HZ),
    }
}

const fn leap_year(year: u16) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

const fn days_in_month(year: u16, month: u8) -> Option<u8> {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => Some(31),
        4 | 6 | 9 | 11 => Some(30),
        2 => Some(if leap_year(year) { 29 } else { 28 }),
        _ => None,
    }
}

/// Convert one validated UTC civil timestamp to Unix seconds.
pub fn unix_seconds(
    year: u16,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
) -> Result<u64, Error> {
    if year < 1970 || hour > 23 || minute > 59 || second > 59 {
        return Err(if year < 1970 {
            Error::BeforeUnixEpoch
        } else {
            Error::InvalidDate
        });
    }
    let max_day = days_in_month(year, month).ok_or(Error::InvalidDate)?;
    if day == 0 || day > max_day {
        return Err(Error::InvalidDate);
    }

    let mut days = 0u64;
    let mut current = 1970u16;
    while current < year {
        days += if leap_year(current) { 366 } else { 365 };
        current += 1;
    }
    let mut current_month = 1u8;
    while current_month < month {
        days += u64::from(days_in_month(year, current_month).ok_or(Error::InvalidDate)?);
        current_month += 1;
    }
    days += u64::from(day - 1);

    Ok(days
        .saturating_mul(86_400)
        .saturating_add(u64::from(hour) * 3_600)
        .saturating_add(u64::from(minute) * 60)
        .saturating_add(u64::from(second)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_epoch_and_known_leap_dates_convert_exactly() {
        assert_eq!(unix_seconds(1970, 1, 1, 0, 0, 0), Ok(0));
        assert_eq!(unix_seconds(2000, 1, 1, 0, 0, 0), Ok(946_684_800));
        assert_eq!(unix_seconds(2024, 2, 29, 12, 34, 56), Ok(1_709_210_096));
    }

    #[test]
    fn invalid_dates_fail_closed() {
        assert_eq!(
            unix_seconds(1969, 12, 31, 23, 59, 59),
            Err(Error::BeforeUnixEpoch)
        );
        assert_eq!(unix_seconds(2026, 2, 29, 0, 0, 0), Err(Error::InvalidDate));
        assert_eq!(unix_seconds(2026, 13, 1, 0, 0, 0), Err(Error::InvalidDate));
        assert_eq!(unix_seconds(2026, 1, 1, 24, 0, 0), Err(Error::InvalidDate));
    }
}
