//! Userspace software presentation; bounded tiles cross the native syscall ABI.
use crate::{App, CELL_HEIGHT, CELL_WIDTH, Desktop, Terminal, files::Files, font, task_button};
use vibrix_syscall::{self as syscall, Result, display::Rect};
pub const BACKGROUND: u32 = 0x0010_1925;
pub const FOREGROUND: u32 = 0x00db_e7f3;
const PANEL: u32 = 0x0022_3750;
const ACCENT: u32 = 0x005e_a5ed;
const MUTED: u32 = 0x0087_a5be;

pub trait Canvas {
    fn fill(&mut self, rect: Rect, color: u32) -> Result<()>;
    fn blit(&mut self, rect: Rect, pixels: &[u32]) -> Result<()>;
}
pub struct Native;
impl Canvas for Native {
    fn fill(&mut self, rect: Rect, color: u32) -> Result<()> {
        syscall::display_fill(rect, color)
    }
    fn blit(&mut self, rect: Rect, pixels: &[u32]) -> Result<()> {
        syscall::display_blit(rect, pixels)
    }
}
struct Painter<'a, C> {
    canvas: &'a mut C,
    width: u32,
    height: u32,
}
impl<C: Canvas> Painter<'_, C> {
    fn fill(&mut self, x: u32, y: u32, width: u32, height: u32, color: u32) -> Result<()> {
        let width = width.min(self.width.saturating_sub(x));
        let height = height.min(self.height.saturating_sub(y));
        if width == 0 || height == 0 {
            return Ok(());
        }
        self.canvas.fill(
            Rect {
                x,
                y,
                width,
                height,
            },
            color,
        )
    }
    fn text(&mut self, x: u32, y: u32, bytes: &[u8], colors: (u32, u32), limit: u32) -> Result<()> {
        if y.saturating_add(CELL_HEIGHT) > self.height {
            return Ok(());
        }
        let count = bytes
            .len()
            .min((limit.min(self.width.saturating_sub(x)) / CELL_WIDTH) as usize);
        let mut pixels = [0u32; 960];
        for (index, chunk) in bytes[..count].chunks(5).enumerate() {
            let width = chunk.len() * CELL_WIDTH as usize;
            for (cell, &byte) in chunk.iter().enumerate() {
                let glyph = font::glyph(byte);
                for y in 0..16 {
                    for x in 0..12 {
                        let on = y < 14 && x < 10 && glyph[y / 2] & (1 << (4 - x / 2)) != 0;
                        pixels[y * width + cell * 12 + x] = if on { colors.0 } else { colors.1 };
                    }
                }
            }
            self.canvas.blit(
                Rect {
                    x: x + index as u32 * 60,
                    y,
                    width: width as u32,
                    height: 16,
                },
                &pixels[..width * 16],
            )?;
        }
        Ok(())
    }
    fn terminal(&mut self, desktop: &Desktop, terminal: &mut Terminal) -> Result<()> {
        for row in 0..terminal.rows {
            if terminal.dirty[row] {
                let start = row * terminal.columns;
                self.text(
                    desktop.window.x + 16,
                    desktop.window.y + 44 + row as u32 * 16,
                    &terminal.cells[start..start + terminal.columns],
                    (FOREGROUND, BACKGROUND),
                    desktop.window.width - 32,
                )?;
                terminal.dirty[row] = false;
            }
        }
        self.fill(
            desktop.window.x + 16 + terminal.column.min(terminal.columns - 1) as u32 * 12,
            desktop.window.y + 44 + terminal.row as u32 * 16 + 14,
            10,
            2,
            ACCENT,
        )
    }
    fn pointer(&mut self, desktop: &Desktop) -> Result<()> {
        // Filled arrow and its outline; the scene is repainted before motion.
        let x = desktop.pointer_x as u32;
        let y = desktop.pointer_y as u32;
        for row in 0..16 {
            let width = (row / 2 + 1).min(10);
            self.fill(x, y + row, width, 1, 0x0005_0910)?;
            if width > 2 {
                self.fill(x + 1, y + row, width - 2, 1, 0x00ff_ffff)?;
            }
        }
        Ok(())
    }
}

