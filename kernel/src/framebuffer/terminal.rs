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

/// Original five-column glyphs, doubled in each dimension at presentation.
/// No external font, allocation, firmware call or userspace MMIO is needed.
fn glyph(byte: u8) -> [u8; 7] {
    match byte {
        b' ' => [0; 7],
        b'!' => [4, 4, 4, 4, 4, 0, 4],
        b'"' => [10, 10, 10, 0, 0, 0, 0],
        b'#' => [10, 10, 31, 10, 31, 10, 10],
        b'$' => [4, 15, 20, 14, 5, 30, 4],
        b'%' => [24, 25, 2, 4, 8, 19, 3],
        b'&' => [12, 18, 20, 8, 21, 18, 13],
        b'\'' => [4, 4, 8, 0, 0, 0, 0],
        b'(' => [2, 4, 8, 8, 8, 4, 2],
        b')' => [8, 4, 2, 2, 2, 4, 8],
        b'*' => [0, 21, 14, 31, 14, 21, 0],
        b'+' => [0, 4, 4, 31, 4, 4, 0],
        b',' => [0, 0, 0, 0, 4, 4, 8],
        b'-' => [0, 0, 0, 31, 0, 0, 0],
        b'.' => [0, 0, 0, 0, 0, 12, 12],
        b'/' => [1, 2, 2, 4, 8, 8, 16],
        b'0' => [14, 17, 19, 21, 25, 17, 14],
        b'1' => [4, 12, 4, 4, 4, 4, 14],
        b'2' => [14, 17, 1, 2, 4, 8, 31],
        b'3' => [30, 1, 1, 14, 1, 1, 30],
        b'4' => [2, 6, 10, 18, 31, 2, 2],
        b'5' => [31, 16, 16, 30, 1, 1, 30],
        b'6' => [14, 16, 16, 30, 17, 17, 14],
        b'7' => [31, 1, 2, 4, 8, 8, 8],
        b'8' => [14, 17, 17, 14, 17, 17, 14],
        b'9' => [14, 17, 17, 15, 1, 1, 14],
        b':' => [0, 12, 12, 0, 12, 12, 0],
        b';' => [0, 12, 12, 0, 4, 4, 8],
        b'<' => [1, 2, 4, 8, 4, 2, 1],
        b'=' => [0, 0, 31, 0, 31, 0, 0],
        b'>' => [16, 8, 4, 2, 4, 8, 16],
        b'?' => [14, 17, 1, 2, 4, 0, 4],
        b'@' => [14, 17, 23, 21, 23, 16, 14],
        b'A' => [14, 17, 17, 31, 17, 17, 17],
        b'B' => [30, 17, 17, 30, 17, 17, 30],
        b'C' => [14, 17, 16, 16, 16, 17, 14],
        b'D' => [30, 17, 17, 17, 17, 17, 30],
        b'E' => [31, 16, 16, 30, 16, 16, 31],
        b'F' => [31, 16, 16, 30, 16, 16, 16],
        b'G' => [14, 17, 16, 23, 17, 17, 15],
        b'H' => [17, 17, 17, 31, 17, 17, 17],
        b'I' => [14, 4, 4, 4, 4, 4, 14],
        b'J' => [7, 2, 2, 2, 2, 18, 12],
        b'K' => [17, 18, 20, 24, 20, 18, 17],
        b'L' => [16, 16, 16, 16, 16, 16, 31],
        b'M' => [17, 27, 21, 21, 17, 17, 17],
        b'N' => [17, 25, 25, 21, 19, 19, 17],
        b'O' => [14, 17, 17, 17, 17, 17, 14],
        b'P' => [30, 17, 17, 30, 16, 16, 16],
        b'Q' => [14, 17, 17, 17, 21, 18, 13],
        b'R' => [30, 17, 17, 30, 20, 18, 17],
        b'S' => [15, 16, 16, 14, 1, 1, 30],
        b'T' => [31, 4, 4, 4, 4, 4, 4],
        b'U' => [17, 17, 17, 17, 17, 17, 14],
        b'V' => [17, 17, 17, 17, 17, 10, 4],
        b'W' => [17, 17, 17, 21, 21, 27, 17],
        b'X' => [17, 17, 10, 4, 10, 17, 17],
        b'Y' => [17, 17, 10, 4, 4, 4, 4],
        b'Z' => [31, 1, 2, 4, 8, 16, 31],
        b'[' => [14, 8, 8, 8, 8, 8, 14],
        b'\\' => [16, 8, 8, 4, 2, 2, 1],
        b']' => [14, 2, 2, 2, 2, 2, 14],
        b'^' => [4, 10, 17, 0, 0, 0, 0],
        b'_' => [0, 0, 0, 0, 0, 0, 31],
        b'`' => [8, 4, 2, 0, 0, 0, 0],
        b'a' => [0, 0, 14, 1, 15, 17, 15],
        b'b' => [16, 16, 30, 17, 17, 17, 30],
        b'c' => [0, 0, 14, 17, 16, 17, 14],
        b'd' => [1, 1, 15, 17, 17, 17, 15],
        b'e' => [0, 0, 14, 17, 31, 16, 14],
        b'f' => [6, 9, 8, 28, 8, 8, 8],
        b'g' => [0, 0, 15, 17, 15, 1, 14],
        b'h' => [16, 16, 30, 17, 17, 17, 17],
        b'i' => [4, 0, 12, 4, 4, 4, 14],
        b'j' => [2, 0, 6, 2, 2, 18, 12],
        b'k' => [16, 16, 18, 20, 24, 20, 18],
        b'l' => [12, 4, 4, 4, 4, 4, 14],
        b'm' => [0, 0, 26, 21, 21, 21, 21],
        b'n' => [0, 0, 30, 17, 17, 17, 17],
        b'o' => [0, 0, 14, 17, 17, 17, 14],
        b'p' => [0, 0, 30, 17, 30, 16, 16],
        b'q' => [0, 0, 15, 17, 15, 1, 1],
        b'r' => [0, 0, 22, 25, 16, 16, 16],
        b's' => [0, 0, 15, 16, 14, 1, 30],
        b't' => [8, 8, 28, 8, 8, 9, 6],
        b'u' => [0, 0, 17, 17, 17, 19, 13],
        b'v' => [0, 0, 17, 17, 17, 10, 4],
        b'w' => [0, 0, 17, 17, 21, 21, 10],
        b'x' => [0, 0, 17, 10, 4, 10, 17],
        b'y' => [0, 0, 17, 17, 15, 1, 14],
        b'z' => [0, 0, 31, 2, 4, 8, 31],
        b'{' => [2, 4, 4, 8, 4, 4, 2],
        b'|' => [4, 4, 4, 4, 4, 4, 4],
        b'}' => [8, 4, 4, 2, 4, 4, 8],
        b'~' => [0, 0, 8, 21, 2, 0, 0],
        _ => [14, 17, 1, 2, 4, 0, 4],
    }
}

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
        // SAFETY: new() acquired the retained mapping for this object's entire
        // lifetime. PixelSurface independently checks coordinates/byte bounds.
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
mod runtime {
    use super::{BootInfo, Terminal};
    use core::{
        cell::UnsafeCell,
        sync::atomic::{AtomicBool, Ordering},
    };

