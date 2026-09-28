//! Feature-gated x86-64 SYSCALL/SYSRETQ transport proof.
//!
//! This is the M5 fast-syscall transport boundary, not a process dispatcher.
//! The proof is deliberately single-BSP and uses one dedicated static kernel
//! stack because the SYSCALL instruction does not load TSS.RSP0.

use super::gdt::Gdt;

#[allow(dead_code)]
#[path = "../../../../shared/syscall_abi.rs"]
mod abi;

const IA32_EFER: u32 = 0xc000_0080;
const IA32_STAR: u32 = 0xc000_0081;
const IA32_LSTAR: u32 = 0xc000_0082;
const IA32_FMASK: u32 = 0xc000_0084;
const EFER_SCE: u64 = 1;
const RFLAGS_TF: u64 = 1 << 8;
const RFLAGS_IF: u64 = 1 << 9;
const RFLAGS_DF: u64 = 1 << 10;
const RFLAGS_AC: u64 = 1 << 18;

/// STAR's SYSRET selector base. In 64-bit mode SYSRETQ derives SS as base+8
/// and CS as base+16, then forces both RPLs to 3.
pub const USER_STAR_BASE: u16 = 0x10;

pub const fn star_value() -> u64 {
    ((USER_STAR_BASE as u64) << 48) | ((Gdt::KERNEL_CODE_SELECTOR as u64) << 32)
}

pub const fn fmask_value() -> u64 {
    RFLAGS_TF | RFLAGS_IF | RFLAGS_DF | RFLAGS_AC
}

pub const fn derived_syscall_selectors() -> (u16, u16) {
    (Gdt::KERNEL_CODE_SELECTOR, Gdt::KERNEL_CODE_SELECTOR + 8)
}

pub const fn derived_sysret_selectors() -> (u16, u16) {
    ((USER_STAR_BASE + 16) | 3, (USER_STAR_BASE + 8) | 3)
}

#[cfg(target_os = "none")]
mod native {
    use super::{
        EFER_SCE, Gdt, IA32_EFER, IA32_FMASK, IA32_LSTAR, IA32_STAR, abi,
        derived_syscall_selectors, derived_sysret_selectors, fmask_value, star_value,
    };
    use core::arch::{asm, global_asm, x86_64::__cpuid_count};
    use core::cell::UnsafeCell;
    use core::sync::atomic::{AtomicBool, Ordering};

    const SYSCALL_STACK_BYTES: usize = 16 * 1024;
    const SYSCALL_CPUID_BIT: u32 = 1 << 11;

    #[repr(C, align(16))]
    struct SyscallStack([u8; SYSCALL_STACK_BYTES]);

    struct StaticSyscallStack(UnsafeCell<SyscallStack>);

    // SAFETY: the feature-gated transport is single-BSP only. No AP or nested
    // syscall may share the stack until a synchronized per-CPU design exists.
    unsafe impl Sync for StaticSyscallStack {}

    static SYSCALL_STACK: StaticSyscallStack =
        StaticSyscallStack(UnsafeCell::new(SyscallStack([0; SYSCALL_STACK_BYTES])));
    static INITIALIZED: AtomicBool = AtomicBool::new(false);
    static PROBE_SEEN: AtomicBool = AtomicBool::new(false);

    static mut SYSCALL_KERNEL_RSP: u64 = 0;
    static mut SYSCALL_USER_RSP: u64 = 0;

    global_asm!(
        r#"
        .global vibrix_syscall_entry
        vibrix_syscall_entry:
            mov qword ptr [rip + {user_rsp}], rsp
            mov rsp, qword ptr [rip + {kernel_rsp}]

            push r11
            push rcx
            push rdi
            push rsi
            push rdx
            push r10
            push r8
            push r9

            mov rdi, rax
            call {handler}

            pop r9
            pop r8
            pop r10
            pop rdx
            pop rsi
            pop rdi
            pop rcx
            pop r11

            mov rsp, qword ptr [rip + {user_rsp}]
            sysretq
        "#,
        user_rsp = sym SYSCALL_USER_RSP,
        kernel_rsp = sym SYSCALL_KERNEL_RSP,
        handler = sym syscall_probe_handler,
    );

