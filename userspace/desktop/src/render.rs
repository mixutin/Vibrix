//! Userspace software presentation; bounded tiles cross the native syscall ABI.
use crate::{App, Desktop, Terminal, files::Files};
use vibrix_syscall::Result;
use vibrix_ui::{CELL_HEIGHT, CELL_WIDTH, Painter, Theme, paint_button, paint_window_frame};
pub use vibrix_ui::{Canvas, NativeCanvas as Native};

pub const BACKGROUND: u32 = Theme::VIBRIX.background;
pub const FOREGROUND: u32 = Theme::VIBRIX.foreground;
const PANEL: u32 = Theme::VIBRIX.panel;
const ACCENT: u32 = Theme::VIBRIX.accent;
const MUTED: u32 = Theme::VIBRIX.muted;

fn draw_terminal<C: Canvas>(
    painter: &mut Painter<'_, C>,
    desktop: &Desktop,
    terminal: &mut Terminal,
) -> Result<()> {
    for row in 0..terminal.rows {
        if terminal.dirty[row] {
            let start = row * terminal.columns;
            painter.text(
                desktop.window.x + 16,
                desktop.window.y + 44 + row as u32 * CELL_HEIGHT,
                &terminal.cells[start..start + terminal.columns],
                (FOREGROUND, BACKGROUND),
                desktop.window.width - 32,
            )?;
            terminal.dirty[row] = false;
        }
    }
    painter.fill(
        desktop.window.x + 16 + terminal.column.min(terminal.columns - 1) as u32 * CELL_WIDTH,
        desktop.window.y + 44 + terminal.row as u32 * CELL_HEIGHT + 14,
        10,
        2,
        ACCENT,
    )
}

fn pointer<C: Canvas>(painter: &mut Painter<'_, C>, desktop: &Desktop) -> Result<()> {
    let x = desktop.pointer_x as u32;
    let y = desktop.pointer_y as u32;
    for row in 0..16 {
        let width = (row / 2 + 1).min(10);
        painter.fill(x, y + row, width, 1, 0x0005_0910)?;
        if width > 2 {
            painter.fill(x + 1, y + row, width - 2, 1, 0x00ff_ffff)?;
        }
    }
    Ok(())
}

pub fn draw<C: Canvas>(
    canvas: &mut C,
    desktop: &mut Desktop,
    terminal: &mut Terminal,
    files: &Files,
) -> Result<()> {
    let mut p = Painter::new(canvas, desktop.width, desktop.height);
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
            let rect = crate::task_button(desktop.height, index);
            paint_button(
                &mut p,
                rect,
                label,
                desktop.visible && desktop.app.index() == index,
                Theme::VIBRIX,
            )?;
        }
        if desktop.visible {
            let w = desktop.window;
            paint_window_frame(&mut p, w, desktop.app.title(), Theme::VIBRIX)?;
            match desktop.app {
                App::Terminal => {
                    terminal.dirty.fill(true);
                    draw_terminal(&mut p, desktop, terminal)?;
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
        draw_terminal(&mut p, desktop, terminal)?;
    }
    pointer(&mut p, desktop)
}

#[cfg(test)]
mod tests {
    use vibrix_syscall::display::Rect;
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
