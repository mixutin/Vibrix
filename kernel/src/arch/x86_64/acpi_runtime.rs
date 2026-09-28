//! Read retained ACPI SDTs through the BootInfo v3 temporary mapping window.
//! Unlike pure ACPI decoding, this module is kernel-only and maps real RAM.
//! No firmware calls, heap, global aliases, MMIO or dynamic page tables.

use core::arch::{asm, x86_64::__cpuid_count};
use core::slice;

use crate::BootInfo;
use crate::arch::x86_64::acpi::{AcpiError, EcamSummary, McfgEntry, Rsdp, Sdt};
use crate::arch::x86_64::irq::LEGACY_TIMER_IRQ;
use crate::memory::{self, virtual_memory::Window};

const PAGE: u64 = 4096;
const MAX_SDT: usize = 1024 * 1024;
const MAX_ROOT_ENTRIES: usize = 64;
const WINDOW_BASE: u64 = 0xffff_c000_0000_0000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadError {
    PhysicalRange,
    Mapping,
    TableLength,
    RootTable,
    TooManyEntries,
    MissingMcfg,
    MissingEcam,
    MultipleEcam,
    MissingApic,
    MultipleApic,
    MmioRange,
    PatNotUncached,
    Acpi(AcpiError),
}

impl From<AcpiError> for ReadError {
    fn from(value: AcpiError) -> Self {
        Self::Acpi(value)
    }
}

