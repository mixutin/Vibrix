//! Post-UEFI QEMU/i8042 keyboard input, translated PS/2 scan-code set 1.
//!
//! Original bounded US-ASCII decoder. Unsupported extended keys are ignored;
//! no USB HID, IRQ routing, keyboard-layout switching or port writes are added.
//! Scan-1 make-code reference: Microsoft Keyboard Input Overview, 2026-09-29.

#[cfg(not(test))]
use core::arch::asm;

#[cfg(not(test))]
const I8042_STATUS: u16 = 0x64;
#[cfg(not(test))]
const I8042_DATA: u16 = 0x60;

#[derive(Default)]
pub struct SetOne {
    extended: bool,
    pause_remaining: u8,
    left_shift: bool,
    right_shift: bool,
    left_control: bool,
    right_control: bool,
    caps: bool,
    caps_down: bool,
}

impl SetOne {
    pub const fn new() -> Self {
        Self {
            extended: false,
            pause_remaining: 0,
            left_shift: false,
            right_shift: false,
            left_control: false,
            right_control: false,
            caps: false,
            caps_down: false,
        }
    }

    pub fn feed(&mut self, scan: u8) -> Option<u8> {
        if self.pause_remaining != 0 {
            self.pause_remaining -= 1;
            return None;
        }
        if scan == 0xe1 {
            // Pause's remaining five bytes include apparent Control codes.
            // Consume the entire sequence without changing modifier state.
            self.extended = false;
            self.pause_remaining = 5;
            return None;
        }
        if scan == 0xe0 {
            self.extended = true;
            return None;
        }
        let pressed = scan & 0x80 == 0;
        let code = scan & 0x7f;
        if self.extended {
            self.extended = false;
            if code == 0x1d {
                self.right_control = pressed;
            }
            return None;
        }
        match code {
            0x2a => { self.left_shift = pressed; return None; }
            0x36 => { self.right_shift = pressed; return None; }
            0x1d => { self.left_control = pressed; return None; }
            0x3a => {
                if pressed && !self.caps_down { self.caps = !self.caps; }
                self.caps_down = pressed;
                return None;
            }
            _ => {}
        }
        if !pressed { return None; }
        let byte = match code {
            0x02..=0x0b => b"1234567890"[usize::from(code - 0x02)],
            0x0c => b'-',
            0x0d => b'=',
            0x0e => 8,
            0x0f => b'\t',
            0x10..=0x19 => b"qwertyuiop"[usize::from(code - 0x10)],
            0x1a => b'[',
            0x1b => b']',
            0x1c => b'\n',
            0x1e..=0x26 => b"asdfghjkl"[usize::from(code - 0x1e)],
            0x27 => b';',
            0x28 => b'\'',
            0x29 => b'`',
            0x2b => b'\\',
            0x2c..=0x32 => b"zxcvbnm"[usize::from(code - 0x2c)],
            0x33 => b',',
            0x34 => b'.',
            0x35 => b'/',
            0x39 => b' ',
            _ => return None,
        };
        if byte.is_ascii_lowercase() {
            if self.left_control || self.right_control {
                return Some(byte - b'a' + 1);
            }
            return Some(if (self.left_shift || self.right_shift) ^ self.caps {
                byte.to_ascii_uppercase()
            } else { byte });
        }
        if self.left_shift || self.right_shift {
            const PLAIN: &[u8] = b"1234567890-=[];'`\\,./";
            const SHIFT: &[u8] = b"!@#$%^&*()_+{}:\"~|<>?";
            if let Some(index) = PLAIN.iter().position(|&plain| plain == byte) {
                return Some(SHIFT[index]);
            }
        }
        Some(byte)
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
    // Ignore auxiliary mouse bytes, indicated by bit 5; consume them rather
    // than leaving the FIFO stuck. No second consumer is introduced.
    let byte: u8;
    unsafe {
        asm!(
            "in al, dx",
            in("dx") I8042_DATA,
            out("al") byte,
            options(nomem, nostack, preserves_flags)
        );
    }
    if status & (1 << 5) != 0 { None } else { Some(byte) }
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
        assert_eq!(keys.feed(0x1c), None);
        assert_eq!(keys.feed(0x9e), None);
        assert_eq!(keys.feed(0x01), None);
        assert_eq!(keys.feed(0x1e), Some(b'a'));
        assert_eq!(keys.feed(0x35), Some(b'/'));
        assert_eq!(keys.feed(0x0c), Some(b'-'));
        assert_eq!(keys.feed(0x39), Some(b' '));
        assert_eq!(keys.feed(0x0e), Some(8));
    }

    #[test]
    fn punctuation_quotes_and_redirection_are_typeable() {
        let mut keys = SetOne::new();
        for (scan, plain, shifted) in [(0x28, b'\'', b'"'), (0x34, b'.', b'>'), (0x33, b',', b'<'), (0x2b, b'\\', b'|'), (0x0d, b'=', b'+')] {
            assert_eq!(keys.feed(scan), Some(plain));
            assert_eq!(keys.feed(0x2a), None);
            assert_eq!(keys.feed(scan), Some(shifted));
            assert_eq!(keys.feed(0xaa), None);
        }
    }

    #[test]
    fn independent_shift_releases_and_caps_repeats_are_balanced() {
        let mut keys = SetOne::new();
        keys.feed(0x2a);
        keys.feed(0x36);
        keys.feed(0xaa);
        assert_eq!(keys.feed(0x1e), Some(b'A'));
        keys.feed(0xb6);
        assert_eq!(keys.feed(0x1e), Some(b'a'));
        keys.feed(0x3a);
        keys.feed(0x3a);
        keys.feed(0xba);
        assert_eq!(keys.feed(0x1e), Some(b'A'));
        keys.feed(0x2a);
        assert_eq!(keys.feed(0x1e), Some(b'a'));
        keys.feed(0xaa);
        keys.feed(0x3a);
        keys.feed(0xba);
        assert_eq!(keys.feed(0x1e), Some(b'a'));
    }

    #[test]
    fn_control_u_and_pause_do_not_leave_stuck_modifiers() {
        let mut keys = SetOne::new();
        keys.feed(0x1d);
        assert_eq!(keys.feed(0x16), Some(0x15));
        keys.feed(0x9d);
        for scan in [0xe1, 0x1d, 0x45, 0xe1, 0x9d, 0xc5] { assert_eq!(keys.feed(scan), None); }
        assert_eq!(keys.feed(0x16), Some(b'u'));
        keys.feed(0xe0);
        keys.feed(0x1d);
        assert_eq!(keys.feed(0x16), Some(0x15));
        keys.feed(0xe0);
        keys.feed(0x9d);
        assert_eq!(keys.feed(0x16), Some(b'u'));
    }
}
