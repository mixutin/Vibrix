//! Minimal CMOS RTC reader for a UTC wall-clock boot anchor.

use core::arch::asm;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Unstable,
    InvalidBcd,
    Date(crate::boot_clock::Error),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Snapshot {
    second: u8,
    minute: u8,
    hour: u8,
    day: u8,
    month: u8,
    year: u8,
    status_b: u8,
}

unsafe fn outb(port: u16, value: u8) {
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags));
    }
}

unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    unsafe {
        asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack, preserves_flags));
    }
    value
}

unsafe fn register(index: u8) -> u8 {
    unsafe {
        outb(0x70, index | 0x80);
        inb(0x71)
    }
}

unsafe fn update_in_progress() -> bool {
    unsafe { register(0x0a) & 0x80 != 0 }
}

unsafe fn snapshot() -> Snapshot {
    unsafe {
        Snapshot {
            second: register(0x00),
            minute: register(0x02),
            hour: register(0x04),
            day: register(0x07),
            month: register(0x08),
            year: register(0x09),
            status_b: register(0x0b),
        }
    }
}

fn bcd(value: u8) -> Result<u8, Error> {
    let high = value >> 4;
    let low = value & 0x0f;
    if high > 9 || low > 9 {
        return Err(Error::InvalidBcd);
    }
    Ok(high * 10 + low)
}

fn decode(raw: Snapshot) -> Result<(u16, u8, u8, u8, u8, u8), Error> {
    let binary = raw.status_b & 0x04 != 0;
    let mode_24h = raw.status_b & 0x02 != 0;
    let pm = raw.hour & 0x80 != 0;
    let hour_raw = raw.hour & 0x7f;

    let second = if binary { raw.second } else { bcd(raw.second)? };
    let minute = if binary { raw.minute } else { bcd(raw.minute)? };
    let mut hour = if binary { hour_raw } else { bcd(hour_raw)? };
    let day = if binary { raw.day } else { bcd(raw.day)? };
    let month = if binary { raw.month } else { bcd(raw.month)? };
    let year2 = if binary { raw.year } else { bcd(raw.year)? };

    if !mode_24h {
        if hour == 0 || hour > 12 {
            return Err(Error::InvalidBcd);
        }
        if pm {
            if hour != 12 {
                hour += 12;
            }
        } else if hour == 12 {
            hour = 0;
        }
    }

    // Current x86 platform policy supports RTC years 2000..2099. Extending
    // beyond that needs a validated firmware century register/FADT contract.
    let year = 2000u16 + u16::from(year2);
    Ok((year, month, day, hour, minute, second))
}

/// Read a stable CMOS RTC snapshot and return Unix UTC seconds.
///
/// # Safety
/// Caller runs on the sole BSP at CPL0 before concurrent CMOS users exist.
pub unsafe fn read_unix_seconds() -> Result<u64, Error> {
    for _ in 0..8 {
        while unsafe { update_in_progress() } {
            core::hint::spin_loop();
        }
        let first = unsafe { snapshot() };
        while unsafe { update_in_progress() } {
            core::hint::spin_loop();
        }
        let second = unsafe { snapshot() };
        // Re-enable NMI after the last CMOS index transaction.
        unsafe { outb(0x70, 0) };
        if first != second {
            continue;
        }
        let (year, month, day, hour, minute, second) = decode(first)?;
        return crate::boot_clock::unix_seconds(year, month, day, hour, minute, second)
            .map_err(Error::Date);
    }
    Err(Error::Unstable)
}