/// Temporarily map the complete checked SDT as supervisor-only, NX, WB RAM.
/// The callback cannot keep a reference into the mapping after this returns.
/// All already mapped leaves are retired on both success and failure.
///
/// # Safety
/// The loader's v3 window must be exclusively owned on the sole boot CPU
/// with IF=0; the map is initialized, immutable and covers reserved ACPI RAM.
/// Firmware/other CPUs must never mutate tables while they are read.
unsafe fn with_table<T>(
    vm: &mut Window,
    physical: u64,
    consume: impl FnOnce(&[u8]) -> Result<T, ReadError>,
) -> Result<T, ReadError> {
    let mut mapped = 0usize;
    let result = (|| -> Result<T, ReadError> {
        if physical == 0 || !unsafe { memory::acpi_span_is_reserved(physical, 36) } {
            return Err(ReadError::PhysicalRange);
        }
        let base = physical & !(PAGE - 1);
        let offset = usize::try_from(physical - base).map_err(|_| ReadError::PhysicalRange)?;
        let initial = (offset + 36).div_ceil(PAGE as usize);
        for i in 0..initial {
            // SAFETY: each page was checked as retained WB ACPI RAM; IF=0.
            unsafe { vm.map(i, base + i as u64 * PAGE, false) }.map_err(|_| ReadError::Mapping)?;
            mapped += 1;
        }
        let address =
            usize::try_from(WINDOW_BASE + offset as u64).map_err(|_| ReadError::Mapping)?;
        // SAFETY: initial leaves map all 36 readable header bytes.
        let header = unsafe { slice::from_raw_parts(address as *const u8, 36) };
        let length = usize::try_from(u32::from_le_bytes(
            header[4..8]
                .try_into()
                .map_err(|_| ReadError::TableLength)?,
        ))
        .map_err(|_| ReadError::TableLength)?;
        if !(36..=MAX_SDT).contains(&length)
            || offset.checked_add(length).is_none_or(|n| n > 512 * 4096)
            || !unsafe { memory::acpi_span_is_reserved(physical, length as u64) }
        {
            return Err(ReadError::TableLength);
        }
        let pages = (offset + length).div_ceil(PAGE as usize);
        for i in initial..pages {
            // SAFETY: entire physical span, including page padding, checked
            // against the immutable UEFI map before adding any new leaves.
            unsafe { vm.map(i, base + i as u64 * PAGE, false) }.map_err(|_| ReadError::Mapping)?;
            mapped += 1;
        }
        // SAFETY: all bytes are mapped and firmware-owned during the callback.
        let bytes = unsafe { slice::from_raw_parts(address as *const u8, length) };
        consume(bytes)
    })();

    let mut cleanup_failed = false;
    for index in (0..mapped).rev() {
        // SAFETY: the callback has returned; no borrow into mapped RAM remains.
        if unsafe { vm.unmap(index) }.is_err() {
            cleanup_failed = true;
        }
    }
    if cleanup_failed {
        Err(ReadError::Mapping)
    } else {
        result
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Discovery {
    pub allocations: usize,
    pub ecam: EcamSummary,
    pub lapic_physical: u64,
    pub ioapic_physical: u64,
    pub ioapic_gsi_base: u32,
    pub ioapics: usize,
    pub timer_gsi: u32,
    pub timer_active_low: bool,
    pub timer_level_triggered: bool,
}

/// Only create UC mappings when x86 PAT index 3 (PCD=1, PWT=1) is UC.
/// Firmware might have reprogrammed the PAT MSR; do not silently alias MMIO
/// with a cacheable memory type. This boot CPU owns the early paging window.
///
/// # Safety
/// Ring-zero x86-64 with PAT MSR access; no other CPU changes PAT concurrently.
unsafe fn pat_index_three_is_uc() -> bool {
    if __cpuid_count(1, 0).edx & (1 << 16) == 0 {
        return false;
    }
    let low: u32;
    let high: u32;
    // SAFETY: PAT advertised in CPUID, CPL0, architectural IA32_PAT MSR.
    unsafe {
        asm!(
            "rdmsr",
            in("ecx") 0x277_u32,
            out("eax") low,
            out("edx") high,
            options(nomem, nostack, preserves_flags)
        );
    }
    let _upper_pat_entries = high;
    (low >> 24) as u8 == 0
}

/// A single aligned volatile PCI ECAM dword read; no MMIO writes.
/// All page extents are checked before mapping, including page padding.
///
/// # Safety
/// One boot CPU, IF=0, validated MCFG allocation and firmware MMIO region,
/// PAT index 3 UC, no concurrent users or conflicting cache aliases.
unsafe fn read_ecam_word(vm: &mut Window, physical: u64) -> Result<u32, ReadError> {
    if !physical.is_multiple_of(4) {
        return Err(ReadError::MmioRange);
    }
    let page = physical & !(PAGE - 1);
    let offset = usize::try_from(physical - page).map_err(|_| ReadError::MmioRange)?;
    if offset > PAGE as usize - 4 || !unsafe { memory::mmio_span_is_reserved(page, PAGE) } {
        return Err(ReadError::MmioRange);
    }
    // Prevalidate virtual-pointer arithmetic before changing any mapping.
    let ptr = usize::try_from(WINDOW_BASE)
        .ok()
        .and_then(|start| start.checked_add(offset))
        .ok_or(ReadError::Mapping)?;
    // SAFETY: device config MMIO is exclusively mapped UC, RO and NX.
    unsafe { vm.map_mmio_readonly(0, page) }.map_err(|_| ReadError::Mapping)?;
    // SAFETY: aligned four-byte register lies within the mapped MMIO page.
    let word = unsafe { core::ptr::read_volatile(ptr as *const u32) };
    // SAFETY: the volatile read is complete and no Rust reference remains.
    unsafe { vm.unmap(0) }.map_err(|_| ReadError::Mapping)?;
    Ok(word)
}

/// Read the real XSDT (or ACPI 1.0 RSDT), then validate a real MCFG table.
/// The result is validated MCFG counts and actual bus-zero ECAM reads,
/// not device enabling, BAR MMIO, DMA or a native driver.
///
/// # Safety
/// After ExitBootServices, sole boot CPU with IF=0; the temporary v3 window
/// is empty following memory::virtual_memory::runtime::smoke_test. Firmware
/// ACPI pages stay excluded from the physical allocator for this boot.
pub unsafe fn inspect(info: &BootInfo, rsdp: &Rsdp) -> Result<Discovery, ReadError> {
    // SAFETY: exclusive retained v3 leaf table, no active window references.
    let mut vm = unsafe { memory::virtual_memory::runtime::from_boot_info(info) }
        .map_err(|_| ReadError::Mapping)?;
    let (root_physical, signature) = if let Some(xsdt) = rsdp.xsdt_physical {
        (xsdt, *b"XSDT")
    } else {
        (u64::from(rsdp.rsdt_physical), *b"RSDT")
    };
    // SAFETY: mapping and ownership validated independently for the table.
    let (root_addresses, count) = unsafe {
        with_table(&mut vm, root_physical, |bytes| {
            let table = Sdt::parse(bytes)?;
            if table.signature != signature {
                return Err(ReadError::RootTable);
            }
            let entries = table.root_entries()?;
            if entries.len() > MAX_ROOT_ENTRIES {
                return Err(ReadError::TooManyEntries);
            }
            let mut addresses = [0u64; MAX_ROOT_ENTRIES];
            for (index, address) in addresses[..entries.len()].iter_mut().enumerate() {
                *address = entries.address(index)?;
            }
            Ok((addresses, entries.len()))
        })
    }?;
    let mut mcfg_allocations = 0usize;
    let mut selected_ecam = None;
    let mut selected_apic = None;
    for &physical in &root_addresses[..count] {
        // SAFETY: the previous mapping is completely retired; all addresses
        // are untrusted numbers until checked against ACPI-type map entries.
        let result = unsafe {
            with_table(&mut vm, physical, |bytes| {
                let table = Sdt::parse(bytes)?;
                if &table.signature == b"MCFG" {
                    let allocations = table.mcfg_entries()?;
                    let mut count = 0usize;
                    let mut bus_zero = None;
                    for entry in allocations.iter() {
                        let entry = entry?;
                        if entry.segment == 0 && entry.bus_start == 0 {
                            bus_zero = Some(entry);
                        }
                        count += 1;
                    }
                    return Ok((count, bus_zero, None));
                }
                if &table.signature == b"APIC" {
                    let topology = vibrix_kernel::cpu_topology::Topology::from_madt(table.bytes())
                        .map_err(|_| ReadError::Acpi(AcpiError::InvalidEntry))?;
                    for cpu in topology.processors() {
                        if cpu.availability == vibrix_kernel::cpu_topology::Availability::Enabled {
                            crate::println!(
                                "kernel CPU enabled: uid={} apic={} x2apic={}",
                                cpu.firmware_uid,
                                cpu.apic_id,
                                cpu.x2apic
                            );
                        }
                    }
                    crate::println!(
                        "kernel CPUs: enabled={} online_capable={} total={}",
                        topology.enabled_count(),
                        topology.online_capable_count(),
                        topology.processors().len()
                    );
                    crate::println!("VIBRIX: kernel CPU enumeration verified");
                    crate::debugcon::write("VIBRIX: kernel CPU enumeration verified\r\n");
                    let madt = table.madt_entries()?;
                    let first = madt.ioapics().next().ok_or(ReadError::MissingApic)??;
                    let timer = madt.interrupt_override(LEGACY_TIMER_IRQ)?;
                    let (timer_gsi, timer_active_low, timer_level_triggered) =
                        if let Some(override_) = timer {
                            (
                                override_.gsi,
                                override_.active_low,
                                override_.level_triggered,
                            )
                        } else {
                            (u32::from(LEGACY_TIMER_IRQ), false, false)
                        };
                    return Ok((
                        0usize,
                        None,
                        Some((
                            madt.lapic_address,
                            first.address,
                            first.gsi_base,
                            madt.ioapic_count(),
                            timer_gsi,
                            timer_active_low,
                            timer_level_triggered,
                        )),
                    ));
                }
                Ok((0usize, None, None))
            })
        }?;
        mcfg_allocations = mcfg_allocations
            .checked_add(result.0)
            .ok_or(ReadError::TooManyEntries)?;
        if let Some(entry) = result.1
            && selected_ecam.replace(entry).is_some()
        {
            return Err(ReadError::MultipleEcam);
        }
        if let Some(apic) = result.2
            && selected_apic.replace(apic).is_some()
        {
            return Err(ReadError::MultipleApic);
        }
    }
    if mcfg_allocations == 0 {
        return Err(ReadError::MissingMcfg);
    }
    let entry: McfgEntry = selected_ecam.ok_or(ReadError::MissingEcam)?;
    let (
        lapic_physical,
        ioapic_physical,
        ioapic_gsi_base,
        ioapics,
        timer_gsi,
        timer_active_low,
        timer_level_triggered,
    ) = selected_apic.ok_or(ReadError::MissingApic)?;
    // SAFETY: one CPL0 boot CPU, no other PAT owner at this stage.
    if !unsafe { pat_index_three_is_uc() } {
        return Err(ReadError::PatNotUncached);
    }
    // Validate that every bus-zero function-zero page is UC MMIO *before*
    // issuing the first read, so no unexpected memory type is accessed.
    for device in 0..32u8 {
        let physical = entry.config_physical(0, device, 0, 0)?;
        if !unsafe { memory::mmio_span_is_reserved(physical, PAGE) } {
            // QEMU diagnostic only: UEFI descriptor metadata are numeric
            // integers; do not dereference an unvalidated MMIO address.
            #[cfg(feature = "qemu-debugcon")]
            crate::println!(
                "kernel ECAM reject bus0 device {} UEFI descriptor {:?}",
                device,
                unsafe { memory::firmware_descriptor_at(physical) }
            );
            return Err(ReadError::MmioRange);
        }
    }
    // SAFETY: preflighted MCFG bus/page bounds; each page is temporarily
    // mapped supervisor RO/NX/UC and retired after its volatile dword read.
    let ecam = crate::arch::x86_64::acpi::scan_ecam_bus_zero(entry, |physical| {
        unsafe { read_ecam_word(&mut vm, physical) }.map_err(|_| AcpiError::InvalidAllocation)
    })?;
    if ecam.devices == 0 {
        return Err(ReadError::MissingEcam);
    }
    Ok(Discovery {
        allocations: mcfg_allocations,
        ecam,
        lapic_physical,
        ioapic_physical,
        ioapic_gsi_base,
        ioapics,
        timer_gsi,
        timer_active_low,
        timer_level_triggered,
    })
}
