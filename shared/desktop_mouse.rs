//! Standard three-byte PS/2 mouse packets; device setup selects type zero.
//! Interface reference: Microsoft PS/2 packet table; no driver code copied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Report {
    pub dx: i32,
    pub dy: i32,
    pub buttons: u32,
}

pub struct Mouse {
    bytes: [u8; 3],
    used: usize,
}
impl Default for Mouse {
    fn default() -> Self {
        Self::new()
    }
}
impl Mouse {
    pub const fn new() -> Self {
        Self {
            bytes: [0; 3],
            used: 0,
        }
    }
    pub fn reset(&mut self) {
        self.used = 0;
    }
    pub fn feed(&mut self, byte: u8) -> Option<Report> {
        if self.used == 0 && byte & 8 == 0 {
            return None;
        }
        self.bytes[self.used] = byte;
        self.used += 1;
        if self.used != 3 {
            return None;
        }
        self.used = 0;
        let status = self.bytes[0];
        // Overflow invalidates motion, but release/button state is retained.
        let dx = if status & 0x40 != 0 {
            0
        } else {
            i32::from(self.bytes[1]) - if status & 0x10 != 0 { 256 } else { 0 }
        };
        let dy = if status & 0x80 != 0 {
            0
        } else {
            i32::from(self.bytes[2]) - if status & 0x20 != 0 { 256 } else { 0 }
        };
        Some(Report {
            dx,
            dy: -dy,
            buttons: u32::from(status & 7),
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn packet(mouse: &mut Mouse, bytes: [u8; 3]) -> Report {
        assert_eq!(mouse.feed(bytes[0]), None);
        assert_eq!(mouse.feed(bytes[1]), None);
        mouse.feed(bytes[2]).unwrap()
    }
    #[test]
    fn signed_nine_bit_movement_and_buttons() {
        let mut mouse = Mouse::new();
        assert_eq!(
            packet(&mut mouse, [0x29, 20, 246]),
            Report {
                dx: 20,
                dy: 10,
                buttons: 1
            }
        );
        assert_eq!(
            packet(&mut mouse, [0x18, 0, 255]),
            Report {
                dx: -256,
                dy: -255,
                buttons: 0
            }
        );
        assert_eq!(
            packet(&mut mouse, [8, 200, 0]),
            Report {
                dx: 200,
                dy: 0,
                buttons: 0
            }
        );
    }
    #[test]
    fn incomplete_noise_overflow_and_reset_are_bounded() {
        let mut mouse = Mouse::new();
        for byte in [0, 1, 2, 4, 0x80] {
            assert_eq!(mouse.feed(byte), None);
        }
        assert_eq!(
            packet(&mut mouse, [0xc8, 255, 255]),
            Report {
                dx: 0,
                dy: 0,
                buttons: 0
            }
        );
        mouse.feed(8);
        mouse.reset();
        assert_eq!(packet(&mut mouse, [0x0a, 0, 0]).buttons, 2);
    }
}
