//! Feature-gated CPL3 transition proof. This is not the Vibrix syscall ABI.
use core::arch::asm;

use super::gdt::Gdt;

pub const PROBE_VECTOR: usize = 0x80;

pub const fn user_frame_selectors_valid(code_segment: u64, stack_segment: u64) -> bool {
    code_segment as u16 == Gdt::USER_CODE_SELECTOR
        && stack_segment as u16 == Gdt::USER_DATA_SELECTOR
}

/// Enter one already-mapped CPL3 instruction stream with interrupts disabled.
///
/// # Safety
/// `user_rip` must be canonical, user-accessible RX memory containing valid
/// instructions, and `user_rsp` must be the aligned exclusive end of live
/// user-accessible RW stack storage. GDT/TSS/IDT must be installed and the
/// feature-gated probe vector must be DPL3. This function never returns.
pub unsafe fn enter(user_rip: u64, user_rsp: u64) -> ! {
    let user_ss = u64::from(Gdt::USER_DATA_SELECTOR);
    let user_cs = u64::from(Gdt::USER_CODE_SELECTOR);
    // Bit 1 is architecturally fixed. IF stays clear so the diagnostic probe
    // cannot be preempted before its deliberate software interrupt.
    let user_rflags = 0x2u64;
    // SAFETY: caller establishes every IRETQ target/selector/stack invariant.
    // Pushes construct the hardware privilege-return frame in SS,RSP,RFLAGS,
    // CS,RIP order. IRETQ changes CPL from 0 to 3.
    unsafe {
        asm!(
            "cli",
            "push {user_ss}",
            "push {user_rsp}",
            "push {user_rflags}",
            "push {user_cs}",
            "push {user_rip}",
            "iretq",
            user_ss = in(reg) user_ss,
            user_rsp = in(reg) user_rsp,
            user_rflags = in(reg) user_rflags,
            user_cs = in(reg) user_cs,
            user_rip = in(reg) user_rip,
            options(noreturn)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_vector_stays_outside_exception_and_irq_slots() {
        assert!(PROBE_VECTOR >= 0x20);
        assert_ne!(PROBE_VECTOR, super::super::irq::TIMER_VECTOR as usize);
        assert_ne!(PROBE_VECTOR, super::super::irq::SPURIOUS_VECTOR as usize);
    }

    #[test]
    fn only_exact_user_selectors_are_accepted() {
        assert!(user_frame_selectors_valid(
            u64::from(Gdt::USER_CODE_SELECTOR),
            u64::from(Gdt::USER_DATA_SELECTOR)
        ));
        assert!(!user_frame_selectors_valid(
            u64::from(Gdt::KERNEL_CODE_SELECTOR),
            u64::from(Gdt::USER_DATA_SELECTOR)
        ));
        assert!(!user_frame_selectors_valid(
            u64::from(Gdt::USER_CODE_SELECTOR),
            u64::from(Gdt::KERNEL_DATA_SELECTOR)
        ));
    }
}
