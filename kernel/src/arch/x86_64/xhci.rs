//! Bounded native xHCI controller initialization for the early single-BSP kernel.
//!
//! This module intentionally stops before USB device enumeration. It proves
//! that one PCI-discovered xHCI controller can be reset, supplied with the
//! mandatory host-controller data structures, and transitioned to Running
//! after ExitBootServices.

use crate::{BootInfo, memory};
use core::arch::{asm, x86_64::__cpuid_count};

use super::pci::{self, Bar, Device};
use crate::memory::virtual_memory::Window;

const PAGE: u64 = 4096;
const IA32_PAT: u32 = 0x277;
const CAPLENGTH: usize = 0x00;
const HCSPARAMS1: usize = 0x04;
const HCSPARAMS2: usize = 0x08;
const HCCPARAMS1: usize = 0x10;
const DBOFF: usize = 0x14;
const RTSOFF: usize = 0x18;

const USBCMD: usize = 0x00;
const USBSTS: usize = 0x04;
const PAGESIZE: usize = 0x08;
const CRCR: usize = 0x18;
const DCBAAP: usize = 0x30;
const CONFIG: usize = 0x38;

const USBCMD_RUN: u32 = 1 << 0;
const USBCMD_HCRST: u32 = 1 << 1;
const USBSTS_HCH: u32 = 1 << 0;
const USBSTS_CNR: u32 = 1 << 11;

const PORTSC_BASE: usize = 0x400;
const PORTSC_STRIDE: usize = 0x10;
const PORTSC_CCS: u32 = 1 << 0;
const PORTSC_PED: u32 = 1 << 1;
const PORTSC_PR: u32 = 1 << 4;
const PORTSC_PP: u32 = 1 << 9;
const PORTSC_SPEED_SHIFT: u32 = 10;
const PORTSC_SPEED_MASK: u32 = 0xf << PORTSC_SPEED_SHIFT;
const PORTSC_RW1C: u32 = 0x7f << 17;

const TRB_BYTES: usize = 16;
const TRB_CYCLE: u32 = 1;
const TRB_IOC: u32 = 1 << 5;
const TRB_IDT: u32 = 1 << 6;
const TRB_DIR_IN: u32 = 1 << 16;
const TRB_TYPE_SHIFT: u32 = 10;
const TRB_TYPE_ENABLE_SLOT: u32 = 9;
const TRB_TYPE_ADDRESS_DEVICE: u32 = 11;
const TRB_TYPE_SETUP_STAGE: u32 = 2;
const TRB_TYPE_DATA_STAGE: u32 = 3;
const TRB_TYPE_STATUS_STAGE: u32 = 4;
const TRB_TYPE_TRANSFER_EVENT: u32 = 32;
const TRB_TYPE_COMMAND_COMPLETION: u32 = 33;
const COMPLETION_SUCCESS: u8 = 1;
const COMPLETION_SHORT_PACKET: u8 = 13;

const IMAN: usize = 0x00;
const ERSTSZ: usize = 0x08;
const ERSTBA: usize = 0x10;
const ERDP: usize = 0x18;
const INTERRUPTER_ZERO: usize = 0x20;

