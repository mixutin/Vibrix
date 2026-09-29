//! Checked copy-based graphics surface. Userspace never receives GOP MMIO.
use super::{BootInfo, PixelSurface};
#[allow(dead_code)]
#[path = "../../../shared/display_abi.rs"]
pub mod abi;

pub struct Display {
    pixels: PixelSurface,
    rgb_bytes: bool,
}

impl Display {
    /// # Safety
    /// The validated GOP mapping remains live, supervisor-only and exclusively
    /// writable by this owner under the retained kernel root until shutdown.
    pub unsafe fn new(info: &BootInfo) -> Result<Self, ()> {
        let pixels = PixelSurface::new(info)?;
        if info.framebuffer_format > 1 || pixels.width > 4096 || pixels.height > 2160 {
            return Err(());
        }
        Ok(Self {
            pixels,
            rgb_bytes: info.framebuffer_format == 0,
        })
    }

    pub fn info(&self) -> abi::DisplayInfo {
        abi::DisplayInfo {
            version: abi::VERSION,
            width: self.pixels.width as u32,
            height: self.pixels.height as u32,
            format: abi::XRGB8888,
            max_blit_pixels: abi::MAX_BLIT_PIXELS as u32,
            capabilities: abi::KEYBOARD,
        }
    }

    fn validate(&self, rect: abi::Rect) -> Result<(), ()> {
        if rect.fits(self.pixels.width as u32, self.pixels.height as u32) {
            Ok(())
        } else {
            Err(())
        }
    }

    fn color(&self, color: u32) -> u32 {
        let rgb = color & 0x00ff_ffff;
        if self.rgb_bytes {
            (rgb & 0xff00) | ((rgb & 0xff) << 16) | (rgb >> 16)
        } else {
            rgb
        }
    }

    pub fn fill(&mut self, rect: abi::Rect, color: u32) -> Result<(), ()> {
        self.validate(rect)?;
        let color = self.color(color);
        for y in rect.y..rect.y + rect.height {
            #[cfg(not(test))]
            if y & 7 == 0 {
                crate::desktop_input::pump();
            }
            for x in rect.x..rect.x + rect.width {
                // SAFETY: exclusive lifetime from new(); rectangle prevalidated
                // in full before the first volatile write. No user pointer.
                unsafe { self.pixels.pixel(x as usize, y as usize, color)? };
            }
        }
        Ok(())
    }

    pub fn blit(&mut self, rect: abi::Rect, pixels: &[u32]) -> Result<(), ()> {
        self.validate(rect)?;
        if rect.pixels() != Some(pixels.len()) || pixels.len() > abi::MAX_BLIT_PIXELS {
            return Err(());
        }
        #[cfg(not(test))]
        crate::desktop_input::pump();
        for (index, &pixel) in pixels.iter().enumerate() {
            let x = rect.x as usize + index % rect.width as usize;
            let y = rect.y as usize + index / rect.width as usize;
            // SAFETY: same sole-owner mapping invariant; source is a checked
            // kernel copy, never read under a different userspace mapping.
            unsafe { self.pixels.pixel(x, y, self.color(pixel))? };
        }
        Ok(())
    }
}

#[cfg(not(test))]
#[path = "desktop_runtime.rs"]
mod runtime;
#[cfg(not(test))]
pub use runtime::{dispatch, init};

#[cfg(test)]
mod tests {
    use super::super::bootinfo::{FinalMemoryMap, FramebufferInfo};
    use super::*;
    fn info(pixels: &mut [u32], format: u32) -> BootInfo {
        BootInfo::new(
            FramebufferInfo {
                physical_base: pixels.as_mut_ptr() as u64,
                size_bytes: 9 * 6 * 4,
                width: 7,
                height: 6,
                stride: 9,
                pixel_format: format,
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
    fn fill_and_blit_preserve_padding_canaries_and_convert_rgb() {
        for format in [0, 1] {
            let mut backing = vec![0xaabb_ccdd; 9 * 6 + 8];
            let info = info(&mut backing, format);
            // SAFETY: test retains unique access to live aligned buffer.
            let mut display = unsafe { Display::new(&info) }.unwrap();
            display
                .fill(
                    abi::Rect {
                        x: 1,
                        y: 1,
                        width: 3,
                        height: 2,
                    },
                    0x123456,
                )
                .unwrap();
            assert_eq!(backing[10], if format == 0 { 0x563412 } else { 0x123456 });
            display
                .blit(
                    abi::Rect {
                        x: 5,
                        y: 5,
                        width: 2,
                        height: 1,
                    },
                    &[0xff0000, 0x00ff00],
                )
                .unwrap();
            assert_eq!(backing[50], if format == 0 { 0xff } else { 0xff0000 });
            assert_eq!(backing[51], 0xff00);
            assert_eq!(backing[0], 0xaabb_ccdd);
            assert!(backing[54..].iter().all(|&p| p == 0xaabb_ccdd));
            for row in 0..6 {
                assert_eq!(&backing[row * 9 + 7..row * 9 + 9], &[0xaabb_ccdd; 2]);
            }
        }
    }
    #[test]
    fn invalid_requests_do_not_partially_mutate_framebuffer() {
        let mut backing = vec![0xaabb_ccdd; 9 * 6];
        let info = info(&mut backing, 1);
        // SAFETY: live host mock backing, sole renderer.
        let mut display = unsafe { Display::new(&info) }.unwrap();
        for rect in [
            abi::Rect {
                x: 6,
                y: 0,
                width: 2,
                height: 1,
            },
            abi::Rect {
                x: u32::MAX,
                y: 0,
                width: 2,
                height: 1,
            },
            abi::Rect {
                x: 0,
                y: 0,
                width: 0,
                height: 1,
            },
        ] {
            assert!(display.fill(rect, 0).is_err());
            assert!(display.blit(rect, &[0, 0]).is_err());
        }
        assert!(
            display
                .blit(
                    abi::Rect {
                        x: 0,
                        y: 0,
                        width: 2,
                        height: 1
                    },
                    &[0]
                )
                .is_err()
        );
        assert!(backing.iter().all(|&p| p == 0xaabb_ccdd));
    }
    #[test]
    fn metadata_is_checked_before_mapping_use() {
        let mut backing = vec![0; 9 * 6];
        let mut info = info(&mut backing, 2);
        // SAFETY: metadata rejection occurs before any pixel access.
        assert!(unsafe { Display::new(&info) }.is_err());
        info.framebuffer_format = 1;
        info.framebuffer_size = 4;
        assert!(unsafe { Display::new(&info) }.is_err());
    }
}