pub fn draw<C: Canvas>(
    canvas: &mut C,
    desktop: &mut Desktop,
    terminal: &mut Terminal,
    files: &Files,
) -> Result<()> {
    let mut p = Painter {
        canvas,
        width: desktop.width,
        height: desktop.height,
    };
    if desktop.full_redraw {
        p.fill(0, 0, desktop.width, desktop.height, 0x0008_121d)?;
        p.fill(0, 36, desktop.width / 3, desktop.height - 80, 0x000c_1b2b)?;
        p.fill(desktop.width / 3, 36, 2, desktop.height - 80, 0x0017_2e43)?;
        p.text(
            52,
            100,
            b"VIBRIX",
            (ACCENT, 0x000c_1b2b),
            desktop.width - 60,
        )?;
        p.text(
            52,
            128,
            b"Your native workspace.",
            (MUTED, 0x000c_1b2b),
            desktop.width - 60,
        )?;
        p.fill(0, 0, desktop.width, 36, PANEL)?;
        p.text(16, 10, b"VIBRIX", (FOREGROUND, PANEL), 84)?;
        p.text(112, 10, b"Desktop Preview", (MUTED, PANEL), 192)?;
        p.text(
            desktop.width - 252,
            10,
            b"Ring 3 / native Rust",
            (FOREGROUND, PANEL),
            240,
        )?;
        p.fill(0, desktop.height - 46, desktop.width, 46, PANEL)?;
        for (index, label) in [
            b"F1 Terminal".as_slice(),
            b"F2 Files",
            b"F3 System",
            b"F4 Port",
        ]
        .iter()
        .enumerate()
        {
            let rect = task_button(desktop.height, index);
            let background = if desktop.visible && desktop.app.index() == index {
                0x0034_597c
            } else {
                BACKGROUND
            };
            p.fill(rect.x, rect.y, rect.width, rect.height, background)?;
            p.text(
                rect.x + 6,
                rect.y + 6,
                label,
                (FOREGROUND, background),
                rect.width - 12,
            )?;
        }
        if desktop.visible {
            let w = desktop.window;
            p.fill(w.x + 6, w.y + 6, w.width, w.height, 0x0003_0810)?;
            p.fill(w.x, w.y, w.width, w.height, ACCENT)?;
            p.fill(w.x + 1, w.y + 1, w.width - 2, 31, PANEL)?;
            p.fill(w.x + 1, w.y + 32, w.width - 2, w.height - 33, BACKGROUND)?;
            p.text(
                w.x + 12,
                w.y + 9,
                desktop.app.title(),
                (FOREGROUND, PANEL),
                w.width - 120,
            )?;
            p.text(
                w.x + w.width - 94,
                w.y + 9,
                b"_  +  x",
                (FOREGROUND, PANEL),
                88,
            )?;
            match desktop.app {
                App::Terminal => {
                    terminal.dirty.fill(true);
                    p.terminal(desktop, terminal)?;
                }
                App::Files => {
                    p.text(
                        w.x + 16,
                        w.y + 44,
                        files.cwd.as_bytes(),
                        (ACCENT, BACKGROUND),
                        w.width - 32,
                    )?;
                    p.text(
                        w.x + 16,
                        w.y + 68,
                        b"[..] Parent",
                        (FOREGROUND, BACKGROUND),
                        220,
                    )?;
                    let visible = ((w.height - 132) / 20) as usize;
                    let first = files.selected / visible * visible;
                    for (row, entry) in files.entries[..files.count]
                        .iter()
                        .enumerate()
                        .skip(first)
                        .take(visible)
                    {
                        let y = w.y + 96 + (row - first) as u32 * 20;
                        let color = if row == files.selected {
                            ACCENT
                        } else {
                            FOREGROUND
                        };
                        let marker = if entry.kind == vibrix_syscall::abi::ENTRY_DIRECTORY {
                            b"+"
                        } else {
                            b" "
                        };
                        p.text(w.x + 16, y, marker, (color, BACKGROUND), 12)?;
                        p.text(
                            w.x + 32,
                            y,
                            &entry.name[..usize::from(entry.name_len)],
                            (color, BACKGROUND),
                            208,
                        )?;
                    }
                    p.fill(w.x + 248, w.y + 44, 1, w.height - 76, PANEL)?;
                    let mut preview = Terminal::new(
                        ((w.width - 280) / 12) as usize,
                        ((w.height - 132) / 16) as usize,
                    );
                    preview.write(&files.preview[..files.preview_len]);
                    p.text(
                        w.x + 268,
                        w.y + 68,
                        b"Read-only preview",
                        (MUTED, BACKGROUND),
                        w.width - 284,
                    )?;
                    for row in 0..preview.rows {
                        p.text(
                            w.x + 268,
                            w.y + 96 + row as u32 * 16,
                            &preview.cells[row * preview.columns..(row + 1) * preview.columns],
                            (FOREGROUND, BACKGROUND),
                            w.width - 284,
                        )?;
                    }
                    p.text(
                        w.x + 16,
                        w.y + w.height - 22,
                        files.message,
                        (MUTED, BACKGROUND),
                        w.width - 32,
                    )?;
                }
                App::System => {
                    for (index, line) in [
                        b"Vibrix native desktop / 0.0.1".as_slice(),
                        b"Rust ELF running at CPU privilege level 3.",
                        b"Original kernel, loader, syscalls and VFS.",
                        b"Mouse: drag the title bar; click task buttons.",
                        b"F11: maximize. Escape: minimize or restore.",
                        b"Terminal uses the real vibrix-sh command engine.",
                        b"Files and settings are NOT persistent yet.",
                        b"One process / one foreground window / ASCII.",
                        b"No GPU acceleration, clipboard or USB desktop.",
                        b"Chromium is not installed or running.",
                    ]
                    .iter()
                    .enumerate()
                    {
                        p.text(
                            w.x + 20,
                            w.y + 52 + index as u32 * 24,
                            line,
                            (FOREGROUND, BACKGROUND),
                            w.width - 40,
                        )?;
                    }
                }
                App::BrowserStatus => {
                    for (index, line) in [
                        b"CHROMIUM: NOT PORTED".as_slice(),
                        b"This is a status panel, NOT a browser.",
                        b"Native input and software graphics: available.",
                        b"Still missing for an actual Chromium port:",
                        b"* C/C++ runtime and platform build target",
                        b"* user processes, threads, TLS and IPC",
                        b"* VM allocation, shared memory, file mapping",
                        b"* sockets, DNS, TLS, fonts and sandboxing",
                        b"* Ozone window/surface integration",
                        b"No hidden Linux guest or host Chromium.",
                        b"See docs/CHROMIUM_PORT.md for acceptance gates.",
                    ]
                    .iter()
                    .enumerate()
                    {
                        p.text(
                            w.x + 20,
                            w.y + 52 + index as u32 * 24,
                            line,
                            (if index == 0 { ACCENT } else { FOREGROUND }, BACKGROUND),
                            w.width - 40,
                        )?;
                    }
                }
            }
        }
        desktop.full_redraw = false;
    } else if desktop.visible && desktop.app == App::Terminal {
        p.terminal(desktop, terminal)?;
    }
    p.pointer(desktop)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Checked {
        width: u32,
        height: u32,
        calls: usize,
    }
    impl Canvas for Checked {
        fn fill(&mut self, rect: Rect, _: u32) -> Result<()> {
            assert!(rect.fits(self.width, self.height));
            self.calls += 1;
            Ok(())
        }
        fn blit(&mut self, rect: Rect, pixels: &[u32]) -> Result<()> {
            assert!(rect.fits(self.width, self.height));
            assert_eq!(rect.pixels(), Some(pixels.len()));
            assert!(pixels.len() <= 1024);
            self.calls += 1;
            Ok(())
        }
    }
    #[test]
    fn every_view_and_maximize_emit_bounded_real_graphics_operations() {
        for (width, height) in [(640, 480), (800, 600), (1024, 768), (1920, 1080)] {
            let mut d = Desktop::new(width, height).unwrap();
            let mut terminal = Terminal::new(
                ((d.window.width - 32) / 12) as usize,
                ((d.window.height - 68) / 16) as usize,
            );
            let files = Files::new();
            let mut c = Checked {
                width,
                height,
                calls: 0,
            };
            for index in 0..4 {
                d.select(App::at(index));
                draw(&mut c, &mut d, &mut terminal, &files).unwrap();
                d.maximize();
                draw(&mut c, &mut d, &mut terminal, &files).unwrap();
                d.maximize();
            }
            assert!(c.calls > 100);
        }
    }
}
