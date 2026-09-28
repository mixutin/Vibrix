//! Opt-in QEMU TCG EDU MSI delivery evidence. Never a general device driver.
//!
//! Only the isolated test EDU function is modified. Its DMA engine is never
//! started or programmed. The command bus-master bit is required for MSI
//! message writes, then cleared after verified MSI disable. No BAR sizing,
//! disk access, physical-memory DMA, hotplug, AP startup or vector release.

use core::arch::{asm, x86_64::__cpuid_count};
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};

use super::pci_msi::{ConfigSpace, Message};
use super::{apic, irq, pci, pci_interrupts, pci_msi};
use crate::memory::{self, virtual_memory::Window};

pub(super) const VECTOR: u8 = 0x50;
const EDU_ID: u32 = 0x11e8_1234;
const STATUS: usize = 0x24;
const RAISE: usize = 0x60;
const ACK: usize = 0x64;

static MMIO: AtomicUsize = AtomicUsize::new(0);
static BDF: AtomicU32 = AtomicU32::new(0);
static READY: AtomicBool = AtomicBool::new(false);
static EXERCISED: AtomicBool = AtomicBool::new(false);
static DELIVERED: AtomicU64 = AtomicU64::new(0);
static UNEXPECTED: AtomicBool = AtomicBool::new(false);

fn interrupts_disabled() -> bool {
    let flags: u64;
    // SAFETY: inspect the current CPU flags without changing their state.
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
        // SAFETY: probe is sole BSP, IF=0; IRQ handlers never use CF8/CFC.
        Some(unsafe { pci::read_legacy_dword(self.0, offset) })
    }

    fn write16(&mut self, offset: u8, value: u16) -> bool {
        if !interrupts_disabled() || offset & 1 != 0 {
            return false;
        }
        let Some(address) = pci::configuration_address(self.0, offset & !3) else {
            return false;
        };
        // SAFETY: exclusive isolated EDU function. Use the actual WORD width,
        // never read-modify-write adjacent write-one-to-clear status bits.
        unsafe {
            asm!("out dx, eax", in("dx") 0xcf8u16, in("eax") address,
                options(nostack, preserves_flags));
            asm!("out dx, ax", in("dx") (0xcfcu16 + u16::from(offset & 2)),
                in("ax") value, options(nostack, preserves_flags));
        }
        true
    }

    fn write32(&mut self, offset: u8, value: u32) -> bool {
        if !interrupts_disabled() {
            return false;
        }
        let Some(address) = pci::configuration_address(self.0, offset) else {
            return false;
        };
        // SAFETY: sole BSP, IF=0, owned EDU configuration DWORD.
        unsafe {
            asm!("out dx, eax", in("dx") 0xcf8u16, in("eax") address,
                options(nostack, preserves_flags));
            asm!("out dx, eax", in("dx") 0xcfcu16, in("eax") value,
                options(nostack, preserves_flags));
        }
        true
    }
}

