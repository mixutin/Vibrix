//! Minimal boot-time framebuffer proof independent of UEFI GOP calls.
//!
//! The loader has already mapped the framebuffer BAR supervisor-writable,
//! NX and uncached in the kernel's active hierarchy (ADR 0006).

#[cfg(not(test))]
use crate::bootinfo::BootInfo;

#[cfg(test)]
#[allow(dead_code)]
#[path = "../../shared/bootinfo.rs"]
mod bootinfo;
#[cfg(test)]
use bootinfo::BootInfo;

const BANNER_X: usize = 16;
const BANNER_Y: usize = 16;
const BANNER_WIDTH: usize = 352;
const BANNER_HEIGHT: usize = 112;
const GREEN: u32 = 0x0000_ff00;
const WHITE: u32 = 0x00ff_ffff;
const DARK: u32 = 0x0010_1517;

/// This is not a font system: only glyphs for two hard-coded boot labels.
/// Rows are five bits wide, most-significant pixel on the left.
fn boot_glyph(ch: u8) -> [u8; 7] {
    match ch {
        b'V' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100],
        b'I' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b11111],
        b'B' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110],
        b'R' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001],
        b'X' => [0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001],
        b'K' => [0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001],
        b'E' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111],
        b'N' => [0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001],
        b'L' => [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111],
        b' ' => [0; 7],
        _ => [0; 7],
    }
}

struct PixelSurface {
    base: usize,
    size: usize,
    width: usize,
    height: usize,
    stride: usize,
}

impl PixelSurface {
    fn new(info: &BootInfo) -> Result<Self, ()> {
        if info.validate().is_err() || !info.framebuffer_base.is_multiple_of(4) {
            return Err(());
        }
        Ok(Self {
            base: usize::try_from(info.framebuffer_base).map_err(|_| ())?,
            size: usize::try_from(info.framebuffer_size).map_err(|_| ())?,
            width: info.framebuffer_width as usize,
            height: info.framebuffer_height as usize,
            stride: info.framebuffer_stride as usize,
        })
    }

    /// # Safety
    /// The caller owns the mapped GOP MMIO range for a single boot CPU.
    unsafe fn pixel(&self, x: usize, y: usize, color: u32) -> Result<(), ()> {
        if x >= self.width || y >= self.height {
            return Err(());
        }
        let offset = y
            .checked_mul(self.stride)
            .and_then(|row| row.checked_add(x))
            .and_then(|pixel| pixel.checked_mul(4))
            .ok_or(())?;
        if offset.checked_add(4).is_none_or(|end| end > self.size) {
            return Err(());
        }
        let address = self.base.checked_add(offset).ok_or(())?;
        // SAFETY: lifetime/mapping is caller's invariant; metadata and
        // checked byte arithmetic bound the volatile access to GOP backing.
        unsafe { core::ptr::write_volatile(address as *mut u32, color) };
        Ok(())
    }
}

/// Draw a tiny self-contained kernel label on already mapped GOP pixels.
///
/// `text` contains only the boot glyphs above; no allocations or firmware
/// calls occur after ExitBootServices.
///
/// # Safety
/// Caller supplies an exclusively writable and valid GOP MMIO mapping.
unsafe fn draw_text(
    pixels: &PixelSurface,
    text: &[u8],
    x: usize,
    y: usize,
    scale: usize,
) -> Result<(), ()> {
    for (character, &ch) in text.iter().enumerate() {
        for (row, bits) in boot_glyph(ch).into_iter().enumerate() {
            for column in 0..5 {
                if bits & (1 << (4 - column)) == 0 {
                    continue;
                }
                for dy in 0..scale {
                    for dx in 0..scale {
                        // SAFETY: bounded glyph slots are inside the
                        // prevalidated banner dimensions and GOP mapping.
                        unsafe {
                            pixels.pixel(
                                x + character * 6 * scale + column * scale + dx,
                                y + row * scale + dy,
                                WHITE,
                            )?
                        };
                    }
                }
            }
        }
    }
    Ok(())
}

