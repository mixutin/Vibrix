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

/// Draw a small green boot marker (or black with firmware's bitmask format).
/// No heap, font, firmware protocol, system call or framebuffer cache alias.
///
/// # Safety
/// `framebuffer_base` must address the full loader-validated, identity-mapped
/// writable MMIO region, and no other CPU/thread may concurrently access
/// those pixels. The caller must have transferred under the kernel page tables.
pub unsafe fn draw_boot_marker(info: &BootInfo) -> Result<(), ()> {
    if info.validate().is_err() || !info.framebuffer_base.is_multiple_of(4) {
        return Err(());
    }
    let width = info.framebuffer_width.min(48) as usize;
    let height = info.framebuffer_height.min(16) as usize;
    let stride = info.framebuffer_stride as usize;
    let size = usize::try_from(info.framebuffer_size).map_err(|_| ())?;
    let base = usize::try_from(info.framebuffer_base).map_err(|_| ())?;
    let green = if info.framebuffer_format == 2 {
        0 // PixelBitMask is not in the initial ABI: zero is universally black.
    } else {
        0x0000_ff00u32 // Green is byte-order agnostic for RGB/BGR 8:8:8.
    };

    for y in 0..height {
        for x in 0..width {
            let offset = y
                .checked_mul(stride)
                .and_then(|row| row.checked_add(x))
                .and_then(|pixel| pixel.checked_mul(4))
                .ok_or(())?;
            let end = offset.checked_add(4).ok_or(())?;
            if end > size {
                return Err(());
            }
            let address = base.checked_add(offset).ok_or(())?;
            // SAFETY: the caller guarantees mapped live MMIO, metadata
            // validation/bounds prove each 4-byte write stays inside it.
            unsafe { core::ptr::write_volatile(address as *mut u32, green) };
        }
    }
    Ok(())
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
            0x3000,
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
