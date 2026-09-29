//! Allocation-free ASCII terminal over the retained supervisor GOP mapping.
//! The shell supplies every displayed prompt and command result; this module
//! only interprets text, line breaks, tabs and destructive backspace.

use super::{BootInfo, PixelSurface};

const MARGIN: usize = 16;
const CELL_WIDTH: usize = 12;
const CELL_HEIGHT: usize = 16;
const MAX_COLUMNS: usize = 80;
const MAX_ROWS: usize = 30;
const BACKGROUND: u32 = 0x0012_1212;
const FOREGROUND: u32 = 0x00e6_e6e6;

#[path = "../../../shared/font.rs"]
mod font;
use font::glyph;

struct Terminal {
    pixels: PixelSurface,
    cells: [u8; MAX_COLUMNS * MAX_ROWS],
    columns: usize,
    rows: usize,
    column: usize,
    row: usize,
}

impl Terminal {
    /// # Safety
    /// The complete validated GOP mapping must remain writable and exclusively
    /// owned until this terminal is dropped. No other renderer may use it.
    /// Every subsequent pixel access must run under the captured kernel root.
    unsafe fn new(info: &BootInfo) -> Result<Self, ()> {
        let pixels = PixelSurface::new(info)?;
        if info.framebuffer_format > 1 || pixels.width > 8192 || pixels.height > 8192 {
            return Err(());
        }
        let columns =
            (pixels.width.checked_sub(MARGIN * 2).ok_or(())? / CELL_WIDTH).min(MAX_COLUMNS);
        let rows = (pixels.height.checked_sub(MARGIN * 2).ok_or(())? / CELL_HEIGHT).min(MAX_ROWS);
        if columns < 2 || rows < 2 {
            return Err(());
        }
        let terminal = Self {
            pixels,
            cells: [b' '; MAX_COLUMNS * MAX_ROWS],
            columns,
            rows,
            column: 0,
            row: 0,
        };
        for y in 0..terminal.pixels.height {
            for x in 0..terminal.pixels.width {
                terminal.pixel(x, y, BACKGROUND)?;
            }
        }
        Ok(terminal)
    }

    fn pixel(&self, x: usize, y: usize, color: u32) -> Result<(), ()> {
        // SAFETY: new() acquired retained mapping ownership; runtime access
        // uses the captured kernel CR3. PixelSurface checks coordinates/bytes.
        unsafe { self.pixels.pixel(x, y, color) }
    }

    fn draw_cell(&self, column: usize, row: usize) -> Result<(), ()> {
        let bitmap = glyph(self.cells[row * self.columns + column]);
        for y in 0..CELL_HEIGHT {
            for x in 0..CELL_WIDTH {
                let on = y < 14 && x < 10 && bitmap[y / 2] & (1 << (4 - x / 2)) != 0;
                self.pixel(
                    MARGIN + column * CELL_WIDTH + x,
                    MARGIN + row * CELL_HEIGHT + y,
                    if on { FOREGROUND } else { BACKGROUND },
                )?;
            }
        }
        Ok(())
    }

    fn newline(&mut self) -> Result<(), ()> {
        self.column = 0;
        if self.row + 1 < self.rows {
            self.row += 1;
            return Ok(());
        }
        let end = self.columns * self.rows;
        self.cells.copy_within(self.columns..end, 0);
        self.cells[end - self.columns..end].fill(b' ');
        for row in 0..self.rows {
            for column in 0..self.columns {
                self.draw_cell(column, row)?;
            }
        }
        Ok(())
    }

    fn clear(&mut self) -> Result<(), ()> {
        let end = self.columns * self.rows;
        self.cells[..end].fill(b' ');
        self.column = 0;
        self.row = 0;
        for row in 0..self.rows {
            for column in 0..self.columns {
                self.draw_cell(column, row)?;
            }
        }
        Ok(())
    }

    fn printable(&mut self, byte: u8) -> Result<(), ()> {
        // Delay wrap until the next printable byte so an exactly full line
        // followed by LF advances once, not twice.
        if self.column == self.columns {
            self.newline()?;
        }
        self.cells[self.row * self.columns + self.column] = byte;
        self.draw_cell(self.column, self.row)?;
        self.column += 1;
        Ok(())
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), ()> {
        self.draw_cell(self.column.min(self.columns - 1), self.row)?;
        for &byte in bytes {
            match byte {
                b'\n' => self.newline()?,
                b'\r' => self.column = 0,
                0x0c => self.clear()?,
                8 | 127 => {
                    if self.column > 0 {
                        self.column -= 1;
                    } else if self.row > 0 {
                        self.row -= 1;
                        self.column = self.columns - 1;
                    } else {
                        continue;
                    }
                    self.cells[self.row * self.columns + self.column] = b' ';
                    self.draw_cell(self.column, self.row)?;
                }
                b'\t' => {
                    for _ in 0..(4 - self.column % 4) {
                        self.printable(b' ')?;
                    }
                }
                0x20..=0x7e => self.printable(byte)?,
                0x80..=0xff => self.printable(b'?')?,
                _ => {}
            }
        }
        let column = self.column.min(self.columns - 1);
        for y in 14..CELL_HEIGHT {
            for x in 0..10 {
                self.pixel(
                    MARGIN + column * CELL_WIDTH + x,
                    MARGIN + self.row * CELL_HEIGHT + y,
                    FOREGROUND,
                )?;
            }
        }
        Ok(())
    }
}

#[cfg(all(feature = "userspace-shell", not(test)))]
#[path = "terminal_runtime.rs"]
mod runtime;

