//! Early xAPIC/I/O-APIC discovery without enabling interrupt delivery.
//!
//! The MADT provides physical addresses. This module proves those addresses
//! are firmware-described UC MMIO, checks the architectural LAPIC base MSR,
//! and reads ID/version registers through short-lived BootInfo v3 mappings.
//! It does not program redirection entries, unmask IRQs, or execute STI.

use core::arch::{asm, x86_64::__cpuid_count};

use crate::BootInfo;
use crate::arch::x86_64::irq::Route;
use crate::memory::{self, virtual_memory::Window};

const PAGE: u64 = 4096;
const WINDOW_BASE: u64 = 0xffff_c000_0000_0000;
const IA32_APIC_BASE: u32 = 0x1b;
const IA32_PAT: u32 = 0x277;
const APIC_GLOBAL_ENABLE: u64 = 1 << 11;
const APIC_X2_MODE: u64 = 1 << 10;
const LAPIC_EOI: usize = 0x0b0;
const LAPIC_SPURIOUS: usize = 0x0f0;
const LAPIC_SOFTWARE_ENABLE: u32 = 1 << 8;
const IOAPIC_REDIRECTION_BASE: u32 = 0x10;
const PIT_COMMAND: u16 = 0x43;
const PIT_CHANNEL_ZERO: u16 = 0x40;
const PIC_MASTER_MASK: u16 = 0x21;
const PIC_SLAVE_MASK: u16 = 0xa1;
const PIT_DIVISOR_100HZ: u16 = 11_932;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApicError {
    Unsupported,
    X2ApicMode,
    AddressMismatch,
    MmioRange,
    Mapping,
    InvalidRegister,
    Routing,
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
    if !physical.is_multiple_of(PAGE) || !unsafe { memory::external_mmio_page_is_safe(physical) } {
        return Err(ApicError::MmioRange);
    }
    let base = virtual_ptr(0)?;
    // SAFETY: validated firmware MMIO page and exclusive early window.
    unsafe { vm.map_mmio_readonly(0, physical) }.map_err(|_| ApicError::Mapping)?;
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

unsafe fn outb(port: u16, value: u8) {
    // SAFETY: caller owns ring-zero legacy interrupt-controller setup.
    unsafe {
        asm!(
            "out dx, al",
            in("dx") port,
            in("al") value,
            options(nomem, nostack, preserves_flags)
        );
    }
}

unsafe fn write_ioapic_register(base: usize, selector: u32, value: u32) {
    // SAFETY: caller owns one writable UC I/O APIC page and supplies an
    // architectural register selector.
    unsafe {
        core::ptr::write_volatile(base as *mut u32, selector);
        core::ptr::write_volatile((base + 0x10) as *mut u32, value);
    }
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
    if !physical.is_multiple_of(PAGE) || !unsafe { memory::external_mmio_page_is_safe(physical) } {
        return Err(ApicError::MmioRange);
    }
    let base = virtual_ptr(0)?;
    // SAFETY: IOREGSEL itself is a required write even for register reads.
    unsafe { vm.map_mmio_writable(0, physical) }.map_err(|_| ApicError::Mapping)?;
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
    let (lapic_id, lapic_version, lapic_max_lvt) = unsafe { read_lapic(&mut vm, lapic_physical) }?;
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

/// Permanently map the BSP LAPIC and selected I/O APIC into the reserved
/// BootInfo v3 window, route the legacy PIT interrupt and program PIT channel 0
/// near 100 Hz. Interrupts remain disabled until enable_interrupts().
///
/// On success window slots 0 and 1 are intentionally retained for the entire
/// early-kernel runtime; no later Window owner may be created.
///
/// # Safety
/// Sole BSP, IF=0, no other mapping-window owner. The supplied APIC summary
/// and physical addresses must come from the immediately preceding validated
/// MADT/APIC probe. The selected route must describe this I/O APIC's GSI range.
pub unsafe fn activate_pit_timer(
    info: &BootInfo,
    lapic_physical: u64,
    ioapic_physical: u64,
    ioapic_gsi_base: u32,
    max_redirection_entry: u8,
    route: Route,
) -> Result<(), ApicError> {
    let (index, redirection) = route
        .redirection_entry(max_redirection_entry, ioapic_gsi_base)
        .map_err(|_| ApicError::Routing)?;
    if !lapic_physical.is_multiple_of(PAGE)
        || !ioapic_physical.is_multiple_of(PAGE)
        || !unsafe { memory::external_mmio_page_is_safe(lapic_physical) }
        || !unsafe { memory::external_mmio_page_is_safe(ioapic_physical) }
    {
        return Err(ApicError::MmioRange);
    }
    if __cpuid_count(1, 0).edx & (1 << 9) == 0 || !unsafe { pat_index_three_is_uc() } {
        return Err(ApicError::Unsupported);
    }
    // SAFETY: APIC CPUID support establishes architectural APIC_BASE MSR.
    let apic_base = unsafe { rdmsr(IA32_APIC_BASE) };
    if apic_base & APIC_GLOBAL_ENABLE == 0
        || apic_base & APIC_X2_MODE != 0
        || apic_base & 0x000f_ffff_ffff_f000 != lapic_physical
    {
        return Err(ApicError::AddressMismatch);
    }

    // SAFETY: caller grants exclusive ownership of the empty v3 leaf table.
    let mut vm = unsafe { crate::memory::virtual_memory::runtime::from_boot_info(info) }
        .map_err(|_| ApicError::Mapping)?;
    let lapic_virtual =
        unsafe { vm.map_mmio_writable(0, lapic_physical) }.map_err(|_| ApicError::Mapping)?;
    let ioapic_virtual = match unsafe { vm.map_mmio_writable(1, ioapic_physical) } {
        Ok(address) => address,
        Err(_) => {
            // SAFETY: no pointer/reference escaped the first temporary map.
            let _ = unsafe { vm.unmap(0) };
            return Err(ApicError::Mapping);
        }
    };
    let lapic_base = usize::try_from(lapic_virtual).map_err(|_| ApicError::Mapping)?;
    let ioapic_base = usize::try_from(ioapic_virtual).map_err(|_| ApicError::Mapping)?;

    // SAFETY: SVR is an aligned writable LAPIC register in the retained UC map.
    let svr_ptr = (lapic_base + LAPIC_SPURIOUS) as *mut u32;
    let svr = unsafe { core::ptr::read_volatile(svr_ptr) };
    unsafe {
        core::ptr::write_volatile(
            svr_ptr,
            (svr & !0xff)
                | u32::from(crate::arch::x86_64::irq::SPURIOUS_VECTOR)
                | LAPIC_SOFTWARE_ENABLE,
        );
    }

    let low_selector = IOAPIC_REDIRECTION_BASE + u32::from(index) * 2;
    let high_selector = low_selector + 1;
    // Program destination first while the existing low word is still masked,
    // then publish the unmasked vector/polarity/trigger low word.
    unsafe {
        write_ioapic_register(ioapic_base, high_selector, (redirection >> 32) as u32);
        write_ioapic_register(ioapic_base, low_selector, redirection as u32);
        // Disable both legacy PICs so only the I/O APIC owns PIT delivery.
        outb(PIC_MASTER_MASK, 0xff);
        outb(PIC_SLAVE_MASK, 0xff);
        // PIT mode 3, lobyte/hibyte, binary counter on channel 0.
        outb(PIT_COMMAND, 0x36);
        outb(PIT_CHANNEL_ZERO, PIT_DIVISOR_100HZ as u8);
        outb(PIT_CHANNEL_ZERO, (PIT_DIVISOR_100HZ >> 8) as u8);
    }
    Ok(())
}

/// Acknowledge one delivered local-APIC interrupt.
///
/// # Safety
/// activate_pit_timer() must have succeeded and retained LAPIC window slot 0.
pub unsafe fn eoi() {
    let eoi = (WINDOW_BASE as usize + LAPIC_EOI) as *mut u32;
    // SAFETY: retained writable UC LAPIC mapping is the activation invariant.
    unsafe { core::ptr::write_volatile(eoi, 0) };
}

/// Enable or disable maskable interrupts on the sole early BSP.
///
/// # Safety
/// Call enable only after every IDT gate and interrupt controller route that
/// can deliver to this CPU is initialized. No SMP or reentrant shared state.
pub unsafe fn enable_interrupts() {
    unsafe { asm!("sti", options(nomem, nostack, preserves_flags)) };
}

