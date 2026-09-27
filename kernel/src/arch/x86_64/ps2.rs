//! Minimal post-UEFI QEMU/i8042 keyboard input, PS/2 scan-code set 1.
//!
//! Only bounded ASCII make codes are accepted; releases, unsupported keys
//! and extended codes are ignored. This is not USB HID, an IRQ handler,
//! a TTY or the eventual userspace console. No port writes are performed.

#[cfg(not(test))]
use core::arch::asm;

#[cfg(not(test))]
const I8042_STATUS: u16 = 0x64;
#[cfg(not(test))]
const I8042_DATA: u16 = 0x60;

/// Decode only the unshifted, translated set-one make codes needed for
/// initial QEMU keyboard proof; unsupported input is not guessed as text.
#[derive(Default)]
pub struct SetOne {
    extended: bool,
}

impl SetOne {
    pub const fn new() -> Self {
        Self { extended: false }
    }

    pub fn feed(&mut self, scan: u8) -> Option<u8> {
        if scan == 0xe0 || scan == 0xe1 {
            self.extended = true;
            return None;
        }
        if self.extended {
            self.extended = false;
            return None;
        }
        if scan & 0x80 != 0 {
            return None; // key release, never type twice
        }
        Some(match scan {
            0x02 => b'1',
            0x03 => b'2',
            0x04 => b'3',
            0x05 => b'4',
            0x06 => b'5',
            0x07 => b'6',
            0x08 => b'7',
            0x09 => b'8',
            0x0a => b'9',
            0x0b => b'0',
            0x0e => 8, // backspace
            0x10 => b'q',
            0x11 => b'w',
            0x12 => b'e',
            0x13 => b'r',
            0x14 => b't',
            0x15 => b'y',
            0x16 => b'u',
            0x17 => b'i',
            0x18 => b'o',
            0x19 => b'p',
            0x1c => b'\n',
            0x1e => b'a',
            0x1f => b's',
            0x20 => b'd',
            0x21 => b'f',
            0x22 => b'g',
            0x23 => b'h',
            0x24 => b'j',
            0x25 => b'k',
            0x26 => b'l',
            0x2c => b'z',
            0x2d => b'x',
            0x2e => b'c',
            0x2f => b'v',
            0x30 => b'b',
            0x31 => b'n',
            0x32 => b'm',
            0x39 => b' ',
            _ => return None,
        })
    }
}

/// Poll once without relying on firmware services, IRQs or a busy wait
/// inside the primitive. An absent/empty i8042 reports None.
/// 
/// # Safety
/// The caller must be ring zero on x86-64 with permission for ports
/// 0x64/0x60 and sole ownership of the legacy keyboard-data consumer.
/// One boot CPU with IF=0 satisfies the current QEMU development invariant.
/// The OS cannot assume a PS/2 keyboard exists on Target 001.
#[cfg(not(test))]
pub unsafe fn poll_scancode() -> Option<u8> {
    let status: u8;
    // SAFETY: ring-zero read of emulated i8042 status; no port write.
    unsafe {
        asm!(
            "in al, dx",
            in("dx") I8042_STATUS,
            out("al") status,
            options(nomem, nostack, preserves_flags)
        );
    }
    if status & 1 == 0 {
        return None;
    }
    // Status bit zero means output buffer full. Ignore auxiliary mouse bytes,
    // indicated by bit 5; consume them rather than leaving the FIFO stuck.
    let byte: u8;
    unsafe {
        asm!(
            "in al, dx",
            in("dx") I8042_DATA,
            out("al") byte,
            options(nomem, nostack, preserves_flags)
        );
    }
    if status & (1 << 5) != 0 {
        None
    } else {
        Some(byte)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qemu_h_and_enter_are_one_ascii_event_each() {
        let mut keys = SetOne::new();
        assert_eq!(keys.feed(0x23), Some(b'h'));
        assert_eq!(keys.feed(0xa3), None);
        assert_eq!(keys.feed(0x1c), Some(b'\n'));
        assert_eq!(keys.feed(0x9c), None);
    }

    #[test]
    fn extended_release_unknown_and_non_ascii_are_not_invented() {
        let mut keys = SetOne::new();
        assert_eq!(keys.feed(0xe0), None);
        assert_eq!(keys.feed(0x1c), None); // extended keypad enter
        assert_eq!(keys.feed(0x80 | 0x1e), None);
        assert_eq!(keys.feed(0x01), None); // escape unsupported
        assert_eq!(keys.feed(0x1e), Some(b'a'));
        assert_eq!(keys.feed(0x39), Some(b' '));
        assert_eq!(keys.feed(0x0e), Some(8));
    }
}