/// Retain one EDU register page using the EXISTING APIC window owner.
///
/// # Safety
/// Sole BSP, IF=0, no AP startup, dedicated QEMU TCG instance without an
/// IOMMU or passed-through devices. The permanent vector-0x50 handler is
/// already installed; xAPIC/UC PAT were validated by the caller. Slots 0/1
/// belong to APICs and slot 2 is vacant. No later Window owner is created.
/// The mapping and handler remain live even after a setup error.
pub(super) unsafe fn prepare(vm: &mut Window, destination: u8) -> Result<(), &'static str> {
    if !interrupts_disabled() || READY.load(Ordering::Acquire) {
        return Err("invalid initialization state");
    }
    let leaf = __cpuid_count(0x4000_0000, 0);
    let mut vendor = [0u8; 12];
    vendor[..4].copy_from_slice(&leaf.ebx.to_le_bytes());
    vendor[4..8].copy_from_slice(&leaf.ecx.to_le_bytes());
    vendor[8..].copy_from_slice(&leaf.edx.to_le_bytes());
    if __cpuid_count(1, 0).ecx & (1 << 31) == 0 || vendor != *b"TCGTCGTCGTCG" {
        return Err("MSI probe requires QEMU TCG");
    }
    let mut found = None;
    let mut duplicate = false;
    pci::scan_segment_zero(
        // SAFETY: same exclusive early BSP configuration access.
        |bdf, offset| unsafe { pci::read_legacy_dword(bdf, offset) },
        |device| {
            if device.vendor == EDU_ID as u16 && device.id == (EDU_ID >> 16) as u16 {
                if found.is_some() {
                    duplicate = true;
                }
                found = Some(device);
            }
        },
    );
    let device = found.ok_or("EDU device missing")?;
    if duplicate || device.header_type & 0x7f != 0 {
        return Err("EDU device identity is ambiguous");
    }
    let mut io = Config(device.bdf);
    let caps = pci_interrupts::discover(&mut |offset| io.read32(offset))
        .map_err(|_| "invalid EDU capabilities")?;
    let msi = caps.msi.ok_or("EDU has no MSI capability")?;
    if msi.control() & 1 != 0 || caps.msix.is_some() {
        return Err("EDU interrupt mode already owned or unexpected");
    }
    let raw_bar = io.read32(0x10).ok_or("EDU BAR unavailable")?;
    let physical = u64::from(raw_bar & !0xf);
    // EDU's documented BAR0 is a non-prefetchable 32-bit 1 MiB aperture.
    // Only its first register page is mapped, never its DMA buffer region.
    if raw_bar & 0xf != 0 || physical == 0 || !physical.is_multiple_of(1 << 20) {
        return Err("EDU BAR layout is unsupported");
    }
    // SAFETY: independently identified device; reject RAM/runtime aliases.
    if !unsafe { memory::external_mmio_page_is_safe(physical) } {
        return Err("EDU BAR conflicts with firmware memory ownership");
    }
    // SAFETY: same live window owner; first EDU register page is validated.
    let virtual_page =
        unsafe { vm.map_mmio_writable(2, physical) }.map_err(|_| "EDU register mapping failed")?;
    let base = usize::try_from(virtual_page).map_err(|_| "EDU virtual address")?;
    let command = io.read32(4).ok_or("EDU command unavailable")? as u16;
    // Enable memory decoding with INTx disabled, but keep bus mastering OFF
    // until MMIO device identity and quiescence have also been verified.
    pci_msi::write16_checked(&mut io, 4, (command | 0x402) & !4)
        .map_err(|_| "EDU memory decode readback")?;
    // SAFETY: documented read-only identification/status DWORDs in live UC map.
    let identification = unsafe { core::ptr::read_volatile(base as *const u32) };
    let pending = unsafe { core::ptr::read_volatile((base + STATUS) as *const u32) };
    let activity = unsafe { core::ptr::read_volatile((base + 0x20) as *const u32) };
    if identification & 0xffff != 0x00ed || pending != 0 || activity != 0 {
        return Err("EDU identity or source quiescence failed");
    }
    MMIO.store(base, Ordering::Release);
    BDF.store(
        (u32::from(device.bdf.bus) << 16)
            | (u32::from(device.bdf.device) << 8)
            | u32::from(device.bdf.function),
        Ordering::Release,
    );
    let message = Message::new(destination, VECTOR).map_err(|_| "invalid MSI destination")?;
    // MSI itself is a PCI memory-write transaction. No EDU DMA source,
    // destination, length or start register is accessed by this probe.
    pci_msi::write16_checked(&mut io, 4, command | 0x406)
        .map_err(|_| "EDU message-write permission")?;
    // SAFETY: owned quiescent EDU, installed permanent handler, physical
    // xAPIC target, IF=0, no IOMMU and retained register mapping/handler state.
    unsafe { pci_msi::enable_single(&mut io, caps, message) }
        .map_err(|_| "EDU MSI setup readback failed")?;
    READY.store(true, Ordering::Release);
    crate::debugcon::write("VIBRIX: native EDU MSI armed\r\n");
    Ok(())
}

pub(super) fn interrupt() {
    let base = MMIO.load(Ordering::Acquire);
    if base == 0 {
        UNEXPECTED.store(true, Ordering::Release);
    } else {
        // SAFETY: initialization publishes this retained UC page before MSI
        // enable. Only documented status/ack DWORDs are accessed by the ISR.
        let status = unsafe { core::ptr::read_volatile((base + STATUS) as *const u32) };
        unsafe { core::ptr::write_volatile((base + ACK) as *mut u32, status) };
        let remaining = unsafe { core::ptr::read_volatile((base + STATUS) as *const u32) };
        if status != 1 || remaining != 0 {
            UNEXPECTED.store(true, Ordering::Release);
        }
        DELIVERED.fetch_add(1, Ordering::Release);
    }
    // SAFETY: hardware delivery is enabled only after retained LAPIC setup.
    unsafe { apic::eoi() };
}

