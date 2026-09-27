//! Framebuffer console for the Vibrix kernel.
//!
//! Writes text to the GOP framebuffer discovered by the UEFI loader. Supports
//! 32-bit BGRA and RGBA pixel formats. No allocation, no dynamic state — just
//! raw pixel writes to the framebuffer physical address.

use crate::font;

/// GOP pixel format: Blue, Green, Red, Reserved (most common on x86-64 UEFI).
const PIXEL_FORMAT_BGRA: u32 = 1;
/// GOP pixel format: Red, Green, Blue, Reserved.
const PIXEL_FORMAT_RGBA: u32 = 0;

/// Framebuffer console state.
pub struct FramebufferConsole {
    base: *mut u32,
    width: u32,
    height: u32,
    stride: u32,
    format: u32,
    cursor_x: u32,
    cursor_y: u32,
}

impl FramebufferConsole {
    /// Create a new console from BootInfo framebuffer parameters.
    ///
    /// Returns `None` if the framebuffer parameters are invalid or unsupported.
    pub fn new(
        base: u64,
        width: u32,
        height: u32,
        stride: u32,
        format: u32,
    ) -> Option<Self> {
        if base == 0 || width == 0 || height == 0 || stride < width {
            return None;
        }
        if format != PIXEL_FORMAT_BGRA && format != PIXEL_FORMAT_RGBA {
            return None;
        }
        // Ensure the framebuffer is within a reasonable physical range.
        if base.checked_add(u64::from(stride) * u64::from(height) * 4).is_none() {
            return None;
        }
        Some(Self {
            base: base as *mut u32,
            width,
            height,
            stride,
            format,
            cursor_x: 0,
            cursor_y: 0,
        })
    }

    /// Write a string to the console, advancing the cursor.
    pub fn write_str(&mut self, text: &str) {
        for byte in text.bytes() {
            match byte {
                b'\n' => self.newline(),
                b'\r' => self.cursor_x = 0,
                _ => self.write_char(byte),
            }
        }
    }

    /// Clear the entire framebuffer to black.
    pub fn clear(&mut self) {
        for row in 0..self.height {
            for col in 0..self.width {
                self.set_pixel(col, row, 0x000000);
            }
        }
        self.cursor_x = 0;
        self.cursor_y = 0;
    }

    fn newline(&mut self) {
        self.cursor_x = 0;
        self.cursor_y += 1;
        if self.cursor_y >= self.height / font::FONT_HEIGHT as u32 {
            self.cursor_y = 0; // wrap to top (simple console)
        }
    }

    fn write_char(&mut self, byte: u8) {
        let glyph = font::glyph(byte);
        let char_width = font::FONT_WIDTH as u32;
        let char_height = font::FONT_HEIGHT as u32;

        // Wrap if needed
        if self.cursor_x + char_width > self.width {
            self.newline();
        }

        let x = self.cursor_x;
        let y = self.cursor_y * char_height;

        for row in 0..char_height {
            let glyph_row = glyph[row as usize];
            for col in 0..char_width {
                if glyph_row & (0x80 >> col) != 0 {
                    self.set_pixel(x + col, y + row, 0xFFFFFF);
                }
            }
        }

        self.cursor_x += char_width;
    }

    /// Set a single pixel to a 24-bit RGB color (0xRRGGBB).
    ///
    /// # Safety
    /// The caller must ensure the framebuffer parameters are valid and the
    /// pixel is within bounds. This is a raw memory write to the framebuffer.
    fn set_pixel(&mut self, x: u32, y: u32, color: u32) {
        if x >= self.width || y >= self.height {
            return;
        }
        let offset = y * self.stride + x;
        let pixel = match self.format {
            PIXEL_FORMAT_BGRA => {
                // B, G, R, Reserved
                let r = (color >> 16) & 0xFF;
                let g = (color >> 8) & 0xFF;
                let b = color & 0xFF;
                (b << 16) | (g << 8) | r
            }
            _ => {
                // RGBA: R, G, B, Reserved
                let r = (color >> 16) & 0xFF;
                let g = (color >> 8) & 0xFF;
                let b = color & 0xFF;
                (r << 16) | (g << 8) | b
            }
        };
        unsafe {
            self.base.add(offset as usize).write_volatile(pixel);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_framebuffer() {
        assert!(FramebufferConsole::new(0, 100, 100, 100, PIXEL_FORMAT_BGRA).is_none());
        assert!(FramebufferConsole::new(0x1000, 0, 100, 100, PIXEL_FORMAT_BGRA).is_none());
        assert!(FramebufferConsole::new(0x1000, 100, 0, 100, PIXEL_FORMAT_BGRA).is_none());
        assert!(FramebufferConsole::new(0x1000, 100, 100, 50, PIXEL_FORMAT_BGRA).is_none());
        assert!(FramebufferConsole::new(0x1000, 100, 100, 100, 99).is_none());
    }

    #[test]
    fn accepts_valid_framebuffer() {
        assert!(FramebufferConsole::new(0x1000, 100, 100, 100, PIXEL_FORMAT_BGRA).is_some());
        assert!(FramebufferConsole::new(0x1000, 100, 100, 100, PIXEL_FORMAT_RGBA).is_some());
    }
}
