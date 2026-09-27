//! Early xAPIC/I/O-APIC discovery without enabling interrupt delivery.
//!
//! The MADT provides physical addresses. This module proves those addresses
//! are firmware-described UC MMIO, checks the architectural LAPIC base MSR,
//! and reads ID/version registers through short-lived BootInfo v3 mappings.
//! It does not program redirection entries, unmask IRQs, or execute STI.

use core::arch::{asm, x86_64::__cpuid_count};

use crate::BootInfo;
use crate::memory::{self, virtual_memory::Window};

const PAGE: u64 = 4096;
const WINDOW_BASE: u64 = 0xffff_c000_0000_0000;
const IA32_APIC_BASE: u32 = 0x1b;
const IA32_PAT: u32 = 0x277;
const APIC_GLOBAL_ENABLE: u64 = 1 << 11;
const APIC_X2_MODE: u64 = 1 << 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApicError {
    Unsupported,
    X2ApicMode,
    AddressMismatch,
    MmioRange,
    Mapping,
    InvalidRegister,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApicSummary {
    pub lapic_id: u8,
    pub lapic_version: u8,
    pub lapic_max_lvt: u8,
    pub ioapic_id: u8,
    pub ioapic_version: u8,
    pub ioapic_max_redirection_entry: u8,
}

unsafe fn rdmsr(msr: u32) -> u64 {
    let low: u32;
    let high: u32;
    // SAFETY: caller runs CPL0 and supplies an architectural MSR supported
    // by the preflight checks at each call site.
    unsafe {
        asm!(
            "rdmsr",
            in("ecx") msr,
            out("eax") low,
            out("edx") high,
            options(nomem, nostack, preserves_flags)
        );
    }
    (u64::from(high) << 32) | u64::from(low)
}

unsafe fn pat_index_three_is_uc() -> bool {
    if __cpuid_count(1, 0).edx & (1 << 16) == 0 {
        return false;
    }
    // SAFETY: PAT capability was checked above; CPL0 boot CPU.
    ((unsafe { rdmsr(IA32_PAT) } >> 24) & 0xff) == 0
}

fn virtual_ptr(offset: usize) -> Result<usize, ApicError> {
    usize::try_from(WINDOW_BASE)
        .ok()
        .and_then(|base| base.checked_add(offset))
        .ok_or(ApicError::Mapping)
}

unsafe fn read_lapic(vm: &mut Window, physical: u64) -> Result<(u8, u8, u8), ApicError> {
    if !physical.is_multiple_of(PAGE)
        || !unsafe { memory::mmio_span_is_reserved(physical, PAGE) }
    {
        return Err(ApicError::MmioRange);
    }
    // SAFETY: validated firmware MMIO page and exclusive early window.
    unsafe { vm.map_mmio_readonly(0, physical) }.map_err(|_| ApicError::Mapping)?;
    let base = virtual_ptr(0)?;
    // SAFETY: LAPIC ID/version are aligned read-only architectural registers.
    let id = unsafe { core::ptr::read_volatile((base + 0x20) as *const u32) };
    // SAFETY: same live UC mapping, version register at aligned offset 0x30.
    let version = unsafe { core::ptr::read_volatile((base + 0x30) as *const u32) };
    // SAFETY: no references escape these volatile reads.
    unsafe { vm.unmap(0) }.map_err(|_| ApicError::Mapping)?;
    let lapic_version = (version & 0xff) as u8;
    let max_lvt = ((version >> 16) & 0xff) as u8;
    if lapic_version == 0 || lapic_version == 0xff || max_lvt < 2 {
        return Err(ApicError::InvalidRegister);
    }
    Ok(((id >> 24) as u8, lapic_version, max_lvt))
}

unsafe fn ioapic_register(base: usize, selector: u32) -> u32 {
    // SAFETY: caller owns one writable UC I/O APIC page. IOREGSEL accepts
    // the bounded register index, and IOWIN is the associated 32-bit window.
    unsafe {
        core::ptr::write_volatile(base as *mut u32, selector);
        core::ptr::read_volatile((base + 0x10) as *const u32)
    }
}

unsafe fn read_ioapic(vm: &mut Window, physical: u64) -> Result<(u8, u8, u8), ApicError> {
    if !physical.is_multiple_of(PAGE)
        || !unsafe { memory::mmio_span_is_reserved(physical, PAGE) }
    {
        return Err(ApicError::MmioRange);
    }
    // SAFETY: IOREGSEL itself is a required write even for register reads.
    unsafe { vm.map_mmio_writable(0, physical) }.map_err(|_| ApicError::Mapping)?;
    let base = virtual_ptr(0)?;
    // SAFETY: selectors 0 and 1 are architectural ID/version registers.
    let id = unsafe { ioapic_register(base, 0) };
    let version = unsafe { ioapic_register(base, 1) };
    // SAFETY: selector transaction is complete and no mapping reference escapes.
    unsafe { vm.unmap(0) }.map_err(|_| ApicError::Mapping)?;
    let io_version = (version & 0xff) as u8;
    let max_entry = ((version >> 16) & 0xff) as u8;
    if io_version == 0 || io_version == 0xff || max_entry == 0 {
        return Err(ApicError::InvalidRegister);
    }
    Ok((((id >> 24) & 0x0f) as u8, io_version, max_entry))
}

/// Validate and read one MADT-selected local APIC and I/O APIC.
///
/// # Safety
/// Sole boot CPU, IF=0, BootInfo v3 window currently empty, immutable final
/// firmware map initialized, and no concurrent PAT/APIC reconfiguration.
pub unsafe fn probe(
    info: &BootInfo,
    lapic_physical: u64,
    ioapic_physical: u64,
) -> Result<ApicSummary, ApicError> {
    if __cpuid_count(1, 0).edx & (1 << 9) == 0 {
        return Err(ApicError::Unsupported);
    }
    // SAFETY: CPUID confirms local APIC support; architectural MSR at CPL0.
    let apic_base = unsafe { rdmsr(IA32_APIC_BASE) };
    if apic_base & APIC_GLOBAL_ENABLE == 0 {
        return Err(ApicError::Unsupported);
    }
    if apic_base & APIC_X2_MODE != 0 {
        return Err(ApicError::X2ApicMode);
    }
    if apic_base & 0x000f_ffff_ffff_f000 != lapic_physical {
        return Err(ApicError::AddressMismatch);
    }
    // SAFETY: PAT exists on all supported QEMU CPUs only after this check.
    if !unsafe { pat_index_three_is_uc() } {
        return Err(ApicError::Unsupported);
    }
    // SAFETY: v3 retained empty leaf table, sole BSP and IF=0.
    let mut vm = unsafe { crate::memory::virtual_memory::runtime::from_boot_info(info) }
        .map_err(|_| ApicError::Mapping)?;
    // SAFETY: each helper independently validates its full MMIO page.
    let (lapic_id, lapic_version, lapic_max_lvt) =
        unsafe { read_lapic(&mut vm, lapic_physical) }?;
    // SAFETY: same empty window reused after LAPIC helper unmapped it.
    let (ioapic_id, ioapic_version, ioapic_max_redirection_entry) =
        unsafe { read_ioapic(&mut vm, ioapic_physical) }?;
    Ok(ApicSummary {
        lapic_id,
        lapic_version,
        lapic_max_lvt,
        ioapic_id,
        ioapic_version,
        ioapic_max_redirection_entry,
    })
}