fn wait_for_count(expected: u64) -> Result<(), &'static str> {
    let start = irq::timer_ticks();
    for _ in 0..50_000_000 {
        if UNEXPECTED.load(Ordering::Acquire) {
            return Err("unexpected MSI source/status");
        }
        if DELIVERED.load(Ordering::Acquire) >= expected {
            return Ok(());
        }
        if irq::timer_ticks().wrapping_sub(start) > 200 {
            return Err("MSI delivery timed out");
        }
        core::hint::spin_loop();
    }
    Err("MSI bounded wait exhausted")
}

/// Request real device interrupts, verify ACK/EOI, then verify disabled silence.
///
/// # Safety
/// Same one-shot QEMU environment as prepare; permanent mappings and handler
/// retained. APIC/PIT setup succeeded and the sole BSP has IF=1. This function
/// briefly clears IF around serialized configuration changes; no other CPU,
/// driver, NMI path or IRQ handler may access PCI configuration.
pub(super) unsafe fn exercise() -> Result<(), &'static str> {
    if !READY.load(Ordering::Acquire)
        || interrupts_disabled()
        || EXERCISED.swap(true, Ordering::AcqRel)
    {
        return Err("MSI exercise state invalid");
    }
    let base = MMIO.load(Ordering::Acquire);
    for expected in 1..=2 {
        // SAFETY: owned EDU's documented IRQ-raise DWORD, not a DMA register.
        unsafe { core::ptr::write_volatile((base + RAISE) as *mut u32, 1) };
        wait_for_count(expected)?;
    }
    let key = BDF.load(Ordering::Acquire);
    let mut io = Config(pci::Bdf {
        bus: (key >> 16) as u8,
        device: (key >> 8) as u8,
        function: key as u8,
    });
    // SAFETY: BSP enters the documented configuration critical section.
    unsafe { asm!("cli", options(nostack)) };
    let disabled = (|| {
        let caps = pci_interrupts::discover(&mut |offset| io.read32(offset))
            .map_err(|_| "EDU capability reread failed")?;
        let msi = caps.msi.ok_or("EDU MSI disappeared")?;
        // SAFETY: same owned device, permanent ISR/mapping remain live.
        unsafe { pci_msi::disable_single(&mut io, msi) }.map_err(|_| "MSI disable readback")
    })();
    // SAFETY: all routes and handlers remain installed after disabling MSI.
    unsafe { asm!("sti", options(nostack)) };
    disabled?;
    // With MSI and INTx both disabled, the device may retain pending status
    // but must not deliver an interrupt. Verify over three actual PIT ticks.
    unsafe { core::ptr::write_volatile((base + RAISE) as *mut u32, 1) };
    let start = irq::timer_ticks();
    let mut elapsed = false;
    for _ in 0..50_000_000 {
        if irq::timer_ticks().wrapping_sub(start) >= 3 {
            elapsed = true;
            break;
        }
        core::hint::spin_loop();
    }
    let pending = unsafe { core::ptr::read_volatile((base + STATUS) as *const u32) };
    let count = DELIVERED.load(Ordering::Acquire);
    if !elapsed || count != 2 || pending != 1 || UNEXPECTED.load(Ordering::Acquire) {
        return Err("disabled MSI delivered or timer/status evidence missing");
    }
    // SAFETY: source is disabled and exclusively owned; clear test pending bit.
    unsafe { core::ptr::write_volatile((base + ACK) as *mut u32, 1) };
    if unsafe { core::ptr::read_volatile((base + STATUS) as *const u32) } != 0 {
        return Err("disabled EDU acknowledgement did not clear status");
    }
    // Leave this diagnostic device quiescent with message writes disabled.
    unsafe { asm!("cli", options(nostack)) };
    let stopped = (|| {
        let command = io.read32(4).ok_or("EDU command reread failed")? as u16;
        pci_msi::write16_checked(&mut io, 4, command & !4)
            .map_err(|_| "EDU bus-master disable readback")
    })();
    unsafe { asm!("sti", options(nostack)) };
    stopped?;
    crate::println!("kernel MSI: delivered=2 acknowledged=2 disabled=true bus_master=false");
    crate::debugcon::write("VIBRIX: native MSI repeated delivery verified\r\n");
    Ok(())
}
