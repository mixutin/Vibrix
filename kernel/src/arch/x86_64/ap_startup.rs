//! QEMU-proven xAPIC application-processor startup foundation.
//!
//! The BSP copies a position-independent real-mode trampoline into the
//! loader-owned BootInfo v4 SIPI page, patches the current low CR3 and one AP's
//! dedicated stack, then starts enabled xAPIC processors one at a time.
//! APs bind their firmware identity in the existing per-CPU table and park
//! with IF=0. Scheduling, TSS/IDT setup and concurrent device access remain
//! later SMP milestones.

use core::arch::{asm, global_asm, x86_64::__cpuid_count};
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU32, Ordering};

use vibrix_kernel::cpu_topology::{Availability, MAX_PROCESSORS};
use vibrix_kernel::per_cpu;

use crate::BootInfo;

const PAGE_BYTES: usize = 4096;
const AP_STACK_BYTES: usize = 16 * 1024;
const STARTUP_WAIT_SPINS: usize = 5_000_000;

#[repr(C, align(16))]
struct StackBank(UnsafeCell<[[u8; AP_STACK_BYTES]; MAX_PROCESSORS]>);

// SAFETY: the BSP assigns one disjoint row before starting each AP. An AP owns
// its row forever after startup and only uses it as architectural stack memory.
unsafe impl Sync for StackBank {}

static AP_STACKS: StackBank =
    StackBank(UnsafeCell::new([[0; AP_STACK_BYTES]; MAX_PROCESSORS]));
static AP_BIND_ERROR: AtomicU32 = AtomicU32::new(0);

global_asm!(
    r#"
    .section .text.ap_trampoline,"ax",@progbits
    .code16
    .p2align 4
    .global vibrix_ap_trampoline_start
vibrix_ap_trampoline_start:
    cli
    mov ax, cs
    mov ds, ax
    mov ss, ax

    lgdt [vibrix_ap_gdt_ptr - vibrix_ap_trampoline_start]

    mov eax, cr4
    or eax, 0x20
    mov cr4, eax

    mov eax, dword ptr [vibrix_ap_cr3 - vibrix_ap_trampoline_start]
    mov cr3, eax

    mov ecx, 0xc0000080
    rdmsr
    or eax, 0x900
    wrmsr

    mov eax, cr0
    or eax, 0x80010001
    mov cr0, eax

    .byte 0x66, 0xea
    .global vibrix_ap_far_target
vibrix_ap_far_target:
    .long 0
    .word 0x08

    .code64
    .global vibrix_ap_long_mode
vibrix_ap_long_mode:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    xor ebp, ebp
    mov rsp, qword ptr [rip + vibrix_ap_stack_top]
    mov rax, qword ptr [rip + vibrix_ap_entry]
    jmp rax

    .p2align 3
    .global vibrix_ap_gdt
vibrix_ap_gdt:
    .quad 0x0000000000000000
    .quad 0x00af9a000000ffff
    .quad 0x00cf92000000ffff
vibrix_ap_gdt_end:

    .global vibrix_ap_gdt_ptr
vibrix_ap_gdt_ptr:
    .word vibrix_ap_gdt_end - vibrix_ap_gdt - 1
    .global vibrix_ap_gdt_base
vibrix_ap_gdt_base:
    .long 0

    .p2align 3
    .global vibrix_ap_cr3
vibrix_ap_cr3:
    .long 0
    .long 0

    .global vibrix_ap_stack_top
vibrix_ap_stack_top:
    .quad 0

    .global vibrix_ap_entry
vibrix_ap_entry:
    .quad 0

    .global vibrix_ap_trampoline_end
vibrix_ap_trampoline_end:
    .previous
"#
);

