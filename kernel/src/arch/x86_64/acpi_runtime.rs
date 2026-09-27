//! Read retained ACPI SDTs through the BootInfo v3 temporary mapping window.
//! Unlike pure ACPI decoding, this module is kernel-only and maps real RAM.
//! No firmware calls, heap, global aliases, MMIO or dynamic page tables.

use core::slice;

use crate::BootInfo;
use crate::arch::x86_64::acpi::{AcpiError, Rsdp, Sdt};
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

/// Read the real XSDT (or ACPI 1.0 RSDT), then validate a real MCFG table.
/// The result is a number of validated MCFG allocations, not enabled ECAM.
///
/// # Safety
/// After ExitBootServices, sole boot CPU with IF=0; the temporary v3 window
/// is empty following memory::virtual_memory::runtime::smoke_test. Firmware
/// ACPI pages stay excluded from the physical allocator for this boot.
pub unsafe fn inspect(info: &BootInfo, rsdp: &Rsdp) -> Result<usize, ReadError> {
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
    for &physical in &root_addresses[..count] {
        // SAFETY: the previous mapping is completely retired; all addresses
        // are untrusted numbers until checked against ACPI-type map entries.
        let result = unsafe {
            with_table(&mut vm, physical, |bytes| {
                let table = Sdt::parse(bytes)?;
                if &table.signature != b"MCFG" {
                    return Ok(0usize);
                }
                let allocations = table.mcfg_entries()?;
                let mut count = 0usize;
                for entry in allocations.iter() {
                    entry?;
                    count += 1;
                }
                Ok(count)
            })
        }?;
        mcfg_allocations = mcfg_allocations
            .checked_add(result)
            .ok_or(ReadError::TooManyEntries)?;
    }
    if mcfg_allocations == 0 {
        return Err(ReadError::MissingMcfg);
    }
    Ok(mcfg_allocations)
}
