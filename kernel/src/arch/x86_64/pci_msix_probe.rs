//! Opt-in QEMU ivshmem-doorbell MSI-X delivery evidence.
//! Test-only integration of the production MSI-X programming contract.

use core::arch::{asm, x86_64::__cpuid_count};
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use super::pci_msi::{ConfigSpace, Message};
use super::pci_msix::{self, Layout, MemoryBar, TableAccess};
use super::{apic, irq, pci, pci_interrupts, pci_msi};
use crate::memory::{self, virtual_memory::Window};

pub(super) const VECTOR: u8 = 0x51;
const VENDOR: u16 = 0x1af4;
const DEVICE: u16 = 0x1110;
const MSIX_BAR_BYTES: u64 = 4096;

static BDF: AtomicU32 = AtomicU32::new(0);
static READY: AtomicBool = AtomicBool::new(false);
static EXERCISED: AtomicBool = AtomicBool::new(false);
static DELIVERED: AtomicU64 = AtomicU64::new(0);

fn interrupts_disabled() -> bool {
    let flags: u64;
    // SAFETY: inspect RFLAGS without changing interrupt state.
    unsafe { asm!("pushfq", "pop {}", out(reg) flags, options(preserves_flags)) };
    flags & (1 << 9) == 0
}

struct Config(pci::Bdf);

impl ConfigSpace for Config {
    fn read32(&mut self, offset: u8) -> Option<u32> {
        if !interrupts_disabled() {
            return None;
        }
        pci::configuration_address(self.0, offset)?;
        // SAFETY: sole BSP, IF=0, serialized legacy config access.
        Some(unsafe { pci::read_legacy_dword(self.0, offset) })
    }

    fn write16(&mut self, offset: u8, value: u16) -> bool {
        if !interrupts_disabled() || offset & 1 != 0 {
            return false;
        }
        let Some(address) = pci::configuration_address(self.0, offset & !3) else {
            return false;
        };
        // SAFETY: exclusive test function. WORD access avoids adjacent W1C status.
        unsafe {
            asm!("out dx, eax", in("dx") 0xcf8u16, in("eax") address,
                options(nostack, preserves_flags));
            asm!("out dx, ax", in("dx") (0xcfcu16 + u16::from(offset & 2)),
                in("ax") value, options(nostack, preserves_flags));
        }
        true
    }

    fn write32(&mut self, _offset: u8, _value: u32) -> bool {
        // The production MSI-X path must not need configuration DWORD writes.
        false
    }
}

struct Table {
    base: usize,
    bytes: u32,
}

impl TableAccess for Table {
    fn read32(&mut self, offset: u32) -> Option<u32> {
        if offset & 3 != 0 || offset.checked_add(4)? > self.bytes {
            return None;
        }
        // SAFETY: base is the validated UC table mapping and offset is bounded.
        Some(unsafe { core::ptr::read_volatile((self.base + offset as usize) as *const u32) })
    }

    fn write32(&mut self, offset: u32, value: u32) -> bool {
        if offset & 3 != 0 || offset.checked_add(4).is_none_or(|end| end > self.bytes) {
            return false;
        }
        // SAFETY: base is the validated writable UC MSI-X table mapping.
        unsafe { core::ptr::write_volatile((self.base + offset as usize) as *mut u32, value) };
        true
    }
}