/// Draw the existing minimal 48x16 marker, then (on a sufficiently large
/// linear RGB/BGR GOP) a readable "VIBRIX / KERNEL LIVE" status card.
///
/// Returns `Ok(true)` if the legible banner was painted; `Ok(false)`
/// means the old bounded marker was painted on a small/bitmask display.
///
/// No heap, firmware protocol, system call, external font or cache alias.
///
/// # Safety
/// `framebuffer_base` must address the full loader-validated,
/// identity-mapped writable MMIO region, and no other CPU/thread may
/// concurrently access those pixels under the kernel page tables.
pub unsafe fn draw_boot_marker(info: &BootInfo) -> Result<bool, ()> {
    let pixels = PixelSurface::new(info)?;
    let marker_width = pixels.width.min(48);
    let marker_height = pixels.height.min(16);
    let green = if info.framebuffer_format == 2 { 0 } else { GREEN };
    // PixelBitMask's masks are absent from the present BootInfo ABI:
    // zero is unambiguously black; do not invent a white/green bit layout.
    for y in 0..marker_height {
        for x in 0..marker_width {
            // SAFETY: validated complete MMIO backing and a bounded marker.
            unsafe { pixels.pixel(x, y, green)? };
        }
    }

    if info.framebuffer_format == 2
        || pixels.width < BANNER_X + BANNER_WIDTH
        || pixels.height < BANNER_Y + BANNER_HEIGHT
    {
        return Ok(false);
    }

    for y in 0..BANNER_HEIGHT {
        for x in 0..BANNER_WIDTH {
            let border = x < 3 || y < 3 || x >= BANNER_WIDTH - 3 || y >= BANNER_HEIGHT - 3;
            // SAFETY: both coordinates lie inside the validated whole
            // framebuffer; the banner never uses an unchecked raw pointer.
            unsafe {
                pixels.pixel(
                    BANNER_X + x,
                    BANNER_Y + y,
                    if border { GREEN } else { DARK },
                )?
            };
        }
    }
    // Both strings fit inside the bounded banner, including their last
    // 5-pixel glyph: text width = number_of_chars * 6 * scale.
    // SAFETY: the same mapped, owned GOP backing as the border above.
    unsafe {
        draw_text(&pixels, b"VIBRIX", BANNER_X + 68, BANNER_Y + 12, 6)?;
        draw_text(&pixels, b"KERNEL LIVE", BANNER_X + 77, BANNER_Y + 72, 3)?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bootinfo::{FinalMemoryMap, FramebufferInfo};

    fn valid_for_buffer(buffer: &mut [u32]) -> BootInfo {
        BootInfo::new(
            FramebufferInfo {
                physical_base: buffer.as_mut_ptr() as u64,
                size_bytes: (buffer.len() * 4) as u64,
                width: 16,
                height: 16,
                stride: 16,
                pixel_format: 1,
            },
            0x1000,
            FinalMemoryMap {
                physical_base: 0x2000,
                byte_len: 48,
                descriptor_size: 48,
                descriptor_version: 1,
            },
        )
        .unwrap()
    }

    #[test]
    fn marker_writes_only_its_bounded_buffer() {
        let mut framebuffer = vec![0xaabb_ccdd_u32; 16 * 17];
        let info = valid_for_buffer(&mut framebuffer);
        // SAFETY: boxed test buffer is live, writable and identity accessible.
        unsafe { draw_boot_marker(&info) }.unwrap();
        assert!(framebuffer[..16 * 16].iter().all(|&p| p == 0x0000_ff00));
        assert!(framebuffer[16 * 16..].iter().all(|&p| p == 0xaabb_ccdd));
    }

    #[test]
    fn refuses_corrupt_size_before_mmio_access() {
        let mut framebuffer = vec![0xaabb_ccdd_u32; 16 * 16];
        let mut info = valid_for_buffer(&mut framebuffer);
        info.framebuffer_size = 1;
        // SAFETY: invalid metadata is rejected before any write.
        assert!(unsafe { draw_boot_marker(&info) }.is_err());
        assert!(framebuffer.iter().all(|&p| p == 0xaabb_ccdd));
    }
}
