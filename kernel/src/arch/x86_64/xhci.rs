//! Bounded native xHCI controller initialization for the early single-BSP kernel.
//!
//! This module owns the bounded early xHCI proof path: controller startup,
 //! one directly attached device, and USB2 hub class control/port management.
 //! HID and mass-storage endpoint drivers remain later milestones.

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
    NotHub,
    NoDownstreamDevice,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsbHubSummary {
    pub root_port: u8,
    pub slot_id: u8,
    pub downstream_ports: u8,
    pub child_port: u8,
    pub child_status: u16,
    pub power_good_units: u8,
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

fn setup_packet(
    request_type: u8,
    request: u8,
    value: u16,
    index: u16,
    length: u16,
    transfer_type: u32,
) -> [u32; 4] {
    [
        u32::from(request_type) | (u32::from(request) << 8) | (u32::from(value) << 16),
        u32::from(index) | (u32::from(length) << 16),
        8,
        trb_control(TRB_TYPE_SETUP_STAGE) | TRB_IDT | (transfer_type << 16),
    ]
}

fn setup_get_device_descriptor() -> [u32; 4] {
    // IN | standard | device, GET_DESCRIPTOR(Device), 18 bytes.
    setup_packet(0x80, 0x06, 0x0100, 0, 18, 3)
}

fn setup_get_hub_descriptor() -> [u32; 4] {
    // IN | class | device, GET_DESCRIPTOR(Hub), bounded 9-byte USB2 header.
    setup_packet(0xa0, 0x06, 0x2900, 0, 9, 3)
}

fn setup_set_port_feature(port: u8, feature: u16) -> [u32; 4] {
    // OUT | class | other, SET_FEATURE(feature), wIndex=port, no data.
    setup_packet(0x23, 0x03, feature, u16::from(port), 0, 0)
}

fn setup_get_port_status(port: u8) -> [u32; 4] {
    // IN | class | other, GET_STATUS, four-byte port status/change payload.
    setup_packet(0xa3, 0x00, 0, u16::from(port), 4, 3)
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
unsafe fn read8(base: usize, offset: usize) -> u8 {
    unsafe { core::ptr::read_volatile((base + offset) as *const u8) }
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

#[cfg(target_os = "none")]
unsafe fn write_trb(base: usize, index: usize, words: [u32; 4]) {
    let offset = index * TRB_BYTES;
    unsafe {
        write32(base, offset, words[0]);
        write32(base, offset + 4, words[1]);
        write32(base, offset + 8, words[2]);
        write32(base, offset + 12, words[3]);
    }
}

#[cfg(target_os = "none")]
unsafe fn wait_for_event_type(
    event_base: usize,
    event_physical: u64,
    runtime_base: usize,
    event_index: &mut usize,
    event_cycle: &mut bool,
    wanted_type: u32,
) -> Option<[u32; 4]> {
    for _ in 0..WAIT_SPINS {
        let offset = *event_index * TRB_BYTES;
        let control = unsafe { read32(event_base, offset + 12) };
        let ready = (control & TRB_CYCLE != 0) == *event_cycle;
        if !ready {
            core::hint::spin_loop();
            continue;
        }

        let event = unsafe {
            [
                read32(event_base, offset),
                read32(event_base, offset + 4),
                read32(event_base, offset + 8),
                control,
            ]
        };
        *event_index += 1;
        if *event_index == EVENT_RING_TRBS as usize {
            *event_index = 0;
            *event_cycle = !*event_cycle;
        }
        let next = event_physical + (*event_index * TRB_BYTES) as u64;
        // EHB=1 acknowledges any pending event-handler-busy state while
        // publishing the next dequeue pointer.
        unsafe { write64(runtime_base, ERDP, next | (1 << 3)) };

        if trb_type(event[3]) == wanted_type {
            return Some(event);
        }
    }
    None
}

#[cfg(target_os = "none")]
struct RingCursor {
    command_base: usize,
    command_index: usize,
    doorbell_base: usize,
    event_base: usize,
    event_physical: u64,
    runtime_base: usize,
    event_index: usize,
    event_cycle: bool,
}

#[cfg(target_os = "none")]
impl RingCursor {
    unsafe fn submit_command(&mut self, words: [u32; 4]) -> Result<[u32; 4], InitError> {
        if self.command_index >= 32 {
            return Err(InitError::InvalidControllerState);
        }
        unsafe { write_trb(self.command_base, self.command_index, words) };
        self.command_index += 1;
        core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::Release);
        unsafe { write32(self.doorbell_base, 0, 0) };
        let event = unsafe {
            wait_for_event_type(
                self.event_base,
                self.event_physical,
                self.runtime_base,
                &mut self.event_index,
                &mut self.event_cycle,
                TRB_TYPE_COMMAND_COMPLETION,
            )
        }
        .ok_or(InitError::CommandTimeout)?;
        let code = completion_code(event[2]);
        if code != COMPLETION_SUCCESS {
            return Err(InitError::CommandFailed(code));
        }
        Ok(event)
    }

    unsafe fn control_in(
        &mut self,
        transfer_base: usize,
        transfer_index: &mut usize,
        slot_id: u8,
        setup: [u32; 4],
        buffer: u64,
        length: u32,
    ) -> Result<u32, InitError> {
        if *transfer_index + 3 > 256 {
            return Err(InitError::InvalidControllerState);
        }
        unsafe {
            write_trb(transfer_base, *transfer_index, setup);
            write_trb(
                transfer_base,
                *transfer_index + 1,
                [
                    buffer as u32,
                    (buffer >> 32) as u32,
                    length,
                    trb_control(TRB_TYPE_DATA_STAGE) | TRB_DIR_IN,
                ],
            );
            write_trb(
                transfer_base,
                *transfer_index + 2,
                [0, 0, 0, trb_control(TRB_TYPE_STATUS_STAGE) | TRB_IOC],
            );
        }
        *transfer_index += 3;
        core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::Release);
        unsafe { write32(self.doorbell_base, usize::from(slot_id) * 4, 1) };

        let event = unsafe {
            wait_for_event_type(
                self.event_base,
                self.event_physical,
                self.runtime_base,
                &mut self.event_index,
                &mut self.event_cycle,
                TRB_TYPE_TRANSFER_EVENT,
            )
        }
        .ok_or(InitError::TransferTimeout)?;
        let code = completion_code(event[2]);
        if code != COMPLETION_SUCCESS && code != COMPLETION_SHORT_PACKET {
            return Err(InitError::TransferFailed(code));
        }
        if event_slot_id(event[3]) != slot_id || ((event[3] >> 16) & 0x1f) != 1 {
            return Err(InitError::InvalidControllerState);
        }
        let remaining = event[2] & 0x00ff_ffff;
        if remaining > length {
            return Err(InitError::DescriptorMalformed);
        }
        Ok(length - remaining)
    }

    unsafe fn control_no_data(
        &mut self,
        transfer_base: usize,
        transfer_index: &mut usize,
        slot_id: u8,
        setup: [u32; 4],
    ) -> Result<(), InitError> {
        if *transfer_index + 2 > 256 {
            return Err(InitError::InvalidControllerState);
        }
        unsafe {
            write_trb(transfer_base, *transfer_index, setup);
            write_trb(
                transfer_base,
                *transfer_index + 1,
                [
                    0,
                    0,
                    0,
                    trb_control(TRB_TYPE_STATUS_STAGE) | TRB_DIR_IN | TRB_IOC,
                ],
            );
        }
        *transfer_index += 2;
        core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::Release);
        unsafe { write32(self.doorbell_base, usize::from(slot_id) * 4, 1) };

        let event = unsafe {
            wait_for_event_type(
                self.event_base,
                self.event_physical,
                self.runtime_base,
                &mut self.event_index,
                &mut self.event_cycle,
                TRB_TYPE_TRANSFER_EVENT,
            )
        }
        .ok_or(InitError::TransferTimeout)?;
        let code = completion_code(event[2]);
        if code != COMPLETION_SUCCESS {
            return Err(InitError::TransferFailed(code));
        }
        if event_slot_id(event[3]) != slot_id || ((event[3] >> 16) & 0x1f) != 1 {
            return Err(InitError::InvalidControllerState);
        }
        Ok(())
    }
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

/// Enumerate one directly attached root-port USB device and read enough of
/// its standard device descriptor to prove real xHCI command and control
/// transfers. Hub traversal is intentionally outside this bounded milestone.
///
/// # Safety
/// Same single-BSP/IF=0 ownership contract as `initialize`. This function
/// resets and exclusively owns the first PCI-discovered xHCI controller for the
/// duration of the probe and allocates DMA frames that remain kernel-owned.
#[cfg(target_os = "none")]
unsafe fn enumerate_first_device_inner(
    info: &BootInfo,
    inspect_hub: bool,
) -> Result<(UsbDeviceSummary, Option<UsbHubSummary>), InitError> {
    let initialized = unsafe { initialize(info) }?;

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
    if bar != initialized.bar {
        return Err(InitError::InvalidControllerState);
    }

    let mmio_page = bar & !(PAGE - 1);
    let bar_offset = usize::try_from(bar - mmio_page).map_err(|_| InitError::InvalidBar)?;
    if !unsafe { memory::external_mmio_page_is_safe(mmio_page) } {
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

    let cap_header = unsafe { read32(cap_base, CAPLENGTH) };
    let cap_length = (cap_header & 0xff) as u8;
    let version = (cap_header >> 16) as u16;
    let hcs1 = unsafe { read32(cap_base, HCSPARAMS1) };
    let hcs2 = unsafe { read32(cap_base, HCSPARAMS2) };
    let hcc1 = unsafe { read32(cap_base, HCCPARAMS1) };
    let capability = parse_capability(
        cap_length,
        version,
        hcs1,
        hcs2,
        unsafe { read32(cap_base, DBOFF) },
        unsafe { read32(cap_base, RTSOFF) },
    )?;
    // CSZ=1 selects 64-byte contexts. The first enumeration proof deliberately
    // implements only the mandatory 32-byte layout used by QEMU q35.
    if hcc1 & (1 << 2) != 0 {
        return Err(InitError::UnsupportedContextSize);
    }
    let op_base = cap_base + usize::from(capability.cap_length);

    let runtime_physical = bar
        .checked_add(u64::from(capability.runtime_offset))
        .and_then(|value| value.checked_add(INTERRUPTER_ZERO as u64))
        .ok_or(InitError::InvalidCapabilityHeader)?;
    let runtime_page = runtime_physical & !(PAGE - 1);
    let runtime_offset =
        usize::try_from(runtime_physical - runtime_page).map_err(|_| InitError::Mapping)?;
    if !unsafe { memory::external_mmio_page_is_safe(runtime_page) } {
        return Err(InitError::UnsafeMmio);
    }
    let runtime_virtual =
        unsafe { vm.map_mmio_writable(2, runtime_page) }.map_err(|_| InitError::Mapping)?;
    let runtime_base = usize::try_from(runtime_virtual)
        .map_err(|_| InitError::Mapping)?
        .checked_add(runtime_offset)
        .ok_or(InitError::Mapping)?;

    let doorbell_physical = bar
        .checked_add(u64::from(capability.doorbell_offset))
        .ok_or(InitError::InvalidCapabilityHeader)?;
    let doorbell_page = doorbell_physical & !(PAGE - 1);
    let doorbell_offset =
        usize::try_from(doorbell_physical - doorbell_page).map_err(|_| InitError::Mapping)?;
    if !unsafe { memory::external_mmio_page_is_safe(doorbell_page) } {
        return Err(InitError::UnsafeMmio);
    }
    let doorbell_virtual =
        unsafe { vm.map_mmio_writable(3, doorbell_page) }.map_err(|_| InitError::Mapping)?;
    let doorbell_base = usize::try_from(doorbell_virtual)
        .map_err(|_| InitError::Mapping)?
        .checked_add(doorbell_offset)
        .ok_or(InitError::Mapping)?;

    let dcbaa = unsafe { read64(op_base, DCBAAP) } & !0x3f;
    let command_ring = unsafe { read64(op_base, CRCR) } & !0x3f;
    let erst = unsafe { read64(runtime_base, ERSTBA) } & !0x3f;
    if dcbaa == 0 || command_ring == 0 || erst == 0 {
        return Err(InitError::InvalidControllerState);
    }

    let erst_virtual = unsafe { vm.map(1, erst, true) }.map_err(|_| InitError::Mapping)?;
    let erst_base = usize::try_from(erst_virtual).map_err(|_| InitError::Mapping)?;
    let event_ring = unsafe { read64(erst_base, 0) } & !0x3f;
    unsafe { vm.unmap(1) }.map_err(|_| InitError::Mapping)?;
    if event_ring == 0 {
        return Err(InitError::InvalidControllerState);
    }

    let command_virtual =
        unsafe { vm.map(4, command_ring, true) }.map_err(|_| InitError::Mapping)?;
    let command_base = usize::try_from(command_virtual).map_err(|_| InitError::Mapping)?;
    let event_virtual = unsafe { vm.map(5, event_ring, true) }.map_err(|_| InitError::Mapping)?;
    let event_base = usize::try_from(event_virtual).map_err(|_| InitError::Mapping)?;
    let dcbaa_virtual = unsafe { vm.map(6, dcbaa, true) }.map_err(|_| InitError::Mapping)?;
    let dcbaa_base = usize::try_from(dcbaa_virtual).map_err(|_| InitError::Mapping)?;

    let mut selected = None;
    for port in 1..=capability.max_ports {
        let offset = PORTSC_BASE + (usize::from(port) - 1) * PORTSC_STRIDE;
        let portsc = unsafe { read32(op_base, offset) };
        if portsc & PORTSC_CCS != 0 {
            selected = Some((port, offset, portsc));
            break;
        }
    }
    let (port, port_offset, initial_portsc) = selected.ok_or(InitError::NoConnectedDevice)?;

    // Clear no RW1C status bits while requesting a normal USB2-style port
    // reset. QEMU's directly attached keyboard is full-speed and follows this
    // path; hub and SuperSpeed warm-reset handling remain later work.
    let reset = (initial_portsc & PORTSC_PP) | PORTSC_PR;
    unsafe { write32(op_base, port_offset, reset) };
    if !wait_until(|| {
        let value = unsafe { read32(op_base, port_offset) };
        value & PORTSC_CCS != 0 && value & PORTSC_PR == 0 && value & PORTSC_PED != 0
    }) {
        return Err(InitError::PortResetTimeout);
    }
    let portsc = unsafe { read32(op_base, port_offset) };
    // Acknowledge reset/change status without disturbing live port state.
    unsafe {
        write32(
            op_base,
            port_offset,
            (portsc & PORTSC_PP) | (portsc & PORTSC_RW1C),
        )
    };
    let speed_id = port_speed(portsc);
    let max_packet_size = endpoint0_packet_size(speed_id).ok_or(InitError::DescriptorMalformed)?;

    let mut rings = RingCursor {
        command_base,
        command_index: 0,
        doorbell_base,
        event_base,
        event_physical: event_ring,
        runtime_base,
        event_index: 0,
        event_cycle: true,
    };
    let enable_event =
        unsafe { rings.submit_command([0, 0, 0, trb_control(TRB_TYPE_ENABLE_SLOT)]) }?;
    let slot_id = event_slot_id(enable_event[3]);
    if slot_id == 0 || slot_id > capability.max_slots.min(8) {
        return Err(InitError::InvalidControllerState);
    }

    let device_context = unsafe { memory::allocate_frame() }.ok_or(InitError::OutOfFrames)?;
    let input_context = unsafe { memory::allocate_frame() }.ok_or(InitError::OutOfFrames)?;
    let transfer_ring = unsafe { memory::allocate_frame() }.ok_or(InitError::OutOfFrames)?;
    let descriptor_buffer = unsafe { memory::allocate_frame() }.ok_or(InitError::OutOfFrames)?;

    unsafe { zero_frame(&mut vm, 7, device_context) }?;
    unsafe { unmap_ram(&mut vm, 7) }?;
    let input_virtual = unsafe { zero_frame(&mut vm, 7, input_context) }?;
    let input_base = usize::try_from(input_virtual).map_err(|_| InitError::Mapping)?;
    let transfer_virtual = unsafe { zero_frame(&mut vm, 8, transfer_ring) }?;
    let transfer_base = usize::try_from(transfer_virtual).map_err(|_| InitError::Mapping)?;
    let descriptor_virtual = unsafe { zero_frame(&mut vm, 9, descriptor_buffer) }?;
    let descriptor_base = usize::try_from(descriptor_virtual).map_err(|_| InitError::Mapping)?;

    unsafe { write64(dcbaa_base, usize::from(slot_id) * 8, device_context) };

    // 32-byte input context: input-control, slot, endpoint-0.
    unsafe {
        write32(input_base, 4, 0x3); // add slot context + endpoint context 0
        write32(
            input_base,
            32,
            (u32::from(speed_id) << 20) | (1 << 27), // Context Entries=1
        );
        write32(input_base, 36, u32::from(port) << 16);
        write32(
            input_base,
            64 + 4,
            (3 << 1) | (4 << 3) | (u32::from(max_packet_size) << 16),
        );
        write64(input_base, 64 + 8, transfer_ring | 1); // DCS=1
        write32(input_base, 64 + 16, 8); // average TRB length
    }
    core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::Release);

    let address_event = unsafe {
        rings.submit_command([
            input_context as u32,
            (input_context >> 32) as u32,
            0,
            trb_control(TRB_TYPE_ADDRESS_DEVICE) | (u32::from(slot_id) << 24),
        ])
    }?;
    if event_slot_id(address_event[3]) != slot_id {
        return Err(InitError::InvalidControllerState);
    }
    unsafe { unmap_ram(&mut vm, 7) }?;

    let mut transfer_index = 0usize;
    let transferred = unsafe {
        rings.control_in(
            transfer_base,
            &mut transfer_index,
            slot_id,
            setup_get_device_descriptor(),
            descriptor_buffer,
            18,
        )
    }?;
    if transferred < 12 {
        return Err(InitError::DescriptorMalformed);
    }

    let length = unsafe { read8(descriptor_base, 0) };
    let descriptor_type = unsafe { read8(descriptor_base, 1) };
    if length < 18 || descriptor_type != 1 {
        return Err(InitError::DescriptorMalformed);
    }
    let class = unsafe { read8(descriptor_base, 4) };
    let subclass = unsafe { read8(descriptor_base, 5) };
    let protocol = unsafe { read8(descriptor_base, 6) };
    let max_packet_size0 = unsafe { read8(descriptor_base, 7) };
    let vendor_id = u16::from_le_bytes([unsafe { read8(descriptor_base, 8) }, unsafe {
        read8(descriptor_base, 9)
    }]);
    let product_id = u16::from_le_bytes([unsafe { read8(descriptor_base, 10) }, unsafe {
        read8(descriptor_base, 11)
    }]);

    let device_summary = UsbDeviceSummary {
        port,
        slot_id,
        speed_id,
        vendor_id,
        product_id,
        class,
        subclass,
        protocol,
        max_packet_size0,
    };

    let hub_summary = if inspect_hub {
        if class != 0x09 {
            return Err(InitError::NotHub);
        }

        let hub_bytes = unsafe {
            rings.control_in(
                transfer_base,
                &mut transfer_index,
                slot_id,
                setup_get_hub_descriptor(),
                descriptor_buffer,
                9,
            )
        }?;
        if hub_bytes < 7
            || unsafe { read8(descriptor_base, 0) } < 7
            || unsafe { read8(descriptor_base, 1) } != 0x29
        {
            return Err(InitError::DescriptorMalformed);
        }
        let downstream_ports = unsafe { read8(descriptor_base, 2) };
        let power_good_units = unsafe { read8(descriptor_base, 5) };
        if downstream_ports == 0 || downstream_ports > 31 {
            return Err(InitError::DescriptorMalformed);
        }

        // USB2 hub PORT_POWER feature selector = 8. Power each advertised
        // downstream port before looking for the QEMU-attached child.
        for downstream_port in 1..=downstream_ports {
            unsafe {
                rings.control_no_data(
                    transfer_base,
                    &mut transfer_index,
                    slot_id,
                    setup_set_port_feature(downstream_port, 8),
                )
            }?;
        }

        let mut child = None;
        for downstream_port in 1..=downstream_ports {
            let status_bytes = unsafe {
                rings.control_in(
                    transfer_base,
                    &mut transfer_index,
                    slot_id,
                    setup_get_port_status(downstream_port),
                    descriptor_buffer,
                    4,
                )
            }?;
            if status_bytes < 4 {
                return Err(InitError::DescriptorMalformed);
            }
            let status = u16::from_le_bytes([
                unsafe { read8(descriptor_base, 0) },
                unsafe { read8(descriptor_base, 1) },
            ]);
            if status & 1 != 0 {
                child = Some((downstream_port, status));
                break;
            }
        }
        let (child_port, _) = child.ok_or(InitError::NoDownstreamDevice)?;

        // USB2 hub PORT_RESET feature selector = 4. Poll GET_STATUS through
        // the same EP0 path until the child is connected, enabled and reset
        // has cleared. This proves real hub class request/port management.
        unsafe {
            rings.control_no_data(
                transfer_base,
                &mut transfer_index,
                slot_id,
                setup_set_port_feature(child_port, 4),
            )
        }?;
        let mut child_status = 0u16;
        let mut ready = false;
        for _ in 0..16 {
            let status_bytes = unsafe {
                rings.control_in(
                    transfer_base,
                    &mut transfer_index,
                    slot_id,
                    setup_get_port_status(child_port),
                    descriptor_buffer,
                    4,
                )
            }?;
            if status_bytes < 4 {
                return Err(InitError::DescriptorMalformed);
            }
            child_status = u16::from_le_bytes([
                unsafe { read8(descriptor_base, 0) },
                unsafe { read8(descriptor_base, 1) },
            ]);
            let connected = child_status & (1 << 0) != 0;
            let enabled = child_status & (1 << 1) != 0;
            let reset = child_status & (1 << 4) != 0;
            let powered = child_status & (1 << 8) != 0;
            if connected && enabled && powered && !reset {
                ready = true;
                break;
            }
            core::hint::spin_loop();
        }
        if !ready {
            return Err(InitError::PortResetTimeout);
        }

        Some(UsbHubSummary {
            root_port: port,
            slot_id,
            downstream_ports,
            child_port,
            child_status,
            power_good_units,
        })
    } else {
        None
    };

    unsafe { vm.unmap(9) }.map_err(|_| InitError::Mapping)?;
    unsafe { vm.unmap(8) }.map_err(|_| InitError::Mapping)?;
    unsafe { vm.unmap(6) }.map_err(|_| InitError::Mapping)?;
    unsafe { vm.unmap(5) }.map_err(|_| InitError::Mapping)?;
    unsafe { vm.unmap(4) }.map_err(|_| InitError::Mapping)?;
    unsafe { vm.unmap(3) }.map_err(|_| InitError::Mapping)?;
    unsafe { vm.unmap(2) }.map_err(|_| InitError::Mapping)?;
    unsafe { vm.unmap(0) }.map_err(|_| InitError::Mapping)?;

    Ok((device_summary, hub_summary))
}

/// Enumerate one directly attached root-port USB device.
///
/// # Safety
/// Same ownership requirements as the internal xHCI enumeration path.
#[cfg(target_os = "none")]
pub unsafe fn enumerate_first_device(info: &BootInfo) -> Result<UsbDeviceSummary, InitError> {
    Ok(unsafe { enumerate_first_device_inner(info, false) }?.0)
}

/// Address a directly attached USB2 hub and prove downstream port management.
///
/// # Safety
/// Same single-BSP/IF=0 exclusive-controller ownership as device enumeration.
#[cfg(target_os = "none")]
pub unsafe fn inspect_first_hub(info: &BootInfo) -> Result<UsbHubSummary, InitError> {
    unsafe { enumerate_first_device_inner(info, true) }?
        .1
        .ok_or(InitError::NotHub)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usb_enumeration_helpers_encode_architectural_fields() {
        assert_eq!(
            trb_type(trb_control(TRB_TYPE_ENABLE_SLOT)),
            TRB_TYPE_ENABLE_SLOT
        );
        assert_eq!(completion_code(1 << 24), COMPLETION_SUCCESS);
        assert_eq!(event_slot_id(7 << 24), 7);
        assert_eq!(port_speed(3 << PORTSC_SPEED_SHIFT), 3);
        assert_eq!(endpoint0_packet_size(1), Some(8));
        assert_eq!(endpoint0_packet_size(3), Some(64));
        assert_eq!(endpoint0_packet_size(4), Some(512));
        assert_eq!(endpoint0_packet_size(0), None);

        let setup = setup_get_device_descriptor();
        assert_eq!(setup[0], 0x0100_0680);
        assert_eq!(setup[1], 18 << 16);
        assert_eq!(setup[2], 8);
        assert_eq!(trb_type(setup[3]), TRB_TYPE_SETUP_STAGE);
        assert_ne!(setup[3] & TRB_IDT, 0);
        assert_eq!((setup[3] >> 16) & 0x3, 3);

        let hub = setup_get_hub_descriptor();
        assert_eq!(hub[0], 0x2900_06a0);
        assert_eq!(hub[1], 9 << 16);
        assert_eq!((hub[3] >> 16) & 0x3, 3);

        let power = setup_set_port_feature(2, 8);
        assert_eq!(power[0], 0x0008_0323);
        assert_eq!(power[1], 2);
        assert_eq!((power[3] >> 16) & 0x3, 0);

        let reset = setup_set_port_feature(3, 4);
        assert_eq!(reset[0], 0x0004_0323);
        assert_eq!(reset[1], 3);

        let status = setup_get_port_status(4);
        assert_eq!(status[0], 0x0000_00a3);
        assert_eq!(status[1], (4 << 16) | 4);
        assert_eq!((status[3] >> 16) & 0x3, 3);
    }

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