const EVENT_RING_TRBS: u32 = 256;
const WAIT_SPINS: usize = 4_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitError {
    NotFound,
    InvalidBar,
    UnsafeMmio,
    UnsupportedPat,
    InvalidCapabilityHeader,
    UnsupportedPageSize,
    RegisterReadback,
    UnsupportedScratchpads,
    Mapping,
    OutOfFrames,
    HaltTimeout,
    ResetTimeout,
    ReadyTimeout,
    RunTimeout,
    NoConnectedDevice,
    PortResetTimeout,
    UnsupportedContextSize,
    InvalidControllerState,
    CommandTimeout,
    CommandFailed(u8),
    TransferTimeout,
    TransferFailed(u8),
    DescriptorMalformed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Summary {
    pub bdf: pci::Bdf,
    pub bar: u64,
    pub version: u16,
    pub max_slots: u8,
    pub max_ports: u8,
    pub doorbell_offset: u32,
    pub runtime_offset: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsbDeviceSummary {
    pub port: u8,
    pub slot_id: u8,
    pub speed_id: u8,
    pub vendor_id: u16,
    pub product_id: u16,
    pub class: u8,
    pub subclass: u8,
    pub protocol: u8,
    pub max_packet_size0: u8,
}

pub struct Capability {
    pub cap_length: u8,
    pub version: u16,
    pub max_slots: u8,
    pub max_ports: u8,
    pub scratchpads: u16,
    pub doorbell_offset: u32,
    pub runtime_offset: u32,
}

pub fn parse_capability(
    cap_length: u8,
    version: u16,
    hcs1: u32,
    hcs2: u32,
    dboff: u32,
    rtsoff: u32,
) -> Result<Capability, InitError> {
    let scratch_hi = ((hcs2 >> 21) & 0x1f) as u16;
    let scratch_lo = ((hcs2 >> 27) & 0x1f) as u16;
    let scratchpads = (scratch_hi << 5) | scratch_lo;
    let max_slots = (hcs1 & 0xff) as u8;
    let max_ports = ((hcs1 >> 24) & 0xff) as u8;
    let doorbell_offset = dboff & !0x3;
    let runtime_offset = rtsoff & !0x1f;

    if !(0x20..=0xff).contains(&cap_length)
        || version < 0x0090
        || max_slots == 0
        || max_ports == 0
        || usize::from(cap_length) + CONFIG + 4 > PAGE as usize
        || doorbell_offset < u32::from(cap_length)
        || runtime_offset < u32::from(cap_length)
    {
        return Err(InitError::InvalidCapabilityHeader);
    }

    Ok(Capability {
        cap_length,
        version,
        max_slots,
        max_ports,
        scratchpads,
        doorbell_offset,
        runtime_offset,
    })
}

fn trb_control(kind: u32) -> u32 {
    TRB_CYCLE | (kind << TRB_TYPE_SHIFT)
}

fn trb_type(control: u32) -> u32 {
    (control >> TRB_TYPE_SHIFT) & 0x3f
}

fn completion_code(status: u32) -> u8 {
    (status >> 24) as u8
}

fn event_slot_id(control: u32) -> u8 {
    (control >> 24) as u8
}

fn port_speed(portsc: u32) -> u8 {
    ((portsc & PORTSC_SPEED_MASK) >> PORTSC_SPEED_SHIFT) as u8
}

fn endpoint0_packet_size(speed_id: u8) -> Option<u16> {
    match speed_id {
        // Full- and low-speed devices begin enumeration with 8-byte EP0.
        1 | 2 => Some(8),
        // High-speed control endpoints use 64-byte max packets.
        3 => Some(64),
        // SuperSpeed and SuperSpeedPlus device contexts encode 512 bytes.
        4 | 5 => Some(512),
        _ => None,
    }
}

fn setup_get_device_descriptor() -> [u32; 4] {
    // bmRequestType=IN|standard|device, bRequest=GET_DESCRIPTOR,
    // wValue=DEVICE<<8 | index0, wIndex=0, wLength=18.
    [
        0x0100_0680,
        18u32 << 16,
        8,
        trb_control(TRB_TYPE_SETUP_STAGE) | TRB_IDT | (3 << 16),
    ]
}

#[cfg(target_os = "none")]
unsafe fn rdmsr(msr: u32) -> u64 {
    let low: u32;
    let high: u32;
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

#[cfg(target_os = "none")]
unsafe fn pat_index_three_is_uc() -> bool {
    if __cpuid_count(1, 0).edx & (1 << 16) == 0 {
        return false;
    }
    ((unsafe { rdmsr(IA32_PAT) } >> 24) & 0xff) == 0
}

#[cfg(target_os = "none")]
unsafe fn read_bar0(device: Device) -> Result<u64, InitError> {
    if !device.is_xhci() || device.bar_slots() == 0 {
        return Err(InitError::NotFound);
    }
    let command_status = unsafe { pci::read_legacy_dword(device.bdf, 0x04) };
    if command_status & (1 << 1) == 0 {
        return Err(InitError::InvalidBar);
    }
    let low = unsafe { pci::read_legacy_dword(device.bdf, 0x10) };
    let high = if low & 1 == 0 && (low >> 1) & 3 == 2 {
        Some(unsafe { pci::read_legacy_dword(device.bdf, 0x14) })
    } else {
        None
    };
    match pci::decode_bar(low, high)
        .map_err(|_| InitError::InvalidBar)?
        .0
    {
        Some(Bar::Memory32 { physical, .. }) => Ok(u64::from(physical)),
        Some(Bar::Memory64 { physical, .. }) => Ok(physical),
        _ => Err(InitError::InvalidBar),
    }
}

#[cfg(target_os = "none")]
unsafe fn zero_frame(vm: &mut Window, slot: usize, frame: u64) -> Result<u64, InitError> {
    let virtual_address = unsafe { vm.map(slot, frame, true) }.map_err(|_| InitError::Mapping)?;
    let base = usize::try_from(virtual_address).map_err(|_| InitError::Mapping)?;
    for offset in (0..PAGE as usize).step_by(core::mem::size_of::<u64>()) {
        unsafe { core::ptr::write_volatile((base + offset) as *mut u64, 0) };
    }
    Ok(virtual_address)
}

#[cfg(target_os = "none")]
unsafe fn unmap_ram(vm: &mut Window, slot: usize) -> Result<(), InitError> {
    unsafe { vm.unmap(slot) }.map_err(|_| InitError::Mapping)?;
    Ok(())
}

#[cfg(target_os = "none")]
unsafe fn read32(base: usize, offset: usize) -> u32 {
    unsafe { core::ptr::read_volatile((base + offset) as *const u32) }
}

#[cfg(target_os = "none")]
unsafe fn write32(base: usize, offset: usize, value: u32) {
    unsafe { core::ptr::write_volatile((base + offset) as *mut u32, value) };
}

#[cfg(target_os = "none")]
unsafe fn read64(base: usize, offset: usize) -> u64 {
    unsafe { core::ptr::read_volatile((base + offset) as *const u64) }
}

#[cfg(target_os = "none")]
unsafe fn write64(base: usize, offset: usize, value: u64) {
    unsafe { core::ptr::write_volatile((base + offset) as *mut u64, value) };
}

#[cfg(target_os = "none")]
fn wait_until(mut predicate: impl FnMut() -> bool) -> bool {
    for _ in 0..WAIT_SPINS {
        if predicate() {
            return true;
        }
        core::hint::spin_loop();
    }
    false
}

/// Reset and start one PCI-discovered xHCI controller with a minimal command
/// ring, event ring, ERST and DCBAA. No USB commands are submitted.
///
/// # Safety
/// Sole BSP, IF=0, final UEFI map retained, and the BootInfo scratch window is
/// empty. No other PCI/MMIO owner may touch this controller concurrently.
#[cfg(target_os = "none")]
pub unsafe fn initialize(info: &BootInfo) -> Result<Summary, InitError> {
    if !unsafe { pat_index_three_is_uc() } {
        return Err(InitError::UnsupportedPat);
    }

    let mut found = None;
    let _ = unsafe {
        pci::discover_legacy_segment_zero(|device| {
            if found.is_none() && device.is_xhci() {
                found = Some(device);
            }
        })
    };
    let device = found.ok_or(InitError::NotFound)?;
    let bar = unsafe { read_bar0(device) }?;
    let mmio_page = bar & !(PAGE - 1);
    let bar_offset = usize::try_from(bar - mmio_page).map_err(|_| InitError::InvalidBar)?;
    if bar_offset + 0x40 > PAGE as usize
        || !unsafe { memory::external_mmio_page_is_safe(mmio_page) }
    {
        return Err(InitError::UnsafeMmio);
    }

    let mut vm = unsafe { memory::virtual_memory::runtime::from_boot_info(info) }
        .map_err(|_| InitError::Mapping)?;
    let cap_virtual =
        unsafe { vm.map_mmio_writable(0, mmio_page) }.map_err(|_| InitError::Mapping)?;
    let cap_base = usize::try_from(cap_virtual)
        .map_err(|_| InitError::Mapping)?
        .checked_add(bar_offset)
        .ok_or(InitError::Mapping)?;

    // CAPLENGTH and HCIVERSION share the first aligned capability dword.
    // Read it once at the controller's natural register width: some MMIO
    // implementations do not preserve sub-dword reads consistently.
    let cap_header = unsafe { read32(cap_base, CAPLENGTH) };
    let cap_length = (cap_header & 0xff) as u8;
    let version = (cap_header >> 16) as u16;
    let hcs1 = unsafe { read32(cap_base, HCSPARAMS1) };
    let hcs2 = unsafe { read32(cap_base, HCSPARAMS2) };
    let dboff = unsafe { read32(cap_base, DBOFF) };
    let rtsoff = unsafe { read32(cap_base, RTSOFF) };
    crate::println!(
        "kernel xHCI caps: bdf={:02x}:{:02x}.{} bar={:#x} caplen={:#x} version={:#x} hcs1={:#x} hcs2={:#x} dboff={:#x} rtsoff={:#x}",
        device.bdf.bus,
        device.bdf.device,
        device.bdf.function,
        bar,
        cap_length,
        version,
        hcs1,
        hcs2,
        dboff,
        rtsoff
    );
    let capability = parse_capability(cap_length, version, hcs1, hcs2, dboff, rtsoff)?;
    if capability.scratchpads != 0 {
        let _ = unsafe { vm.unmap(0) };
        return Err(InitError::UnsupportedScratchpads);
    }

    let op_base = cap_base + usize::from(capability.cap_length);
    let command = unsafe { read32(op_base, USBCMD) } & !USBCMD_RUN;
    unsafe { write32(op_base, USBCMD, command) };
    if !wait_until(|| unsafe { read32(op_base, USBSTS) } & USBSTS_HCH != 0) {
        let _ = unsafe { vm.unmap(0) };
        return Err(InitError::HaltTimeout);
    }

    unsafe { write32(op_base, USBCMD, command | USBCMD_HCRST) };
    if !wait_until(|| unsafe { read32(op_base, USBCMD) } & USBCMD_HCRST == 0) {
        let _ = unsafe { vm.unmap(0) };
        return Err(InitError::ResetTimeout);
    }
    if !wait_until(|| unsafe { read32(op_base, USBSTS) } & USBSTS_CNR == 0) {
        let _ = unsafe { vm.unmap(0) };
        return Err(InitError::ReadyTimeout);
    }
    if unsafe { read32(op_base, PAGESIZE) } & 1 == 0 {
        let _ = unsafe { vm.unmap(0) };
        return Err(InitError::UnsupportedPageSize);
    }

    let dcbaa = unsafe { memory::allocate_frame() }.ok_or(InitError::OutOfFrames)?;
    let command_ring = unsafe { memory::allocate_frame() }.ok_or(InitError::OutOfFrames)?;
    let event_ring = unsafe { memory::allocate_frame() }.ok_or(InitError::OutOfFrames)?;
    let erst = unsafe { memory::allocate_frame() }.ok_or(InitError::OutOfFrames)?;

    unsafe { zero_frame(&mut vm, 1, dcbaa) }?;
    unsafe { unmap_ram(&mut vm, 1) }?;
    unsafe { zero_frame(&mut vm, 1, command_ring) }?;
    unsafe { unmap_ram(&mut vm, 1) }?;
    unsafe { zero_frame(&mut vm, 1, event_ring) }?;
    unsafe { unmap_ram(&mut vm, 1) }?;
    let erst_virtual = unsafe { zero_frame(&mut vm, 1, erst) }?;
    let erst_base = usize::try_from(erst_virtual).map_err(|_| InitError::Mapping)?;
    unsafe {
        write64(erst_base, 0, event_ring);
        write32(erst_base, 8, EVENT_RING_TRBS);
        write32(erst_base, 12, 0);
    }
    unsafe { unmap_ram(&mut vm, 1) }?;

    let runtime_physical = bar
        .checked_add(u64::from(capability.runtime_offset))
        .and_then(|value| value.checked_add(INTERRUPTER_ZERO as u64))
        .ok_or(InitError::InvalidCapabilityHeader)?;
    let runtime_page = runtime_physical & !(PAGE - 1);
    let runtime_in_page =
        usize::try_from(runtime_physical - runtime_page).map_err(|_| InitError::Mapping)?;
    if runtime_in_page + ERDP + 8 > PAGE as usize
        || !unsafe { memory::external_mmio_page_is_safe(runtime_page) }
    {
        let _ = unsafe { vm.unmap(0) };
        return Err(InitError::UnsafeMmio);
    }
    let runtime_virtual =
        unsafe { vm.map_mmio_writable(2, runtime_page) }.map_err(|_| InitError::Mapping)?;
    let runtime_base = usize::try_from(runtime_virtual)
        .map_err(|_| InitError::Mapping)?
        .checked_add(runtime_in_page)
        .ok_or(InitError::Mapping)?;

    unsafe {
        write64(op_base, DCBAAP, dcbaa);
        write64(op_base, CRCR, command_ring | 1);
        write32(op_base, CONFIG, u32::from(capability.max_slots.min(8)));
        write32(runtime_base, IMAN, 0);
        write32(runtime_base, ERSTSZ, 1);
        write64(runtime_base, ERSTBA, erst);
        write64(runtime_base, ERDP, event_ring);
    }

    let published_dcbaa = unsafe { read64(op_base, DCBAAP) } & !0x3f;
    let published_crcr = unsafe { read64(op_base, CRCR) } & !0x3f;
    if published_dcbaa != dcbaa || published_crcr != command_ring {
        let _ = unsafe { vm.unmap(2) };
        let _ = unsafe { vm.unmap(0) };
        return Err(InitError::RegisterReadback);
    }

    unsafe { write32(op_base, USBCMD, USBCMD_RUN) };
    if !wait_until(|| unsafe { read32(op_base, USBSTS) } & USBSTS_HCH == 0) {
        let _ = unsafe { vm.unmap(2) };
        let _ = unsafe { vm.unmap(0) };
        return Err(InitError::RunTimeout);
    }

    unsafe { vm.unmap(2) }.map_err(|_| InitError::Mapping)?;
    unsafe { vm.unmap(0) }.map_err(|_| InitError::Mapping)?;

    Ok(Summary {
        bdf: device.bdf,
        bar,
        version: capability.version,
        max_slots: capability.max_slots,
        max_ports: capability.max_ports,
        doorbell_offset: capability.doorbell_offset,
        runtime_offset: capability.runtime_offset,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_capability_and_masks_array_offsets() {
        let cap = parse_capability(0x40, 0x0110, 32 | (8 << 24), 0, 0x1003, 0x200f).unwrap();
        assert_eq!(cap.cap_length, 0x40);
        assert_eq!(cap.version, 0x0110);
        assert_eq!(cap.max_slots, 32);
        assert_eq!(cap.max_ports, 8);
        assert_eq!(cap.scratchpads, 0);
        assert_eq!(cap.doorbell_offset, 0x1000);
        assert_eq!(cap.runtime_offset, 0x2000);
    }

    #[test]
    fn parses_split_scratchpad_count() {
        let cap = parse_capability(
            0x40,
            0x0100,
            8 | (4 << 24),
            (2 << 21) | (3 << 27),
            0x1000,
            0x2000,
        )
        .unwrap();
        assert_eq!(cap.scratchpads, 67);
    }

    #[test]
    fn rejects_unusable_capability_blocks() {
        assert_eq!(
            parse_capability(0x10, 0x0110, 1 | (1 << 24), 0, 0x1000, 0x2000),
            Err(InitError::InvalidCapabilityHeader)
        );
        assert_eq!(
            parse_capability(0x40, 0x0110, 0, 0, 0x1000, 0x2000),
            Err(InitError::InvalidCapabilityHeader)
        );
        assert_eq!(
            parse_capability(0x40, 0x0110, 1 | (1 << 24), 0, 0x20, 0x2000),
            Err(InitError::InvalidCapabilityHeader)
        );
    }
}
