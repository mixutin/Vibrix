//! Single-BSP ownership and page-table boundary for the TTY framebuffer.

use super::{BootInfo, Terminal};
use crate::framebuffer::root::{RootGuard, current_root, interrupts_enabled};
use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};

struct Console(UnsafeCell<Option<Terminal>>);
// SAFETY: initialization and rendering are exclusively serialized by BUSY.
// Only the BSP is started; IRQ/NMI/panic handlers never call this renderer.
unsafe impl Sync for Console {}

static CONSOLE: Console = Console(UnsafeCell::new(None));
static BUSY: AtomicBool = AtomicBool::new(false);
static STARTED: AtomicBool = AtomicBool::new(false);
static KERNEL_ROOT: AtomicU64 = AtomicU64::new(0);

/// # Safety
/// Called once on the BSP with IF=0 under the boot kernel CR3. The validated
/// GOP mapping and higher-half kernel mappings must remain retained. No later
/// renderer may call draw_boot_marker after ownership is published here.
pub unsafe fn init(info: &BootInfo) -> Result<(), ()> {
    if interrupts_enabled() || STARTED.swap(true, Ordering::SeqCst) {
        return Err(());
    }
    BUSY.store(true, Ordering::SeqCst);
    let kernel_root = current_root();
    // SAFETY: caller supplies the permanent MMIO lifetime/ownership.
    let terminal = unsafe { Terminal::new(info) };
    let success = terminal.is_ok();
    // SAFETY: pre-STI initialization exclusively owns BUSY.
    unsafe { *CONSOLE.0.get() = terminal.ok() };
    if success {
        KERNEL_ROOT.store(kernel_root, Ordering::Release);
    }
    BUSY.store(false, Ordering::SeqCst);
    if success { Ok(()) } else { Err(()) }
}

/// Render only already-copied kernel-owned TTY bytes. The private userspace
/// root replaces PML4 slot zero, including the low GOP identity mapping.
/// Temporarily use the retained kernel root rather than exposing MMIO to the
/// user process; restore the exact incoming CR3 before the syscall continues.
pub fn write(bytes: &[u8]) {
    if interrupts_enabled() || BUSY.swap(true, Ordering::SeqCst) {
        return;
    }
    let kernel_root = KERNEL_ROOT.load(Ordering::Acquire);
    if kernel_root == 0 {
        BUSY.store(false, Ordering::SeqCst);
        return;
    }
    // SAFETY: single-BSP IF=0 syscall-side call; bytes, code and stack live in
    // shared higher-half kernel mappings. No user pointers cross this scope.
    let root = unsafe { RootGuard::enter(kernel_root) };
    {
        // SAFETY: BUSY owns the only borrower; the retained kernel root makes
        // the original supervisor-only GOP backing accessible for this scope.
        let slot = unsafe { &mut *CONSOLE.0.get() };
        if let Some(terminal) = slot.as_mut()
            && terminal.write(bytes).is_err()
        {
            *slot = None;
        }
    }
    drop(root);
    BUSY.store(false, Ordering::SeqCst);
}