    unsafe extern "C" {
        fn vibrix_syscall_entry();
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum InitError {
        AlreadyInitialized,
        Unsupported,
        InvalidSelectors,
        VerificationFailed,
    }

    fn stack_bounds() -> (u64, u64) {
        let base = SYSCALL_STACK.0.get() as u64;
        (base, base + SYSCALL_STACK_BYTES as u64)
    }

    fn stack_contains(rsp: u64) -> bool {
        let (base, top) = stack_bounds();
        (base..top).contains(&rsp)
    }

    unsafe fn read_msr(msr: u32) -> u64 {
        let low: u32;
        let high: u32;
        // SAFETY: caller runs at CPL0 on x86-64 and supplies an architectural
        // MSR number supported by the verified SYSCALL capability.
        unsafe {
            asm!(
                "rdmsr",
                in("ecx") msr,
                out("eax") low,
                out("edx") high,
                options(nomem, nostack)
            )
        };
        (u64::from(high) << 32) | u64::from(low)
    }

    unsafe fn write_msr(msr: u32, value: u64) {
        // SAFETY: caller establishes CPL0 and the exact architectural MSR.
        unsafe {
            asm!(
                "wrmsr",
                in("ecx") msr,
                in("eax") value as u32,
                in("edx") (value >> 32) as u32,
                options(nomem, nostack)
            )
        };
    }

    fn syscall_supported() -> bool {
        let max_extended = __cpuid_count(0x8000_0000, 0).eax;
        if max_extended < 0x8000_0001 {
            return false;
        }
        // AMD64 architectural SYSCALL/SYSRET capability bit.
        __cpuid_count(0x8000_0001, 0).edx & SYSCALL_CPUID_BIT != 0
    }

    /// Install the bounded single-BSP fast-syscall entry contract.
    ///
    /// # Safety
    /// Call once on the boot CPU after the permanent GDT is loaded and before
    /// entering CPL3. No other CPU may mutate these MSRs or use this stack.
    pub unsafe fn init() -> Result<(), InitError> {
        if INITIALIZED.swap(true, Ordering::SeqCst) {
            return Err(InitError::AlreadyInitialized);
        }
        if !syscall_supported() {
            INITIALIZED.store(false, Ordering::SeqCst);
            return Err(InitError::Unsupported);
        }

        let (kernel_cs, kernel_ss) = derived_syscall_selectors();
        let (user_cs, user_ss) = derived_sysret_selectors();
        if kernel_cs != Gdt::KERNEL_CODE_SELECTOR
            || kernel_ss != Gdt::KERNEL_DATA_SELECTOR
            || user_cs != Gdt::USER_CODE_SELECTOR
            || user_ss != Gdt::USER_DATA_SELECTOR
        {
            INITIALIZED.store(false, Ordering::SeqCst);
            return Err(InitError::InvalidSelectors);
        }

        let (_, stack_top) = stack_bounds();
        if stack_top == 0 || stack_top & 0xf != 0 {
            INITIALIZED.store(false, Ordering::SeqCst);
            return Err(InitError::VerificationFailed);
        }

        // SAFETY: this feature has sole ownership of both static words before
        // SCE is published. No syscall can execute until EFER.SCE is set last.
        unsafe {
            core::ptr::addr_of_mut!(SYSCALL_KERNEL_RSP).write(stack_top);
            core::ptr::addr_of_mut!(SYSCALL_USER_RSP).write(0);
        }

        let entry = vibrix_syscall_entry as *const () as u64;
        // SAFETY: CPL0, supported architectural MSRs, and valid permanent
        // kernel entry/stack addresses. Publish SCE only after STAR/LSTAR/FMASK.
        let original_efer = unsafe { read_msr(IA32_EFER) };
        unsafe {
            write_msr(IA32_STAR, star_value());
            write_msr(IA32_LSTAR, entry);
            write_msr(IA32_FMASK, fmask_value());
            write_msr(IA32_EFER, original_efer | EFER_SCE);
        }

        // SAFETY: same CPL0 architectural MSRs; reads only verify publication.
        let verified = unsafe {
            read_msr(IA32_STAR) == star_value()
                && read_msr(IA32_LSTAR) == entry
                && read_msr(IA32_FMASK) == fmask_value()
                && read_msr(IA32_EFER) & EFER_SCE != 0
        };
        if !verified {
            // SAFETY: retract SCE on verification failure so no partial entry
            // contract remains callable.
            unsafe { write_msr(IA32_EFER, original_efer) };
            INITIALIZED.store(false, Ordering::SeqCst);
            return Err(InitError::VerificationFailed);
        }

        Ok(())
    }

    extern "C" fn syscall_probe_handler(number: u64) -> u64 {
        let kernel_rsp: u64;
        // SAFETY: read-only inspection of the current CPL0 stack pointer.
        unsafe {
            asm!(
                "mov {}, rsp",
                out(reg) kernel_rsp,
                options(nomem, nostack, preserves_flags)
            )
        };

        let result = abi::encode_error(abi::Errno::NotSupported);
        if number == u64::MAX && stack_contains(kernel_rsp) {
            PROBE_SEEN.store(true, Ordering::SeqCst);
            crate::debugcon::write("VIBRIX: kernel SYSCALL entry reached\r\n");
            crate::println!(
                "kernel syscall probe: number={:#x} kernel_rsp={:#x} result={:#x}",
                number,
                kernel_rsp,
                result
            );
        } else {
            crate::debugcon::write("VIBRIX: kernel SYSCALL entry validation failed\r\n");
        }
        result
    }

    pub fn probe_observed() -> bool {
        PROBE_SEEN.load(Ordering::SeqCst)
    }
}

#[cfg(target_os = "none")]
pub use native::{init, probe_observed};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn star_selectors_match_gdt_and_sysret_rules() {
        let (kernel_cs, kernel_ss) = derived_syscall_selectors();
        let (user_cs, user_ss) = derived_sysret_selectors();
        assert_eq!(kernel_cs, Gdt::KERNEL_CODE_SELECTOR);
        assert_eq!(kernel_ss, Gdt::KERNEL_DATA_SELECTOR);
        assert_eq!(user_cs, Gdt::USER_CODE_SELECTOR);
        assert_eq!(user_ss, Gdt::USER_DATA_SELECTOR);
        assert_eq!(user_cs, 0x23);
        assert_eq!(user_ss, 0x1b);
        assert_eq!((star_value() >> 32) as u16, Gdt::KERNEL_CODE_SELECTOR);
        assert_eq!((star_value() >> 48) as u16, USER_STAR_BASE);
    }

    #[test]
    fn fmask_blocks_unsafe_entry_flags() {
        assert_ne!(fmask_value() & RFLAGS_IF, 0);
        assert_ne!(fmask_value() & RFLAGS_TF, 0);
        assert_ne!(fmask_value() & RFLAGS_DF, 0);
        assert_ne!(fmask_value() & RFLAGS_AC, 0);
    }

    #[test]
    fn probe_result_uses_abi_not_supported_encoding() {
        assert_eq!(
            abi::decode_result(abi::encode_error(abi::Errno::NotSupported)),
            Err(abi::Errno::NotSupported.code())
        );
    }
}
