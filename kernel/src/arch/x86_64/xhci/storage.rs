//! Direct USB Mass Storage Class BOT/SCSI proof over native xHCI bulk endpoints.
//!
//! This bounded pre-STI path reuses the production EP0/controller ownership,
//! configures one SCSI-transparent BOT interface, executes real commands against
//! one directly attached device, verifies a reversible block write, and disables
//! the slot before returning. DMA pages remain retained for kernel lifetime.

use super::*;
use vibrix_kernel::{
    scsi,
    usb_mass_bulk::{self as bot, CommandStatus, Direction},
    usb_storage,
};

const CONFIGURE_ENDPOINT: u32 = 12;
const DISABLE_SLOT: u32 = 10;
const NORMAL: u32 = 1;
const MAX_RING_TRBS: usize = 64;

pub(super) struct Control<'a> {
    pub input_context: u64,
    pub device_context: u64,
    pub transfer_base: usize,
    pub transfer_index: &'a mut usize,
    pub descriptor_buffer: u64,
    pub descriptor_base: usize,
    pub device: UsbDeviceSummary,
}

pub(super) struct Evidence {
    pub interface: usb_storage::BulkInterface,
    pub capacity: scsi::Capacity10,
    pub verified_lba: u32,
}

struct BulkTransport<'a> {
    rings: &'a mut RingCursor,
    slot_id: u8,
    out_dci: u8,
    in_dci: u8,
    out_ring_base: usize,
    out_ring_physical: u64,
    out_index: usize,
    in_ring_base: usize,
    in_ring_physical: u64,
    in_index: usize,
    status_buffer: u64,
    status_base: usize,
    data_buffer: u64,
    data_base: usize,
    tag: u32,
}

impl BulkTransport<'_> {
    unsafe fn transfer(
        &mut self,
        input: bool,
        buffer: u64,
        length: u32,
    ) -> Result<u32, InitError> {
        let (ring_base, ring_physical, ring_index, dci) = if input {
            (
                self.in_ring_base,
                self.in_ring_physical,
                &mut self.in_index,
                self.in_dci,
            )
        } else {
            (
                self.out_ring_base,
                self.out_ring_physical,
                &mut self.out_index,
                self.out_dci,
            )
        };
        if *ring_index >= MAX_RING_TRBS || length == 0 || length > 4096 {
            return Err(InitError::InvalidControllerState);
        }
        let pointer = ring_physical + (*ring_index * TRB_BYTES) as u64;
        unsafe {
            write_trb(
                ring_base,
                *ring_index,
                [
                    buffer as u32,
                    (buffer >> 32) as u32,
                    length,
                    trb_control(NORMAL) | TRB_IOC,
                ],
            );
        }
        *ring_index += 1;
        core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::Release);
        unsafe {
            write32(
                self.rings.doorbell_base,
                usize::from(self.slot_id) * 4,
                u32::from(dci),
            )
        };

        let event = unsafe {
            wait_for_event_type(
                self.rings.event_base,
                self.rings.event_physical,
                self.rings.runtime_base,
                &mut self.rings.event_index,
                &mut self.rings.event_cycle,
                TRB_TYPE_TRANSFER_EVENT,
            )
        }
        .ok_or(InitError::TransferTimeout)?;

        if event_slot_id(event[3]) != self.slot_id
            || ((event[3] >> 16) & 0x1f) != u32::from(dci)
            || (u64::from(event[0]) | (u64::from(event[1]) << 32)) != pointer
        {
            return Err(InitError::InvalidControllerState);
        }
        let code = completion_code(event[2]);
        if code != COMPLETION_SUCCESS && code != COMPLETION_SHORT_PACKET {
            return Err(InitError::TransferFailed(code));
        }
        let remaining = event[2] & 0x00ff_ffff;
        if remaining > length {
            return Err(InitError::DescriptorMalformed);
        }
        Ok(length - remaining)
    }

    unsafe fn send_cbw(
        &mut self,
        command: scsi::Command,
        transfer_length: u32,
        direction: Direction,
    ) -> Result<u32, InitError> {
        self.tag = self.tag.wrapping_add(1).max(1);
        let wrapper = bot::CommandBlockWrapper::new(
            self.tag,
            transfer_length,
            direction,
            0,
            command.bytes,
            command.length,
        )
        .map_err(InitError::Bot)?;
        let bytes = wrapper.encode();
        unsafe { copy_to_dma(self.status_base, &bytes) };
        let sent = unsafe { self.transfer(false, self.status_buffer, bytes.len() as u32) }?;
        if sent != bytes.len() as u32 {
            return Err(InitError::StorageCommandFailed);
        }
        Ok(self.tag)
    }

    unsafe fn finish_csw(
        &mut self,
        tag: u32,
        expected_transfer_length: u32,
        expected_residue: u32,
    ) -> Result<(), InitError> {
        unsafe { clear_dma(self.status_base, bot::CSW_BYTES) };
        let received =
            unsafe { self.transfer(true, self.status_buffer, bot::CSW_BYTES as u32) }?;
        if received != bot::CSW_BYTES as u32 {
            return Err(InitError::StorageCommandFailed);
        }
        let mut bytes = [0u8; bot::CSW_BYTES];
        unsafe { copy_from_dma(self.status_base, &mut bytes) };
        let status =
            bot::parse_csw(&bytes, tag, expected_transfer_length).map_err(InitError::Bot)?;
        if status.status != CommandStatus::Passed || status.residue != expected_residue {
            return Err(InitError::StorageCommandFailed);
        }
        Ok(())
    }

    unsafe fn command_none(&mut self, command: scsi::Command) -> Result<(), InitError> {
        let tag = unsafe { self.send_cbw(command, 0, Direction::Out) }?;
        unsafe { self.finish_csw(tag, 0, 0) }
    }

    unsafe fn command_in(
        &mut self,
        command: scsi::Command,
        length: u32,
    ) -> Result<u32, InitError> {
        if length == 0 || length > PAGE as u32 {
            return Err(InitError::StorageCommandFailed);
        }
        let tag = unsafe { self.send_cbw(command, length, Direction::In) }?;
        unsafe { clear_dma(self.data_base, length as usize) };
        let received = unsafe { self.transfer(true, self.data_buffer, length) }?;
        let residue = length
            .checked_sub(received)
            .ok_or(InitError::StorageCommandFailed)?;
        unsafe { self.finish_csw(tag, length, residue) }?;
        Ok(received)
    }

    unsafe fn command_out(
        &mut self,
        command: scsi::Command,
        length: u32,
    ) -> Result<(), InitError> {
        if length == 0 || length > PAGE as u32 {
            return Err(InitError::StorageCommandFailed);
        }
        let tag = unsafe { self.send_cbw(command, length, Direction::Out) }?;
        let sent = unsafe { self.transfer(false, self.data_buffer, length) }?;
        if sent != length {
            return Err(InitError::StorageCommandFailed);
        }
        unsafe { self.finish_csw(tag, length, 0) }
    }
}