    struct Console(UnsafeCell<Option<Terminal>>);
    // SAFETY: initialization and rendering are exclusively serialized by BUSY.
    // Only the BSP is started; IRQ/NMI/panic handlers never call this renderer.
    unsafe impl Sync for Console {}

    static CONSOLE: Console = Console(UnsafeCell::new(None));
    static BUSY: AtomicBool = AtomicBool::new(false);
    static STARTED: AtomicBool = AtomicBool::new(false);

    fn interrupts_enabled() -> bool {
        let flags: u64;
        // SAFETY: read-only RFLAGS inspection, no device or memory mutation.
        unsafe {
            core::arch::asm!("pushfq", "pop {}", out(reg) flags, options(preserves_flags));
        }
        flags & (1 << 9) != 0
    }

    /// # Safety
    /// Single-BSP boot, IF=0, and exclusive retained supervisor GOP mapping.
    /// The private user CR3 must inherit this same supervisor mapping (the
    /// current address_space::copy_kernel_root contract). No later renderer
    /// may write through draw_boot_marker after ownership is published here.
    pub unsafe fn init(info: &BootInfo) -> Result<(), ()> {
        if interrupts_enabled() || STARTED.swap(true, Ordering::SeqCst) {
            return Err(());
        }
        BUSY.store(true, Ordering::SeqCst);
        // SAFETY: the caller supplies the permanent MMIO lifetime/ownership.
        let terminal = unsafe { Terminal::new(info) };
        let success = terminal.is_ok();
        // SAFETY: pre-STI initialization, exclusively owning BUSY.
        unsafe { *CONSOLE.0.get() = terminal.ok() };
        BUSY.store(false, Ordering::SeqCst);
        if success { Ok(()) } else { Err(()) }
    }

    /// Called only with kernel-owned bytes from the syscall TTY drain/echo.
    /// Fail closed instead of spinning if called reentrantly or from IF=1.
    /// Serial logging is independent and remains available on display failure.
    pub fn write(bytes: &[u8]) {
        if interrupts_enabled() || BUSY.swap(true, Ordering::SeqCst) {
            return;
        }
        // SAFETY: BUSY grants one exclusive borrower. Every active userspace
        // root inherits the retained supervisor GOP mapping; bytes are already
        // copied into kernel memory, never dereferenced as userspace pointers.
        let slot = unsafe { &mut *CONSOLE.0.get() };
        if let Some(terminal) = slot.as_mut()
            && terminal.write(bytes).is_err()
        {
            *slot = None;
        }
        BUSY.store(false, Ordering::SeqCst);
    }
}

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
