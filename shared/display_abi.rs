//! Additive desktop ABI, version 1. Colors are logical 0x00RRGGBB, not GOP bytes.
//! No framebuffer address crosses this boundary. See docs/DESKTOP.md.

pub const VERSION: u32 = 1;
pub const XRGB8888: u32 = 1;
pub const MAX_BLIT_PIXELS: usize = 1024;
pub const KEYBOARD: u32 = 1;
pub const POINTER: u32 = 2;
pub const EVENT_KEY: u32 = 1;
pub const EVENT_POINTER: u32 = 2;
pub const KEY_F1: u32 = 256;
pub const KEY_F2: u32 = 257;
pub const KEY_F3: u32 = 258;
pub const KEY_F4: u32 = 259;
pub const KEY_F11: u32 = 260;
pub const KEY_UP: u32 = 261;
pub const KEY_DOWN: u32 = 262;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisplayInfo {
    pub version: u32,
    pub width: u32,
    pub height: u32,
    pub format: u32,
    pub max_blit_pixels: u32,
    pub capabilities: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InputEvent {
    pub kind: u32,
    /// Key make code or pointer button mask (left=1, right=2, middle=4).
    pub code: u32,
    /// Relative pointer displacement. Positive y points down the screen.
    pub x: i32,
    pub y: i32,
}

const _: () = assert!(core::mem::size_of::<DisplayInfo>() == 24);
const _: () = assert!(core::mem::size_of::<InputEvent>() == 16);
const _: () = assert!(core::mem::offset_of!(InputEvent, y) == 12);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub fn fits(self, width: u32, height: u32) -> bool {
        self.width != 0
            && self.height != 0
            && self
                .x
                .checked_add(self.width)
                .is_some_and(|end| end <= width)
            && self
                .y
                .checked_add(self.height)
                .is_some_and(|end| end <= height)
    }

    pub fn pixels(self) -> Option<usize> {
        (self.width as usize).checked_mul(self.height as usize)
    }

    pub fn contains(self, x: i32, y: i32) -> bool {
        x >= 0
            && y >= 0
            && (x as u32) >= self.x
            && (y as u32) >= self.y
            && self
                .x
                .checked_add(self.width)
                .is_some_and(|end| (x as u32) < end)
            && self
                .y
                .checked_add(self.height)
                .is_some_and(|end| (y as u32) < end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rectangles_reject_empty_overflow_and_offscreen() {
        let r = Rect {
            x: 2,
            y: 3,
            width: 4,
            height: 5,
        };
        assert!(r.fits(6, 8));
        assert!(!r.fits(5, 8));
        assert!(!Rect { width: 0, ..r }.fits(100, 100));
        assert!(!Rect { x: u32::MAX, ..r }.fits(u32::MAX, 100));
        assert!(r.contains(2, 3));
        assert!(!r.contains(6, 3));
        assert!(!r.contains(-1, 3));
    }
}
