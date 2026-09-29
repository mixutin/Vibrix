#![no_std]

//! Allocation-free Vibrix userspace UI primitives.
//!
//! This crate owns reusable software-rendering policy used by the native
//! desktop. It is intentionally small: checked/clipped fills, bounded ASCII
//! glyph blits, task buttons and a window frame. It is not a compositor,
//! layout engine, Unicode text stack or hardware-accelerated renderer.

#[path = "../../../shared/font.rs"]
mod font;

use vibrix_syscall::{self as syscall, Result, display::Rect};

pub const CELL_WIDTH: u32 = 12;
pub const CELL_HEIGHT: u32 = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Theme {
    pub background: u32,
    pub foreground: u32,
    pub panel: u32,
    pub accent: u32,
    pub muted: u32,
    pub shadow: u32,
}

impl Theme {
    pub const VIBRIX: Self = Self {
        background: 0x0010_1925,
        foreground: 0x00db_e7f3,
        panel: 0x0022_3750,
        accent: 0x005e_a5ed,
        muted: 0x0087_a5be,
        shadow: 0x0003_0810,
    };
}

pub trait Canvas {
    fn fill(&mut self, rect: Rect, color: u32) -> Result<()>;
    fn blit(&mut self, rect: Rect, pixels: &[u32]) -> Result<()>;
}

pub struct NativeCanvas;

impl Canvas for NativeCanvas {
    fn fill(&mut self, rect: Rect, color: u32) -> Result<()> {
        syscall::display_fill(rect, color)
    }

    fn blit(&mut self, rect: Rect, pixels: &[u32]) -> Result<()> {
        syscall::display_blit(rect, pixels)
    }
}

pub struct Painter<'a, C> {
    canvas: &'a mut C,
    width: u32,
    height: u32,
}

impl<'a, C: Canvas> Painter<'a, C> {
    pub fn new(canvas: &'a mut C, width: u32, height: u32) -> Self {
        Self {
            canvas,
            width,
            height,
        }
    }

    pub const fn width(&self) -> u32 {
        self.width
    }

    pub const fn height(&self) -> u32 {
        self.height
    }

    pub fn fill(
        &mut self,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        color: u32,
    ) -> Result<()> {
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

    pub fn text(
        &mut self,
        x: u32,
        y: u32,
        bytes: &[u8],
        colors: (u32, u32),
        limit: u32,
    ) -> Result<()> {
        if y.saturating_add(CELL_HEIGHT) > self.height || x >= self.width {
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
                    height: CELL_HEIGHT,
                },
                &pixels[..width * CELL_HEIGHT as usize],
            )?;
        }
        Ok(())
    }
}

pub fn task_button(height: u32, index: usize) -> Rect {
    Rect {
        x: 12 + index as u32 * 154,
        y: height.saturating_sub(38),
        width: 146,
        height: 28,
    }
}

pub fn paint_button<C: Canvas>(
    painter: &mut Painter<'_, C>,
    rect: Rect,
    label: &[u8],
    selected: bool,
    theme: Theme,
) -> Result<()> {
    let background = if selected {
        0x0034_597c
    } else {
        theme.background
    };
    painter.fill(rect.x, rect.y, rect.width, rect.height, background)?;
    painter.text(
        rect.x + 6,
        rect.y + 6,
        label,
        (theme.foreground, background),
        rect.width.saturating_sub(12),
    )
}

pub fn paint_window_frame<C: Canvas>(
    painter: &mut Painter<'_, C>,
    rect: Rect,
    title: &[u8],
    theme: Theme,
) -> Result<()> {
    if rect.width < 4 || rect.height < 36 {
        return Ok(());
    }
    painter.fill(
        rect.x + 6,
        rect.y + 6,
        rect.width,
        rect.height,
        theme.shadow,
    )?;
    painter.fill(rect.x, rect.y, rect.width, rect.height, theme.accent)?;
    painter.fill(rect.x + 1, rect.y + 1, rect.width - 2, 31, theme.panel)?;
    painter.fill(
        rect.x + 1,
        rect.y + 32,
        rect.width - 2,
        rect.height - 33,
        theme.background,
    )?;
    painter.text(
        rect.x + 12,
        rect.y + 9,
        title,
        (theme.foreground, theme.panel),
        rect.width.saturating_sub(120),
    )?;
    painter.text(
        rect.x + rect.width.saturating_sub(94),
        rect.y + 9,
        b"_  +  x",
        (theme.foreground, theme.panel),
        88,
    )
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;

    struct Checked {
        width: u32,
        height: u32,
        fills: usize,
        blits: usize,
    }

    impl Canvas for Checked {
        fn fill(&mut self, rect: Rect, _: u32) -> Result<()> {
            assert!(rect.fits(self.width, self.height));
            self.fills += 1;
            Ok(())
        }

        fn blit(&mut self, rect: Rect, pixels: &[u32]) -> Result<()> {
            assert!(rect.fits(self.width, self.height));
            assert_eq!(rect.pixels(), Some(pixels.len()));
            assert!(pixels.len() <= 960);
            self.blits += 1;
            Ok(())
        }
    }

    #[test]
    fn painter_clips_fills_and_bounds_text_tiles() {
        let mut canvas = Checked {
            width: 640,
            height: 480,
            fills: 0,
            blits: 0,
        };
        let mut painter = Painter::new(&mut canvas, 640, 480);
        painter.fill(630, 470, 100, 100, 0).unwrap();
        painter
            .text(620, 460, b"bounded text", (1, 0), 200)
            .unwrap();
        assert_eq!(canvas.fills, 1);
        assert!(canvas.blits <= 1);
    }

    #[test]
    fn task_buttons_are_stable_and_non_overlapping() {
        for index in 0..4 {
            let rect = task_button(600, index);
            assert!(rect.fits(800, 600));
            if index != 0 {
                let previous = task_button(600, index - 1);
                assert!(previous.x + previous.width <= rect.x);
            }
        }
    }

    #[test]
    fn button_and_window_widgets_emit_checked_operations() {
        let mut canvas = Checked {
            width: 1024,
            height: 768,
            fills: 0,
            blits: 0,
        };
        let mut painter = Painter::new(&mut canvas, 1024, 768);
        paint_button(
            &mut painter,
            task_button(768, 0),
            b"F1 Terminal",
            true,
            Theme::VIBRIX,
        )
        .unwrap();
        paint_window_frame(
            &mut painter,
            Rect {
                x: 32,
                y: 56,
                width: 800,
                height: 560,
            },
            b"Terminal",
            Theme::VIBRIX,
        )
        .unwrap();
        assert!(canvas.fills >= 5);
        assert!(canvas.blits >= 3);
    }
}
