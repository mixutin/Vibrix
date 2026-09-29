#![no_std]
//! A native single-process desktop session, not a multi-process compositor.
pub mod files;
#[path = "../../../shared/font.rs"]
pub mod font;
pub mod render;
pub mod system;
use vibrix_syscall::display::{self, Rect};

pub const MAX_COLUMNS: usize = 80;
pub const MAX_ROWS: usize = 30;
pub const CELL_WIDTH: u32 = 12;
pub const CELL_HEIGHT: u32 = 16;

pub struct Terminal {
    pub cells: [u8; MAX_COLUMNS * MAX_ROWS],
    pub columns: usize,
    pub rows: usize,
    pub column: usize,
    pub row: usize,
    pub dirty: [bool; MAX_ROWS],
}
impl Terminal {
    pub fn new(columns: usize, rows: usize) -> Self {
        Self {
            cells: [b' '; MAX_COLUMNS * MAX_ROWS],
            columns: columns.clamp(2, MAX_COLUMNS),
            rows: rows.clamp(2, MAX_ROWS),
            column: 0,
            row: 0,
            dirty: [true; MAX_ROWS],
        }
    }
    fn newline(&mut self) {
        self.dirty[self.row] = true;
        self.column = 0;
        if self.row + 1 < self.rows {
            self.row += 1;
        } else {
            self.cells
                .copy_within(self.columns..self.rows * self.columns, 0);
            self.cells[(self.rows - 1) * self.columns..self.rows * self.columns].fill(b' ');
            self.dirty.fill(true);
        }
        self.dirty[self.row] = true;
    }
    pub fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            match byte {
                12 => {
                    self.cells.fill(b' ');
                    self.column = 0;
                    self.row = 0;
                    self.dirty.fill(true);
                }
                b'\n' => self.newline(),
                b'\r' => {
                    self.dirty[self.row] = true;
                    self.column = 0;
                }
                8 | 127 => {
                    self.dirty[self.row] = true;
                    if self.column != 0 {
                        self.column -= 1;
                    } else if self.row != 0 {
                        self.row -= 1;
                        self.column = self.columns - 1;
                    }
                    self.cells[self.row * self.columns + self.column] = b' ';
                    self.dirty[self.row] = true;
                }
                b'\t' => {
                    let count = 4 - self.column % 4;
                    for _ in 0..count {
                        self.write(b" ");
                    }
                }
                0x20..=0x7e => {
                    if self.column == self.columns {
                        self.newline();
                    }
                    self.cells[self.row * self.columns + self.column] = byte;
                    self.dirty[self.row] = true;
                    self.column += 1;
                }
                _ => {}
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum App {
    Terminal,
    Files,
    System,
    BrowserStatus,
}
impl App {
    pub fn index(self) -> usize {
        match self {
            Self::Terminal => 0,
            Self::Files => 1,
            Self::System => 2,
            Self::BrowserStatus => 3,
        }
    }
    pub fn at(index: usize) -> Self {
        match index {
            1 => Self::Files,
            2 => Self::System,
            3 => Self::BrowserStatus,
            _ => Self::Terminal,
        }
    }
    pub fn title(self) -> &'static [u8] {
        match self {
            Self::Terminal => b"Terminal / vibrix-sh",
            Self::Files => b"Files / volatile bootstrap filesystem",
            Self::System => b"System / native desktop preview",
            Self::BrowserStatus => b"Chromium / port status - NOT RUNNING",
        }
    }
}

pub struct Desktop {
    pub width: u32,
    pub height: u32,
    pub window: Rect,
    normal: Rect,
    pub app: App,
    pub visible: bool,
    pub maximized: bool,
    pub pointer_x: i32,
    pub pointer_y: i32,
    buttons: u32,
    dragging: Option<(i32, i32)>,
    pub full_redraw: bool,
}
impl Desktop {
    pub fn new(width: u32, height: u32) -> Option<Self> {
        if !(640..=4096).contains(&width) || !(480..=2160).contains(&height) {
            return None;
        }
        let window = Rect {
            x: 32,
            y: 56,
            width: (width - 64).min(992),
            height: (height - 112).min(576),
        };
        Some(Self {
            width,
            height,
            window,
            normal: window,
            app: App::Terminal,
            visible: true,
            maximized: false,
            pointer_x: (width - 24) as i32,
            pointer_y: 44,
            buttons: 0,
            dragging: None,
            full_redraw: true,
        })
    }
    pub fn select(&mut self, app: App) {
        self.app = app;
        self.visible = true;
        self.full_redraw = true;
    }
    pub fn maximize(&mut self) {
        self.maximized = !self.maximized;
        self.window = if self.maximized {
            Rect {
                x: 8,
                y: 44,
                width: self.width - 16,
                height: self.height - 96,
            }
        } else {
            self.normal
        };
        self.dragging = None;
        self.full_redraw = true;
    }
    pub fn key(&mut self, code: u32) -> bool {
        match code {
            display::KEY_F1..=display::KEY_F4 => {
                self.select(App::at((code - display::KEY_F1) as usize))
            }
            display::KEY_F11 => self.maximize(),
            27 => {
                self.visible = !self.visible;
                self.full_redraw = true;
            }
            _ => return false,
        }
        true
    }
    /// Returns a click in application content; decoration clicks stay local.
    pub fn pointer(&mut self, dx: i32, dy: i32, buttons: u32) -> Option<(i32, i32)> {
        let old = (self.pointer_x, self.pointer_y, self.buttons);
        self.pointer_x = self
            .pointer_x
            .saturating_add(dx)
            .clamp(0, self.width as i32 - 12);
        self.pointer_y = self
            .pointer_y
            .saturating_add(dy)
            .clamp(0, self.height as i32 - 18);
        let pressed = buttons & 1 != 0 && self.buttons & 1 == 0;
        self.buttons = buttons & 7;
        self.full_redraw |= old != (self.pointer_x, self.pointer_y, self.buttons);
        if self.buttons & 1 == 0 {
            self.dragging = None;
        }
        if let Some((offset_x, offset_y)) = self.dragging {
            self.window.x = (self.pointer_x - offset_x)
                .clamp(0, (self.width - self.window.width) as i32)
                as u32;
            self.window.y = (self.pointer_y - offset_y)
                .clamp(40, (self.height - 48 - self.window.height) as i32)
                as u32;
            self.normal = self.window;
            return None;
        }
        if !pressed {
            return None;
        }
        for index in 0..4 {
            if task_button(self.height, index).contains(self.pointer_x, self.pointer_y) {
                self.select(App::at(index));
                return None;
            }
        }
        if !self.visible || !self.window.contains(self.pointer_x, self.pointer_y) {
            return None;
        }
        let x = self.pointer_x - self.window.x as i32;
        let y = self.pointer_y - self.window.y as i32;
        if y < 32 {
            if x >= self.window.width as i32 - 36 {
                self.visible = false;
            } else if x >= self.window.width as i32 - 68 {
                self.maximize();
            } else if x >= self.window.width as i32 - 100 {
                self.visible = false;
            } else if !self.maximized {
                self.dragging = Some((x, y));
            }
            self.full_redraw = true;
            None
        } else {
            Some((x, y))
        }
    }
}

pub fn task_button(height: u32, index: usize) -> Rect {
    Rect {
        x: 12 + index as u32 * 154,
        y: height - 38,
        width: 146,
        height: 28,
    }
}

#[cfg(test)]
extern crate std;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn terminal_wrap_clear_scroll_and_erase_are_bounded() {
        let mut terminal = Terminal::new(4, 2);
        terminal.write(b"abcd\nef");
        assert_eq!((terminal.row, terminal.column), (1, 2));
        terminal.write(b"\x08g\nh");
        assert_eq!(&terminal.cells[..8], b"eg  h   ");
        terminal.write(b"\x0c");
        assert_eq!((terminal.row, terminal.column), (0, 0));
        assert!(terminal.cells.iter().all(|&b| b == b' '));
        terminal.write(&[b'x'; 4096]);
        assert!(terminal.row < terminal.rows && terminal.column <= terminal.columns);
    }
    #[test]
    fn dimensions_and_extreme_pointer_events_are_clamped() {
        assert!(Desktop::new(639, 480).is_none());
        assert!(Desktop::new(u32::MAX, 480).is_none());
        let mut desktop = Desktop::new(1024, 768).unwrap();
        desktop.pointer(i32::MAX, i32::MIN, 0);
        assert_eq!((desktop.pointer_x, desktop.pointer_y), (1012, 0));
        desktop.maximize();
        assert!(desktop.window.fits(1024, 768));
        desktop.maximize();
        assert_eq!(desktop.window.x, 32);
        assert!(desktop.key(display::KEY_F2));
        assert_eq!(desktop.app, App::Files);
        assert!(desktop.key(27));
        assert!(!desktop.visible);
        desktop.key(display::KEY_F1);
        assert!(desktop.visible);
    }
    #[test]
    fn clicking_taskbar_and_dragging_never_escapes_work_area() {
        let mut desktop = Desktop::new(800, 600).unwrap();
        desktop.pointer_x = 180;
        desktop.pointer_y = 575;
        desktop.pointer(0, 0, 1);
        assert_eq!(desktop.app, App::Files);
        desktop.pointer(0, 0, 0);
        desktop.pointer_x = 60;
        desktop.pointer_y = 65;
        desktop.pointer(0, 0, 1);
        desktop.pointer(-1000, -1000, 1);
        assert_eq!((desktop.window.x, desktop.window.y), (0, 40));
        desktop.pointer(0, 0, 0);
        assert!(desktop.window.fits(800, 552));
    }
}
