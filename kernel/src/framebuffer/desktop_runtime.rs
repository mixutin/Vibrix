//! Foreground bootstrap graphics owner and checked syscall copy boundary.
use super::{BootInfo, Display, abi};
use crate::arch::x86_64::syscall::abi::{self as syscall, Errno, Syscall};
use crate::framebuffer::root::{RootGuard, current_root, interrupts_enabled};
use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};

struct Slot(UnsafeCell<Option<Display>>);
// SAFETY: BSP only; pre-STI initialization then IF-masked syscall access.
// BUSY excludes nested borrows. No IRQ, NMI, panic or AP renders this surface.
unsafe impl Sync for Slot {}
static SLOT: Slot = Slot(UnsafeCell::new(None));
static STARTED: AtomicBool = AtomicBool::new(false);
static BUSY: AtomicBool = AtomicBool::new(false);
static ROOT: AtomicU64 = AtomicU64::new(0);

/// # Safety
/// Called once on the BSP before STI, under the permanent kernel CR3. The
/// framebuffer mapping is retained; the legacy renderer is disabled by cfg.
pub unsafe fn init(info: &BootInfo) -> Result<(), ()> {
    if interrupts_enabled() || STARTED.swap(true, Ordering::SeqCst) {
        return Err(());
    }
    // SAFETY: caller grants sole ownership of the validated retained mapping.
    let display = unsafe { Display::new(info) }?;
    ROOT.store(current_root(), Ordering::Release);
    // SAFETY: one-time initialization, no published userspace execution yet.
    unsafe { *SLOT.0.get() = Some(display) };
    Ok(())
}

fn access<T>(operation: impl FnOnce(&mut Display) -> Result<T, ()>) -> Result<T, Errno> {
    if interrupts_enabled() || BUSY.swap(true, Ordering::SeqCst) {
        return Err(Errno::Busy);
    }
    let root = ROOT.load(Ordering::Acquire);
    let result = if root == 0 {
        Err(Errno::NotSupported)
    } else {
        // SAFETY: only copied higher-half data enters; both roots retain this
        // stack/code. IF=0 throughout; guard restores CR3 before any copyout.
        let _guard = unsafe { RootGuard::enter(root) };
        // SAFETY: BUSY owns the exclusive borrow, single BSP and no IRQ users.
        match unsafe { &mut *SLOT.0.get() } {
            Some(display) => operation(display).map_err(|_| Errno::InvalidArgument),
            None => Err(Errno::NotSupported),
        }
    };
    BUSY.store(false, Ordering::SeqCst);
    result
}

fn rectangle(args: &[u64; 6]) -> Result<abi::Rect, Errno> {
    Ok(abi::Rect {
        x: u32::try_from(args[0]).map_err(|_| Errno::InvalidArgument)?,
        y: u32::try_from(args[1]).map_err(|_| Errno::InvalidArgument)?,
        width: u32::try_from(args[2]).map_err(|_| Errno::InvalidArgument)?,
        height: u32::try_from(args[3]).map_err(|_| Errno::InvalidArgument)?,
    })
}

fn execute(call: Syscall, args: [u64; 6]) -> Result<u64, Errno> {
    match call {
        Syscall::DisplayInfo => {
            if args[1..].iter().any(|&v| v != 0) {
                return Err(Errno::InvalidArgument);
            }
            let mut info = access(|display| Ok(display.info()))?;
            if crate::desktop_input::pointer_ready() {
                info.capabilities |= abi::POINTER;
            }
            let words = [
                info.version,
                info.width,
                info.height,
                info.format,
                info.max_blit_pixels,
                info.capabilities,
            ];
            let mut bytes = [0u8; 24];
            for (out, value) in bytes.as_chunks_mut::<4>().0.iter_mut().zip(words) {
                out.copy_from_slice(&value.to_le_bytes());
            }
            crate::memory::address_space::copy_to_user(args[0], &bytes)
                .map_err(|_| Errno::BadAddress)?;
            Ok(0)
        }
        Syscall::DisplayFill => {
            if args[4] > 0x00ff_ffff || args[5] != 0 {
                return Err(Errno::InvalidArgument);
            }
            let rect = rectangle(&args)?;
            access(|display| display.fill(rect, args[4] as u32))?;
            Ok(0)
        }
        Syscall::DisplayBlit => {
            let rect = rectangle(&args)?;
            let count = usize::try_from(args[5]).map_err(|_| Errno::InvalidArgument)?;
            if count == 0 || count > abi::MAX_BLIT_PIXELS || rect.pixels() != Some(count) {
                return Err(Errno::InvalidArgument);
            }
            access(|display| display.validate(rect))?;
            let mut pixels = [0u32; abi::MAX_BLIT_PIXELS];
            // SAFETY: initialized u32 array has no padding, every byte pattern
            // is valid. The byte view stays within count checked above.
            let bytes = unsafe {
                core::slice::from_raw_parts_mut(pixels.as_mut_ptr().cast::<u8>(), count * 4)
            };
            crate::memory::address_space::copy_from_user(args[4], bytes)
                .map_err(|_| Errno::BadAddress)?;
            access(|display| display.blit(rect, &pixels[..count]))?;
            Ok(0)
        }
        Syscall::InputPoll => {
            if args[1..].iter().any(|&v| v != 0) {
                return Err(Errno::InvalidArgument);
            }
            // Validate the entire writable output before consuming hardware.
            // No concurrent unmap/scheduler exists between these two copies.
            crate::memory::address_space::copy_to_user(args[0], &[0; 16])
                .map_err(|_| Errno::BadAddress)?;
            let Some(event) = crate::desktop_input::poll() else {
                return Ok(0);
            };
            let words = [event.kind, event.code, event.x as u32, event.y as u32];
            let mut bytes = [0u8; 16];
            for (out, value) in bytes.as_chunks_mut::<4>().0.iter_mut().zip(words) {
                out.copy_from_slice(&value.to_le_bytes());
            }
            crate::memory::address_space::copy_to_user(args[0], &bytes)
                .map_err(|_| Errno::BadAddress)?;
            Ok(1)
        }
        _ => Err(Errno::NotSupported),
    }
}

pub fn dispatch(number: u64, args: [u64; 6]) -> Option<u64> {
    let call = Syscall::from_number(number)?;
    if !matches!(
        call,
        Syscall::DisplayInfo | Syscall::DisplayFill | Syscall::DisplayBlit | Syscall::InputPoll
    ) {
        return None;
    }
    Some(match execute(call, args) {
        Ok(value) => value,
        Err(error) => syscall::encode_error(error),
    })
}