unsafe fn clear_dma(base: usize, length: usize) {
    for offset in 0..length {
        unsafe { core::ptr::write_volatile((base + offset) as *mut u8, 0) };
    }
}

unsafe fn copy_to_dma(base: usize, bytes: &[u8]) {
    for (offset, byte) in bytes.iter().copied().enumerate() {
        unsafe { core::ptr::write_volatile((base + offset) as *mut u8, byte) };
    }
}

unsafe fn copy_from_dma(base: usize, bytes: &mut [u8]) {
    for (offset, byte) in bytes.iter_mut().enumerate() {
        *byte = unsafe { read8(base, offset) };
    }
}

unsafe fn copy_dma(source: usize, destination: usize, length: usize) {
    for offset in 0..length {
        let byte = unsafe { read8(source, offset) };
        unsafe { core::ptr::write_volatile((destination + offset) as *mut u8, byte) };
    }
}

unsafe fn configure_endpoint(
    input_base: usize,
    dci: u8,
    endpoint: usb_storage::Endpoint,
    ring: u64,
) {
    let offset = (usize::from(dci) + 1) * 32;
    let endpoint_type = if endpoint.is_in() { 6 } else { 2 };
    unsafe {
        write32(input_base, offset, 0);
        write32(
            input_base,
            offset + 4,
            (3 << 1) | (endpoint_type << 3) | (u32::from(endpoint.max_packet) << 16),
        );
        write64(input_base, offset + 8, ring | 1);
        write32(
            input_base,
            offset + 16,
            u32::from(endpoint.max_packet),
        );
    }
}