#[cfg(all(feature = "userspace-shell", not(test)))]
pub use runtime::{init, write};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bootinfo::{FinalMemoryMap, FramebufferInfo};

    fn info(buffer: &mut [u32], columns: usize, rows: usize) -> BootInfo {
        let width = MARGIN * 2 + columns * CELL_WIDTH;
        let height = MARGIN * 2 + rows * CELL_HEIGHT;
        BootInfo::new(
            FramebufferInfo {
                physical_base: buffer.as_mut_ptr() as u64,
                size_bytes: ((width + 3) * height * 4) as u64,
                width: width as u32,
                height: height as u32,
                stride: (width + 3) as u32,
                pixel_format: 1,
            },
            0x1000,
            FinalMemoryMap {
                physical_base: 0x2000,
                byte_len: 48,
                descriptor_size: 48,
                descriptor_version: 1,
            },
            0x3000,
        )
        .unwrap()
    }

    #[test]
    fn shell_text_and_cursor_are_real_pixels_without_stride_overwrite() {
        let mut pixels = vec![0xaabb_ccdd; 400 * 200];
        let boot = info(&mut pixels, 24, 6);
        // SAFETY: uniquely owned live host buffer exceeds the declared backing.
        let mut terminal = unsafe { Terminal::new(&boot) }.unwrap();
        terminal.write(b"Vibrix shell\nvibrix$ ").unwrap();
        assert_eq!(&terminal.cells[..12], b"Vibrix shell");
        assert_eq!(&terminal.cells[24..32], b"vibrix$ ");
        let stride = boot.framebuffer_stride as usize;
        assert_eq!(pixels[MARGIN * stride + MARGIN], FOREGROUND);
        assert_eq!(
            pixels[(MARGIN + CELL_HEIGHT + 14) * stride + MARGIN + 8 * CELL_WIDTH],
            FOREGROUND
        );
        for row in 0..boot.framebuffer_height as usize {
            let end = row * stride + boot.framebuffer_width as usize;
            assert_eq!(&pixels[end..end + 3], &[0xaabb_ccdd; 3]);
        }
        let end = (boot.framebuffer_size / 4) as usize;
        assert!(pixels[end..].iter().all(|&pixel| pixel == 0xaabb_ccdd));
    }

    #[test]
    fn destructive_backspace_and_carriage_return_update_cells() {
        let mut pixels = vec![0; 200 * 200];
        let boot = info(&mut pixels, 8, 3);
        // SAFETY: uniquely owned live host pixel storage.
        let mut terminal = unsafe { Terminal::new(&boot) }.unwrap();
        terminal.write(b"abc\x08Z").unwrap();
        assert_eq!(&terminal.cells[..4], b"abZ ");
        terminal.write(b"\rQ").unwrap();
        assert_eq!(&terminal.cells[..4], b"QbZ ");
    }

    #[test]
    fn form_feed_clears_cells_and_homes_cursor() {
        let mut pixels = vec![0; 200 * 200];
        let boot = info(&mut pixels, 4, 2);
        // SAFETY: uniquely owned live host pixel storage.
        let mut terminal = unsafe { Terminal::new(&boot) }.unwrap();
        terminal.write(b"abc\ndef\x0c").unwrap();
        assert_eq!(&terminal.cells[..8], b"        ");
        assert_eq!((terminal.row, terminal.column), (0, 0));
    }

    #[test]
    fn full_line_then_newline_advances_exactly_once() {
        let mut pixels = vec![0; 200 * 200];
        let boot = info(&mut pixels, 4, 2);
        // SAFETY: uniquely owned live host pixel storage.
        let mut terminal = unsafe { Terminal::new(&boot) }.unwrap();
        terminal.write(b"abcd\nE").unwrap();
        assert_eq!(&terminal.cells[..8], b"abcdE   ");
        assert_eq!((terminal.row, terminal.column), (1, 1));
    }

    #[test]
    fn scroll_moves_text_and_clears_the_last_line() {
        let mut pixels = vec![0; 200 * 200];
        let boot = info(&mut pixels, 4, 2);
        // SAFETY: uniquely owned live host pixel storage.
        let mut terminal = unsafe { Terminal::new(&boot) }.unwrap();
        terminal.write(b"a\nb\nc").unwrap();
        assert_eq!(&terminal.cells[..8], b"b   c   ");
        terminal.write(b"\tX\xff").unwrap();
        assert!(terminal.cells[..8].contains(&b'?'));
    }

    #[test]
    fn rejects_bitmask_and_corrupt_backing_without_writes() {
        let mut pixels = vec![0xaabb_ccdd; 200 * 200];
        let mut boot = info(&mut pixels, 4, 2);
        boot.framebuffer_format = 2;
        // SAFETY: malformed metadata must be rejected before any MMIO access.
        assert!(unsafe { Terminal::new(&boot) }.is_err());
        boot.framebuffer_format = 1;
        boot.framebuffer_size = 4;
        // SAFETY: same reject-before-write test invariant.
        assert!(unsafe { Terminal::new(&boot) }.is_err());
        assert!(pixels.iter().all(|&pixel| pixel == 0xaabb_ccdd));
    }

    #[test]
    fn printable_ascii_glyphs_are_bounded_and_nonblank() {
        assert_eq!(glyph(b' '), [0; 7]);
        for byte in b'!'..=b'~' {
            let rows = glyph(byte);
            assert!(rows.iter().any(|&row| row != 0));
            assert!(rows.iter().all(|&row| row < 32));
        }
    }
}
