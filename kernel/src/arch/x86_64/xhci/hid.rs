//! Direct full/low-speed HID boot-protocol input over native xHCI interrupt IN.
//! All DMA frames remain owned for kernel lifetime; no AP or IRQ can race this
//! bounded pre-STI path. Device input is decoded only after a matching event.

use super::*;
use vibrix_kernel::usb_hid::{self, Keyboard, Mouse, Protocol};

const CONFIGURE_ENDPOINT: u32 = 12;
const DISABLE_SLOT: u32 = 10;
const NORMAL: u32 = 1;

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
    pub endpoint: usb_hid::Endpoint,
    pub keyboard: Option<Keyboard>,
    pub mouse: Option<Mouse>,
}

pub(super) unsafe fn run(
    vm: &mut Window,
    rings: &mut RingCursor,
    control: Control<'_>,
    protocol: Protocol,
    expected_usage: u8,
) -> Result<Evidence, InitError> {
    let device = control.device;
    // SAFETY: the caller owns these mapped EP0 ring/buffer pages and submits
    // one transfer at a time, with IF=0. All descriptor accesses are within a
    // 256-byte prefix of the retained 4096-byte DMA page.
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
    let total = u16::from_le_bytes(unsafe {
        [
            read8(control.descriptor_base, 2),
            read8(control.descriptor_base, 3),
        ]
    });
    if !(9..=256).contains(&total) {
        return Err(InitError::DescriptorMalformed);
    }
    // Clear the bounded buffer so a short descriptor can never reuse data
    // from the prior response. The controller has completed the header TD.
    for offset in 0..256 {
        unsafe { core::ptr::write_volatile((control.descriptor_base + offset) as *mut u8, 0) };
    }
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
    let mut descriptor = [0; 256];
    for (offset, byte) in descriptor[..usize::from(total)].iter_mut().enumerate() {
        // SAFETY: completed DMA, checked length, volatile copy into owned RAM.
        *byte = unsafe { read8(control.descriptor_base, offset) };
    }
    let endpoint =
        usb_hid::endpoint(&descriptor[..usize::from(total)], protocol).map_err(InitError::Hid)?;
    let interval = usb_hid::interval(device.speed_id, endpoint.interval).map_err(InitError::Hid)?;
    if device.speed_id == 2 && endpoint.max_packet > 8 {
        return Err(InitError::Hid(usb_hid::Error::Unsupported));
    }
    let dci = endpoint.context_index();

    // SAFETY: the allocator returns fresh, retained aligned frames. Slots
    // 7/10/11/12 are vacant here and exclusively owned until the final unmap.
    let interrupt_ring = unsafe { memory::allocate_frame() }.ok_or(InitError::OutOfFrames)?;
    let report_buffer = unsafe { memory::allocate_frame() }.ok_or(InitError::OutOfFrames)?;
    let interrupt_base = unsafe { zero_frame(vm, 10, interrupt_ring) }? as usize;
    let report_base = unsafe { zero_frame(vm, 11, report_buffer) }? as usize;
    let input_base = unsafe { zero_frame(vm, 7, control.input_context) }? as usize;
    let output_base = unsafe { vm.map(12, control.device_context, false) }
        .map_err(|_| InitError::Mapping)? as usize;

    // SAFETY: Address Device completed and EP0 is idle. Copy only its live
    // 32-byte slot context, then set the highest enabled DCI. The validated
    // interrupt endpoint yields DCI 3..31, fitting this input-context page.
    unsafe {
        for offset in (0..32).step_by(4) {
            write32(input_base, 32 + offset, read32(output_base, offset));
        }
        write32(input_base, 4, 1 | (1u32 << dci));
        let slot = read32(input_base, 32);
        write32(
            input_base,
            32,
            (slot & !(0x1f << 27)) | (u32::from(dci) << 27),
        );
        // Input Slot DWord3 is reserved; output slot address/state is not input.
        write32(input_base, 44, 0);
        let offset = (usize::from(dci) + 1) * 32;
        write32(input_base, offset, u32::from(interval) << 16);
        write32(
            input_base,
            offset + 4,
            (3 << 1) | (7 << 3) | (u32::from(endpoint.max_packet) << 16),
        );
        write64(input_base, offset + 8, interrupt_ring | 1);
        write32(
            input_base,
            offset + 16,
            (protocol.report_bytes() as u32) | (u32::from(endpoint.max_packet) << 16),
        );
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
    // SAFETY: the same serialized EP0 ownership; these requests have no data.
    unsafe {
        rings.control_no_data(
            control.transfer_base,
            control.transfer_index,
            device.slot_id,
            setup_set_configuration(endpoint.configuration),
        )?;
        rings.control_no_data(
            control.transfer_base,
            control.transfer_index,
            device.slot_id,
            setup_hid_set_protocol(endpoint.interface),
        )?;
        rings.control_no_data(
            control.transfer_base,
            control.transfer_index,
            device.slot_id,
            setup_hid_set_idle(endpoint.interface),
        )?;
    }
    unsafe { unmap_ram(vm, 12) }?;
    unsafe { unmap_ram(vm, 7) }?;

    let mut previous = Keyboard::RELEASED;
    let mut key_pressed = None;
    let mut b_shifted = false;
    let mut button_pressed = false;
    let mut motion = None;
    let mut verified = false;
    for index in 0..32 {
        let length = u32::from(endpoint.max_packet);
        let pointer = interrupt_ring + (index * TRB_BYTES) as u64;
        // SAFETY: one retained ring page, no wrap/reuse, one report buffer,
        // only one outstanding TD. 32 TRBs occupy 512 bytes; all pages are WB
        // and x86 coherent. Publish data/control before ringing the doorbell.
        unsafe {
            write_trb(
                interrupt_base,
                index,
                [
                    report_buffer as u32,
                    (report_buffer >> 32) as u32,
                    length,
                    trb_control(NORMAL) | TRB_IOC,
                ],
            );
        }
        core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::Release);
        unsafe {
            write32(
                rings.doorbell_base,
                usize::from(device.slot_id) * 4,
                u32::from(dci),
            )
        };
        if index == 0 {
            crate::debugcon::write(match protocol {
                Protocol::Keyboard => "VIBRIX: kernel USB HID keyboard ready\r\n",
                Protocol::Mouse => "VIBRIX: kernel USB HID mouse ready\r\n",
            });
        }
        let mut completion = None;
        for _ in 0..16 {
            completion = unsafe {
                wait_for_event_type(
                    rings.event_base,
                    rings.event_physical,
                    rings.runtime_base,
                    &mut rings.event_index,
                    &mut rings.event_cycle,
                    TRB_TYPE_TRANSFER_EVENT,
                )
            };
            if completion.is_some() {
                break;
            }
        }
        let event = completion.ok_or(InitError::TransferTimeout)?;
        if event_slot_id(event[3]) != device.slot_id
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
        if remaining > length || length - remaining < protocol.report_bytes() as u32 {
            return Err(InitError::DescriptorMalformed);
        }
        if protocol == Protocol::Keyboard && length - remaining != 8 {
            return Err(InitError::DescriptorMalformed);
        }
        core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::Acquire);
        let mut bytes = [0; 8];
        for (offset, byte) in bytes[..protocol.report_bytes()].iter_mut().enumerate() {
            // SAFETY: matching completed TD, eight bytes maximum in one page.
            *byte = unsafe { read8(report_base, offset) };
        }
        match protocol {
            Protocol::Keyboard => {
                let current = Keyboard::decode(&bytes).map_err(InitError::Hid)?;
                let pressed = current.presses(previous);
                if pressed.contains(&expected_usage) && current.modifiers == 0 {
                    key_pressed = Some(current);
                }
                b_shifted |= pressed.contains(&5) && current.modifiers & 0x22 != 0;
                crate::println!(
                    "kernel USB keyboard: modifiers={:#04x} keys={:?}",
                    current.modifiers,
                    current.keys
                );
                verified = key_pressed.is_some() && b_shifted && current == Keyboard::RELEASED;
                previous = current;
            }
            Protocol::Mouse => {
                let current = Mouse::decode(&bytes[..3]).map_err(InitError::Hid)?;
                button_pressed |= current.buttons & 1 != 0;
                if current.dx == 7 && current.dy == -5 {
                    motion = Some(current);
                }
                crate::println!(
                    "kernel USB mouse: buttons={} dx={} dy={}",
                    current.buttons,
                    current.dx,
                    current.dy
                );
                verified = button_pressed && motion.is_some() && current.buttons == 0;
            }
        }
        if verified {
            break;
        }
    }
    if !verified {
        return Err(InitError::TransferTimeout);
    }
    // SAFETY: no pending transfer. Disable Slot completes before retiring CPU
    // mappings; DMA frames stay reserved even if later boot probes reset HC.
    let disabled = unsafe {
        rings.submit_command([
            0,
            0,
            0,
            trb_control(DISABLE_SLOT) | (u32::from(device.slot_id) << 24),
        ])
    }?;
    if event_slot_id(disabled[3]) != device.slot_id {
        return Err(InitError::InvalidControllerState);
    }
    unsafe { unmap_ram(vm, 11) }?;
    unsafe { unmap_ram(vm, 10) }?;
    crate::debugcon::write(match protocol {
        Protocol::Keyboard => "VIBRIX: USB keyboard interrupt press modifiers release verified\r\n",
        Protocol::Mouse => "VIBRIX: USB mouse interrupt motion buttons release verified\r\n",
    });
    Ok(Evidence {
        endpoint,
        keyboard: key_pressed,
        mouse: motion,
    })
}