/// Program vector zero on one QEMU ivshmem-doorbell device.
///
/// # Safety
/// Sole BSP, IF=0, QEMU TCG, no AP startup or passthrough/IOMMU. The permanent
/// vector handler is installed. Window slots 0/1 belong to APICs and slot 2 is
/// temporarily free. The isolated device is not shared with another driver.
pub(super) unsafe fn prepare(vm: &mut Window, destination: u8) -> Result<(), &'static str> {
    if !interrupts_disabled() || READY.load(Ordering::Acquire) {
        return Err("invalid MSI-X initialization state");
    }
    let leaf = __cpuid_count(0x4000_0000, 0);
    let mut vendor = [0u8; 12];
    vendor[..4].copy_from_slice(&leaf.ebx.to_le_bytes());
    vendor[4..8].copy_from_slice(&leaf.ecx.to_le_bytes());
    vendor[8..].copy_from_slice(&leaf.edx.to_le_bytes());
    if __cpuid_count(1, 0).ecx & (1 << 31) == 0 || vendor != *b"TCGTCGTCGTCG" {
        return Err("MSI-X probe requires QEMU TCG");
    }

    let mut found = None;
    let mut duplicate = false;
    pci::scan_segment_zero(
        |bdf, offset| unsafe { pci::read_legacy_dword(bdf, offset) },
        |device| {
            if device.vendor == VENDOR && device.id == DEVICE {
                if found.is_some() {
                    duplicate = true;
                }
                found = Some(device);
            }
        },
    );
    let device = found.ok_or("ivshmem-doorbell missing")?;
    if duplicate || device.header_type & 0x7f != 0 || device.revision != 1 {
        return Err("ivshmem identity/header/revision is unsupported");
    }

    let mut io = Config(device.bdf);
    let caps = pci_interrupts::discover(&mut |offset| io.read32(offset))
        .map_err(|_| "invalid ivshmem capabilities")?;
    let cap = caps.msix.ok_or("ivshmem has no MSI-X capability")?;
    if caps.msi.is_some() || cap.vectors() != 1 || cap.control() & 0x8000 != 0 {
        return Err("unexpected ivshmem interrupt layout/state");
    }

    let raw_bar = io.read32(0x14).ok_or("ivshmem BAR1 unavailable")?;
    if raw_bar & 0xf != 0 {
        return Err("ivshmem BAR1 is not 32-bit non-prefetchable memory");
    }
    let physical = u64::from(raw_bar & !0xf);
    if physical == 0
        || !physical.is_multiple_of(MSIX_BAR_BYTES)
        || !unsafe { memory::external_mmio_page_is_safe(physical) }
    {
        return Err("ivshmem BAR1 range is unsafe");
    }

    let mut bars = [None; 6];
    bars[1] = Some(MemoryBar::new(physical, MSIX_BAR_BYTES).map_err(|_| "invalid BAR1 extent")?);
    let layout = Layout::new(cap, &bars).map_err(|_| "MSI-X layout outside BAR1")?;
    if layout.table_physical() / MSIX_BAR_BYTES != physical / MSIX_BAR_BYTES
        || layout.pending_physical() / MSIX_BAR_BYTES != physical / MSIX_BAR_BYTES
    {
        return Err("MSI-X table/PBA do not fit the dedicated BAR1 page");
    }

    // QEMU routes MSI-X message writes through the PCI bus-master address space.
    // Enable memory decoding + bus mastering for this isolated interrupt proof
    // and disable legacy INTx. No device DMA engine or shared-memory transfer
    // is programmed, and BME is cleared again after MSI-X disable.
    let command = io.read32(4).ok_or("ivshmem command unavailable")? as u16;
    pci_msi::write16_checked(&mut io, 4, command | 0x406)
        .map_err(|_| "ivshmem command readback failed")?;

    let virtual_bar = unsafe { vm.map_mmio_writable(2, physical) }
        .map_err(|_| "ivshmem MSI-X BAR mapping failed")?;
    let table_offset =
        usize::try_from(layout.table_physical() - physical).map_err(|_| "ivshmem table offset")?;
    let mut table = Table {
        base: usize::try_from(virtual_bar).map_err(|_| "ivshmem virtual address")? + table_offset,
        bytes: layout.table_bytes(),
    };
    let message = Message::new(destination, VECTOR).map_err(|_| "invalid MSI-X destination")?;
    let programmed =
        unsafe { pci_msix::enable_entry(&mut io, &mut table, caps, layout, 0, message) };
    // SAFETY: all CPU accesses to BAR1 ended before retiring the scratch mapping.
    unsafe { vm.unmap(2) }.map_err(|_| "ivshmem MSI-X BAR unmap failed")?;
    programmed.map_err(|_| "ivshmem MSI-X programming/readback failed")?;

    BDF.store(
        (u32::from(device.bdf.bus) << 16)
            | (u32::from(device.bdf.device) << 8)
            | u32::from(device.bdf.function),
        Ordering::Release,
    );
    READY.store(true, Ordering::Release);
    crate::debugcon::write("VIBRIX: native IVSHMEM MSI-X armed\r\n");
    Ok(())
}

pub(super) fn interrupt() {
    if !READY.load(Ordering::Acquire) {
        return;
    }
    DELIVERED.fetch_add(1, Ordering::Release);
    // SAFETY: delivery is enabled only after the retained LAPIC mapping exists.
    unsafe { apic::eoi() };
}

fn wait_for_count(expected: u64) -> Result<(), &'static str> {
    let start = irq::timer_ticks();
    loop {
        if DELIVERED.load(Ordering::Acquire) >= expected {
            return Ok(());
        }
        if irq::timer_ticks().wrapping_sub(start) > 200 {
            return Err("MSI-X delivery timed out");
        }
        core::hint::spin_loop();
    }
}

/// Verify repeated eventfd-driven delivery, then disable and prove silence.
///
/// # Safety
/// prepare() succeeded, APIC/PIT are live on the sole BSP and IF=1. The handler
/// and configuration identity remain permanently valid for this diagnostic boot.
pub(super) unsafe fn exercise() -> Result<(), &'static str> {
    if !READY.load(Ordering::Acquire)
        || interrupts_disabled()
        || EXERCISED.swap(true, Ordering::AcqRel)
    {
        return Err("MSI-X exercise state invalid");
    }

    wait_for_count(1)?;
    crate::debugcon::write("VIBRIX: native MSI-X first delivery observed\r\n");
    wait_for_count(2)?;
    crate::debugcon::write("VIBRIX: native MSI-X repeated delivery verified\r\n");

    let key = BDF.load(Ordering::Acquire);
    let mut io = Config(pci::Bdf {
        bus: (key >> 16) as u8,
        device: (key >> 8) as u8,
        function: key as u8,
    });
    // SAFETY: enter the sole-BSP serialized configuration critical section.
    unsafe { asm!("cli", options(nostack)) };
    let disabled = (|| {
        let caps = pci_interrupts::discover(&mut |offset| io.read32(offset))
            .map_err(|_| "ivshmem capability reread failed")?;
        let cap = caps.msix.ok_or("ivshmem MSI-X disappeared")?;
        unsafe { pci_msix::disable(&mut io, cap) }.map_err(|_| "MSI-X disable readback failed")?;
        let command = io.read32(4).ok_or("ivshmem command reread failed")? as u16;
        pci_msi::write16_checked(&mut io, 4, command & !4)
            .map_err(|_| "ivshmem bus-master disable readback failed")
    })();
    // SAFETY: permanent routes/handlers remain installed.
    unsafe { asm!("sti", options(nostack)) };
    disabled?;
    crate::debugcon::write("VIBRIX: native MSI-X disabled\r\n");

    let start = irq::timer_ticks();
    while irq::timer_ticks().wrapping_sub(start) < 5 {
        core::hint::spin_loop();
    }
    if DELIVERED.load(Ordering::Acquire) != 2 {
        return Err("disabled MSI-X delivered an interrupt");
    }
    crate::println!(
        "kernel MSI-X: delivered=2 disabled=true bus_master=false post_disable_silent=true"
    );
    crate::debugcon::write("VIBRIX: native MSI-X post-disable silence verified\r\n");
    Ok(())
}