unsafe extern "C" {
    static vibrix_ap_trampoline_start: u8;
    static vibrix_ap_trampoline_end: u8;
    static vibrix_ap_far_target: u8;
    static vibrix_ap_long_mode: u8;
    static vibrix_ap_gdt: u8;
    static vibrix_ap_gdt_base: u8;
    static vibrix_ap_cr3: u8;
    static vibrix_ap_stack_top: u8;
    static vibrix_ap_entry: u8;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    MissingBsp,
    InvalidTrampoline,
    RootAbove4GiB,
    UnsupportedTopology,
    TemplateTooLarge,
    Apic(super::apic::ApicError),
    Bind(u32),
    Timeout(u32),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Summary {
    pub requested: usize,
    pub online: usize,
}

fn template_bounds() -> (usize, usize) {
    (
        core::ptr::addr_of!(vibrix_ap_trampoline_start) as usize,
        core::ptr::addr_of!(vibrix_ap_trampoline_end) as usize,
    )
}

fn template_offset(symbol: *const u8) -> Result<usize, Error> {
    let (start, end) = template_bounds();
    let address = symbol as usize;
    if address < start || address >= end {
        return Err(Error::TemplateTooLarge);
    }
    Ok(address - start)
}

fn stack_top(index: usize) -> Result<u64, Error> {
    if index >= MAX_PROCESSORS {
        return Err(Error::UnsupportedTopology);
    }
    let base = AP_STACKS.0.get().cast::<u8>() as u64;
    base.checked_add(((index + 1) * AP_STACK_BYTES) as u64)
        .ok_or(Error::UnsupportedTopology)
}

fn current_root() -> u64 {
    let cr3: u64;
    // SAFETY: read-only architectural register access at CPL0.
    unsafe {
        asm!(
            "mov {}, cr3",
            out(reg) cr3,
            options(nomem, nostack, preserves_flags)
        )
    };
    cr3 & 0x000f_ffff_ffff_f000
}

unsafe fn patch_u32(base: *mut u8, offset: usize, value: u32) {
    unsafe { base.add(offset).cast::<u32>().write_unaligned(value) };
}

unsafe fn patch_u64(base: *mut u8, offset: usize, value: u64) {
    unsafe { base.add(offset).cast::<u64>().write_unaligned(value) };
}

unsafe fn install_common(info: &BootInfo) -> Result<(*mut u8, u8), Error> {
    let physical = u64::from(info.ap_trampoline_page);
    if physical < PAGE_BYTES as u64
        || physical > 0x000f_f000
        || !physical.is_multiple_of(PAGE_BYTES as u64)
    {
        return Err(Error::InvalidTrampoline);
    }

    let root = current_root();
    let root32 = u32::try_from(root).map_err(|_| Error::RootAbove4GiB)?;
    let (start, end) = template_bounds();
    let length = end.checked_sub(start).ok_or(Error::TemplateTooLarge)?;
    if length == 0 || length > PAGE_BYTES {
        return Err(Error::TemplateTooLarge);
    }

    let destination = physical as usize as *mut u8;
    // SAFETY: BootInfo v4 names the exclusive loader-owned identity-mapped
    // writable/executable page; source is immutable linked kernel text.
    unsafe {
        core::ptr::write_bytes(destination, 0, PAGE_BYTES);
        core::ptr::copy_nonoverlapping(start as *const u8, destination, length);
    }

    let cr3 = template_offset(core::ptr::addr_of!(vibrix_ap_cr3))?;
    let gdt = template_offset(core::ptr::addr_of!(vibrix_ap_gdt))?;
    let gdt_base = template_offset(core::ptr::addr_of!(vibrix_ap_gdt_base))?;
    let far_target = template_offset(core::ptr::addr_of!(vibrix_ap_far_target))?;
    let long_mode = template_offset(core::ptr::addr_of!(vibrix_ap_long_mode))?;
    let entry = template_offset(core::ptr::addr_of!(vibrix_ap_entry))?;

    let gdt_physical = physical
        .checked_add(gdt as u64)
        .ok_or(Error::InvalidTrampoline)?;
    let long_physical = physical
        .checked_add(long_mode as u64)
        .ok_or(Error::InvalidTrampoline)?;
    unsafe {
        patch_u32(destination, cr3, root32);
        patch_u32(
            destination,
            gdt_base,
            u32::try_from(gdt_physical).map_err(|_| Error::InvalidTrampoline)?,
        );
        patch_u32(
            destination,
            far_target,
            u32::try_from(long_physical).map_err(|_| Error::InvalidTrampoline)?,
        );
        patch_u64(destination, entry, vibrix_ap_kernel_entry as *const () as u64);
    }

    let vector = u8::try_from(physical >> 12).map_err(|_| Error::InvalidTrampoline)?;
    if vector == 0 {
        return Err(Error::InvalidTrampoline);
    }
    Ok((destination, vector))
}

#[unsafe(no_mangle)]
extern "C" fn vibrix_ap_kernel_entry() -> ! {
    let apic_id = (__cpuid_count(1, 0).ebx >> 24) & 0xff;
    if per_cpu::bind_ap(apic_id).is_err() {
        AP_BIND_ERROR.store(apic_id.wrapping_add(1), Ordering::Release);
    }

    loop {
        // APs are deliberately parked with IF=0 until the later SMP scheduler,
        // per-CPU descriptor-table and synchronization milestones exist.
        unsafe { asm!("cli; hlt", options(nomem, nostack)) };
    }
}

/// Start every enabled xAPIC AP in the published firmware topology, serially.
///
/// # Safety
/// Sole BSP, IF=0, BootInfo v4 trampoline identity mapping live, current CR3
/// below 4 GiB, and activate_pit_timer() has retained writable LAPIC slot zero.
pub unsafe fn start_enabled(info: &BootInfo) -> Result<Summary, Error> {
    let bsp = per_cpu::bsp().ok_or(Error::MissingBsp)?;
    let (trampoline, vector) = unsafe { install_common(info)? };
    let stack_patch = template_offset(core::ptr::addr_of!(vibrix_ap_stack_top))?;

    let mut requested = 0usize;
    for (index, slot) in per_cpu::slots().iter().copied().enumerate() {
        if slot.availability != Availability::Enabled || slot.apic_id == bsp.apic_id {
            continue;
        }
        if slot.x2apic || slot.apic_id > u32::from(u8::MAX) {
            return Err(Error::UnsupportedTopology);
        }

        let top = stack_top(index)?;
        unsafe { patch_u64(trampoline, stack_patch, top) };
        AP_BIND_ERROR.store(0, Ordering::Release);
        // Publish all trampoline patches before the target can observe them.
        core::sync::atomic::fence(Ordering::SeqCst);
        unsafe { asm!("mfence", options(nostack, preserves_flags)) };

        unsafe { super::apic::startup_xapic(slot.apic_id as u8, vector) }.map_err(Error::Apic)?;
        requested += 1;

        let mut online = false;
        for _ in 0..STARTUP_WAIT_SPINS {
            if per_cpu::is_online(slot.apic_id) {
                online = true;
                break;
            }
            let bind_error = AP_BIND_ERROR.load(Ordering::Acquire);
            if bind_error != 0 {
                return Err(Error::Bind(bind_error - 1));
            }
            unsafe { asm!("pause", options(nomem, nostack, preserves_flags)) };
        }
        if !online {
            return Err(Error::Timeout(slot.apic_id));
        }
    }

    Ok(Summary {
        requested,
        online: per_cpu::online_count(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stack_bank_has_one_aligned_disjoint_stack_per_cpu_slot() {
        let base = AP_STACKS.0.get().cast::<u8>() as u64;
        assert_eq!(base & 0xf, 0);
        assert_eq!(stack_top(0).unwrap() - base, AP_STACK_BYTES as u64);
        assert_eq!(
            stack_top(MAX_PROCESSORS - 1).unwrap() - base,
            (MAX_PROCESSORS * AP_STACK_BYTES) as u64
        );
        assert_eq!(stack_top(MAX_PROCESSORS), Err(Error::UnsupportedTopology));
    }

    #[test]
    fn sipi_page_contract_matches_bootinfo_validation() {
        for page in [0x1000u64, 0x8000, 0x000f_f000] {
            assert_eq!(page & 0xfff, 0);
            assert!((page >> 12) <= u64::from(u8::MAX));
        }
    }
}