pub(super) unsafe fn run(
    vm: &mut Window,
    rings: &mut RingCursor,
    control: Control<'_>,
) -> Result<Evidence, InitError> {
    let device = control.device;

    let header = unsafe {
        rings.control_in(
            control.transfer_base,
            control.transfer_index,
            device.slot_id,
            setup_get_configuration_descriptor(9),
            control.descriptor_buffer,
            9,
        )
    }?;
    if header != 9 {
        return Err(InitError::DescriptorMalformed);
    }
    let total = u16::from_le_bytes([
        unsafe { read8(control.descriptor_base, 2) },
        unsafe { read8(control.descriptor_base, 3) },
    ]);
    if !(9..=256).contains(&total) {
        return Err(InitError::DescriptorMalformed);
    }

    unsafe { clear_dma(control.descriptor_base, 256) };
    let count = unsafe {
        rings.control_in(
            control.transfer_base,
            control.transfer_index,
            device.slot_id,
            setup_get_configuration_descriptor(total),
            control.descriptor_buffer,
            u32::from(total),
        )
    }?;
    if count != u32::from(total) {
        return Err(InitError::DescriptorMalformed);
    }
    let mut descriptor = [0u8; 256];
    for (offset, byte) in descriptor[..usize::from(total)].iter_mut().enumerate() {
        *byte = unsafe { read8(control.descriptor_base, offset) };
    }
    let interface = usb_storage::bulk_interface(&descriptor[..usize::from(total)])
        .map_err(InitError::Storage)?;
    let out_dci = interface.bulk_out.context_index();
    let in_dci = interface.bulk_in.context_index();
    if out_dci < 2 || in_dci < 3 || out_dci > 31 || in_dci > 31 || out_dci == in_dci {
        return Err(InitError::DescriptorMalformed);
    }

    let out_ring = unsafe { memory::allocate_frame() }.ok_or(InitError::OutOfFrames)?;
    let in_ring = unsafe { memory::allocate_frame() }.ok_or(InitError::OutOfFrames)?;
    let data_buffer = unsafe { memory::allocate_frame() }.ok_or(InitError::OutOfFrames)?;
    let status_buffer = unsafe { memory::allocate_frame() }.ok_or(InitError::OutOfFrames)?;
    let original_buffer = unsafe { memory::allocate_frame() }.ok_or(InitError::OutOfFrames)?;

    let out_ring_base = unsafe { zero_frame(vm, 10, out_ring) }? as usize;
    let in_ring_base = unsafe { zero_frame(vm, 11, in_ring) }? as usize;
    let data_base = unsafe { zero_frame(vm, 12, data_buffer) }? as usize;
    let output_base = unsafe { vm.map(13, control.device_context, false) }
        .map_err(|_| InitError::Mapping)? as usize;
    let status_base = unsafe { zero_frame(vm, 14, status_buffer) }? as usize;
    let original_base = unsafe { zero_frame(vm, 15, original_buffer) }? as usize;
    let input_base = unsafe { zero_frame(vm, 7, control.input_context) }? as usize;

    unsafe {
        for offset in (0..32).step_by(4) {
            write32(input_base, 32 + offset, read32(output_base, offset));
        }
        let add = 1 | (1u32 << out_dci) | (1u32 << in_dci);
        write32(input_base, 4, add);
        let slot = read32(input_base, 32);
        let context_entries = out_dci.max(in_dci);
        write32(
            input_base,
            32,
            (slot & !(0x1f << 27)) | (u32::from(context_entries) << 27),
        );
        write32(input_base, 44, 0);
        configure_endpoint(input_base, out_dci, interface.bulk_out, out_ring);
        configure_endpoint(input_base, in_dci, interface.bulk_in, in_ring);
    }
    core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::Release);
    let configured = unsafe {
        rings.submit_command([
            control.input_context as u32,
            (control.input_context >> 32) as u32,
            0,
            trb_control(CONFIGURE_ENDPOINT) | (u32::from(device.slot_id) << 24),
        ])
    }?;
    if event_slot_id(configured[3]) != device.slot_id {
        return Err(InitError::InvalidControllerState);
    }
    unsafe {
        rings.control_no_data(
            control.transfer_base,
            control.transfer_index,
            device.slot_id,
            setup_set_configuration(interface.configuration),
        )?;
        unmap_ram(vm, 13)?;
        unmap_ram(vm, 7)?;
    }

    let mut transport = BulkTransport {
        rings,
        slot_id: device.slot_id,
        out_dci,
        in_dci,
        out_ring_base,
        out_ring_physical: out_ring,
        out_index: 0,
        in_ring_base,
        in_ring_physical: in_ring,
        in_index: 0,
        status_buffer,
        status_base,
        data_buffer,
        data_base,
        tag: 0x5642_5800,
    };

    unsafe { transport.command_none(scsi::test_unit_ready()) }?;

    let inquiry_bytes = unsafe { transport.command_in(scsi::inquiry(36), 36) }?;
    if inquiry_bytes != 36 || unsafe { read8(data_base, 0) } & 0x1f != 0 {
        return Err(InitError::StorageCommandFailed);
    }

    let sense_bytes = unsafe { transport.command_in(scsi::request_sense(18), 18) }?;
    if sense_bytes != 18 {
        return Err(InitError::StorageCommandFailed);
    }
    let mut sense = [0u8; 18];
    unsafe { copy_from_dma(data_base, &mut sense) };
    let sense = scsi::parse_fixed_sense(&sense).map_err(InitError::Scsi)?;
    if sense.key != 0 {
        return Err(InitError::StorageCommandFailed);
    }

    let capacity_bytes = unsafe { transport.command_in(scsi::read_capacity_10(), 8) }?;
    if capacity_bytes != 8 {
        return Err(InitError::StorageCommandFailed);
    }
    let mut capacity_data = [0u8; 8];
    unsafe { copy_from_dma(data_base, &mut capacity_data) };
    let capacity = scsi::parse_read_capacity_10(&capacity_data).map_err(InitError::Scsi)?;
    if capacity.block_bytes > PAGE as u32 || capacity.last_lba == u32::MAX {
        return Err(InitError::StorageCommandFailed);
    }
    let block_bytes = usize::try_from(capacity.block_bytes)
        .map_err(|_| InitError::StorageCommandFailed)?;
    let verified_lba = capacity.last_lba;

    let read = scsi::read_10(verified_lba, 1).map_err(InitError::Scsi)?;
    let read_bytes = unsafe { transport.command_in(read, capacity.block_bytes) }?;
    if read_bytes != capacity.block_bytes {
        return Err(InitError::StorageCommandFailed);
    }
    unsafe { copy_dma(data_base, original_base, block_bytes) };

    for offset in 0..block_bytes {
        let byte = 0xa5u8 ^ (offset as u8).wrapping_mul(37);
        unsafe { core::ptr::write_volatile((data_base + offset) as *mut u8, byte) };
    }
    let write = scsi::write_10(verified_lba, 1).map_err(InitError::Scsi)?;
    unsafe { transport.command_out(write, capacity.block_bytes) }?;
    unsafe { transport.command_none(scsi::synchronize_cache_10()) }?;

    let verify = scsi::read_10(verified_lba, 1).map_err(InitError::Scsi)?;
    let verify_bytes = unsafe { transport.command_in(verify, capacity.block_bytes) }?;
    if verify_bytes != capacity.block_bytes {
        return Err(InitError::StorageCommandFailed);
    }
    for offset in 0..block_bytes {
        let expected = 0xa5u8 ^ (offset as u8).wrapping_mul(37);
        if unsafe { read8(data_base, offset) } != expected {
            return Err(InitError::StorageCommandFailed);
        }
    }

    unsafe { copy_dma(original_base, data_base, block_bytes) };
    let restore = scsi::write_10(verified_lba, 1).map_err(InitError::Scsi)?;
    unsafe { transport.command_out(restore, capacity.block_bytes) }?;
    unsafe { transport.command_none(scsi::synchronize_cache_10()) }?;
    let restored = scsi::read_10(verified_lba, 1).map_err(InitError::Scsi)?;
    let restored_bytes = unsafe { transport.command_in(restored, capacity.block_bytes) }?;
    if restored_bytes != capacity.block_bytes {
        return Err(InitError::StorageCommandFailed);
    }
    for offset in 0..block_bytes {
        if unsafe { read8(data_base, offset) } != unsafe { read8(original_base, offset) } {
            return Err(InitError::StorageCommandFailed);
        }
    }

    crate::debugcon::write("VIBRIX: kernel USB mass storage BOT read write flush verified\r\n");

    let disabled = unsafe {
        transport.rings.submit_command([
            0,
            0,
            0,
            trb_control(DISABLE_SLOT) | (u32::from(device.slot_id) << 24),
        ])
    }?;
    if event_slot_id(disabled[3]) != device.slot_id {
        return Err(InitError::InvalidControllerState);
    }

    unsafe {
        unmap_ram(vm, 15)?;
        unmap_ram(vm, 14)?;
        unmap_ram(vm, 12)?;
        unmap_ram(vm, 11)?;
        unmap_ram(vm, 10)?;
    }

    Ok(Evidence {
        interface,
        capacity,
        verified_lba,
    })
}
