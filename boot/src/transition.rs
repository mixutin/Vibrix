//! Final x86-64 UEFI-to-higher-half-kernel transition.
//!
//! The software-verified PML4 maps the loader PE image, a 16-page dedicated
//! stack, BootInfo v2 and memory map through the CR3 switch. No firmware
//! services may be called after successful ExitBootServices.

use core::arch::{asm, x86_64::__cpuid_count};

use crate::uefi::{EFI_LOAD_ERROR, Status};

const EFER_MSR: u32 = 0xc000_0080;
const EFER_NXE: u32 = 1 << 11;
const CR4_LA57: u64 = 1 << 12;

#[derive(Clone, Copy)]
pub struct PhysicalRegion {
    pub physical_base: u64,
    pub byte_len: u64,
}

/// Fail closed before exiting firmware if NX and 4-level paging are missing,
/// or if a reserved handoff region exceeds the CPU physical-address width.
pub fn preflight(regions: &[PhysicalRegion]) -> Result<(), Status> {
    let extended = __cpuid_count(0x8000_0000, 0).eax;
    if extended < 0x8000_0001
        || __cpuid_count(0x8000_0001, 0).edx & (1 << 20) == 0
    {
        return Err(EFI_LOAD_ERROR);
    }
    let physical_bits = if extended >= 0x8000_0008 {
        (__cpuid_count(0x8000_0008, 0).eax & 0xff) as u8
    } else {
        36
    };
    let cr4: u64;
    // SAFETY: CPL0 UEFI x86-64 boot environment allows reading CR4.
    unsafe { asm!("mov {}, cr4", out(reg) cr4, options(nomem, nostack, preserves_flags)) };
    if cr4 & CR4_LA57 != 0 || !valid_regions(regions, physical_bits) {
        return Err(EFI_LOAD_ERROR);
    }
    Ok(())
}

fn valid_regions(regions: &[PhysicalRegion], physical_bits: u8) -> bool {
    if !(36..=52).contains(&physical_bits) {
        return false;
    }
    let limit = 1u64 << physical_bits;
    regions.iter().all(|region| {
        region.physical_base != 0
            && region.byte_len != 0
            && region
                .physical_base
                .checked_add(region.byte_len)
                .is_some_and(|end| end <= limit)
    })
}

/// Enable NX, replace CR3, switch to the dedicated mapped stack and jump
/// directly to the kernel's linked higher-half entry using SysV x86-64 ABI.
///
/// # Safety
/// Caller has observed successful ExitBootServices and may no longer call
/// firmware. The verified PML4 must map this very PE code until the JMP and
/// map the full 16-page stack and aligned BootInfo page at their identity
/// addresses; `entry` must be the staged ELF executable entry at its linked
/// virtual address. No Rust execution may occur on the old stack after
/// replacing CR3. The kernel entry never returns.
pub unsafe fn enter_kernel(
    root: u64,
    stack_top: u64,
    bootinfo_virtual: u64,
    entry: u64,
) -> ! {
    let efer_lo: u32;
    let efer_hi: u32;
    // SAFETY: CPU NX support and 4-level paging were checked before firmware
    // exit. Changing EFER.NXE is permitted after EBS; preserve all other bits.
    unsafe {
        asm!(
            "rdmsr",
            in("ecx") EFER_MSR,
            out("eax") efer_lo,
            out("edx") efer_hi,
            options(nostack, preserves_flags)
        );
        asm!(
            "wrmsr",
            in("ecx") EFER_MSR,
            in("eax") efer_lo | EFER_NXE,
            in("edx") efer_hi,
            options(nostack, preserves_flags)
        );
        // The transition path is identity mapped. After MOV CR3, no Rust
        // instruction or old-stack access is emitted before replacing RSP.
        // Simulate a call's 8-byte return slot so SysV entry has RSP % 16 = 8.
        asm!(
            "cli",
            "mov cr3, {pml4}",
            "mov rsp, {stack}",
            "sub rsp, 8",
            "mov qword ptr [rsp], 0",
            "xor rbp, rbp",
            "mov rdi, {boot}",
            "jmp {kernel}",
            pml4 = in(reg) root,
            stack = in(reg) stack_top,
            boot = in(reg) bootinfo_virtual,
            kernel = in(reg) entry,
            options(noreturn)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unrepresentable_handoff_ranges() {
        assert!(valid_regions(
            &[PhysicalRegion {
                physical_base: 4096,
                byte_len: 4096
            }],
            36
        ));
        for (base, len) in [
            (0, 4096),
            (4096, 0),
            (u64::MAX - 1, 4096),
            ((1u64 << 36) - 4096, 8192),
        ] {
            assert!(!valid_regions(
                &[PhysicalRegion {
                    physical_base: base,
                    byte_len: len
                }],
                36
            ));
        }
        assert!(!valid_regions(&[], 35));
        assert!(!valid_regions(&[], 53));
        assert!(valid_regions(&[], 52));
    }
}
