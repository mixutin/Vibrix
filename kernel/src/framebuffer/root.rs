//! Retained-kernel-CR3 access, shared by exclusive framebuffer owners.
use core::arch::asm;

pub(super) fn interrupts_enabled() -> bool {
    let flags: u64;
    // SAFETY: read-only RFLAGS inspection, no device or memory mutation.
    unsafe { asm!("pushfq", "pop {}", out(reg) flags, options(preserves_flags)) };
    flags & (1 << 9) != 0
}

pub(super) fn current_root() -> u64 {
    let root: u64;
    // SAFETY: read-only privileged register inspection from CPL0.
    unsafe { asm!("mov {}, cr3", out(reg) root, options(nomem, nostack, preserves_flags)) };
    root
}

pub(super) struct RootGuard {
    previous: u64,
    changed: bool,
}

impl RootGuard {
    /// # Safety
    /// Both roots retain the current higher-half stack/code/data. The target
    /// is the boot kernel root captured before STI and is never reclaimed.
    /// IF remains clear, no AP exists, and no userspace pointers are used while
    /// this guard is live. The guard must be dropped before returning to CPL3.
    pub(super) unsafe fn enter(kernel_root: u64) -> Self {
        let previous = current_root();
        let changed = previous != kernel_root;
        if changed {
            // SAFETY: caller guarantees the retained root and shared kernel
            // stack/code. Do not use nomem: CR3 changes address translation.
            unsafe { asm!("mov cr3, {}", in(reg) kernel_root, options(nostack, preserves_flags)) };
        }
        Self { previous, changed }
    }
}

impl Drop for RootGuard {
    fn drop(&mut self) {
        if self.changed {
            // SAFETY: this is the exact root active at guard entry, still
            // retained. No interrupts, scheduling or user-memory access occurs
            // between switches. Restore it even after a rendering error.
            unsafe {
                asm!("mov cr3, {}", in(reg) self.previous, options(nostack, preserves_flags))
            };
        }
    }
}
