#![no_std]
#![no_main]

#[path = "../../shared/bootinfo.rs"]
#[allow(dead_code)]
mod bootinfo;

mod elf;
mod loader;
mod memory_map;
mod paging;
mod uefi;

use core::panic::PanicInfo;
use uefi::{Console, EFI_LOAD_ERROR, Handle, Status, SystemTable};

/// UEFI application entry point.
///
/// # Safety
///
/// Firmware must call this function according to the UEFI x86-64 ABI. `image` must be the
/// current image handle and `system_table` must point to a valid UEFI system table whose
/// boot services remain available for the duration of this function.
#[unsafe(no_mangle)]
pub unsafe extern "efiapi" fn efi_main(image: Handle, system_table: *mut SystemTable) -> Status {
    let Some(mut console) = (unsafe { Console::from_system_table(system_table) }) else {
        return EFI_LOAD_ERROR;
    };

    console.write("Vibrix bootloader v0.0.2\r\n");
    console.write("VIBRIX: bootloader entered\r\n");

    let kernel = match unsafe { uefi::load_kernel(image, system_table) } {
        Ok(kernel) => kernel,
        Err(status) => {
            console.write("VIBRIX: kernel.elf load failed\r\n");
            return status;
        }
    };

    console.write("VIBRIX: kernel.elf found\r\n");

    let info = match elf::validate(kernel.as_slice()) {
        Ok(info) => info,
        Err(error) => {
            console.write(error.message());
            return EFI_LOAD_ERROR;
        }
    };

    console.write("VIBRIX: ELF64 valid\r\n");
    console.write("VIBRIX: x86_64 executable validated\r\n");

    if info.program_headers == 0 || info.load_segments == 0 || info.entry == 0 {
        console.write("VIBRIX: kernel metadata invalid\r\n");
        return EFI_LOAD_ERROR;
    }

    console.write("VIBRIX: PT_LOAD parsed\r\n");
    console.write("VIBRIX: kernel validated\r\n");

    // Capture only a physical address; it is not a dereferenceable kernel pointer.
    let Some(rsdp_address) = (unsafe { uefi::find_rsdp(system_table) }) else {
        console.write("VIBRIX: ACPI RSDP not found or invalid\r\n");
        return EFI_LOAD_ERROR;
    };
    console.write("VIBRIX: ACPI RSDP validated\r\n");

    let Some(framebuffer) = (unsafe { uefi::discover_framebuffer(system_table) }) else {
        console.write("VIBRIX: GOP framebuffer unavailable\r\n");
        return EFI_LOAD_ERROR;
    };
    console.write("VIBRIX: GOP framebuffer discovered\r\n");

    let loaded_kernel = match unsafe { loader::stage_kernel(system_table, &kernel, &info) } {
        Ok(loaded) => loaded,
        Err(error) => {
            console.write(error.message());
            return error.status();
        }
    };
    let _ = (
        loaded_kernel.physical_base,
        loaded_kernel.virtual_base,
        loaded_kernel.span_bytes,
        loaded_kernel.pages,
        loaded_kernel.entry,
    );
    console.write("VIBRIX: kernel segments staged\r\n");

    let page_tables = match unsafe {
        paging::build_kernel_page_tables(system_table, &kernel, &info, &loaded_kernel)
    } {
        Ok(page_tables) => page_tables,
        Err(error) => {
            console.write(error.message());
            return error.status();
        }
    };
    let _ = (
        page_tables.root_physical,
        page_tables.table_pages,
        page_tables.mapped_pages,
    );
    console.write("VIBRIX: kernel page tables verified\r\n");
    // Allocate permanent loader-owned BootInfo backing *before* acquiring the
    // final map/key. Its eventual kernel virtual mapping is a separate M2 step.
    let boot_info_physical = match unsafe { uefi::allocate_loader_pages(system_table, 1) } {
        Ok(physical) => physical,
        Err(status) => {
            console.write("VIBRIX: BootInfo backing allocation failed\r\n");
            return status;
        }
    };
    let Ok(boot_info_address) = usize::try_from(boot_info_physical) else {
        console.write("VIBRIX: BootInfo backing address invalid\r\n");
        return EFI_LOAD_ERROR;
    };
    if boot_info_address == 0 || boot_info_physical & 4095 != 0 {
        console.write("VIBRIX: BootInfo backing alignment invalid\r\n");
        return EFI_LOAD_ERROR;
    }

    let memory_map = match unsafe { memory_map::capture(system_table) } {
        Ok(map) => map,
        Err(status) => {
            // No successful map/key is retained on failure, so console use is safe.
            console.write("VIBRIX: final memory map capture failed\r\n");
            return status;
        }
    };
    // Retain the complete final tuple from one successful call. Do not use
    // Console here: firmware output could allocate and invalidate the map key.
    // The map's page allocation has no Drop and stays owned while we spin.
    let _ = (
        memory_map.buffer,
        memory_map.physical_base,
        memory_map.pages,
        memory_map.capacity,
        memory_map.byte_len,
        memory_map.map_key,
        memory_map.descriptor_size,
        memory_map.descriptor_version,
    );
    uefi::debug_write("VIBRIX: final memory map captured\r\n");

    // No firmware calls after successful map acquisition: preserve exactly
    // this buffer's length, stride and descriptor version, and keep the key
    // strictly loader-local for the later ExitBootServices implementation.
    let boot_info = match bootinfo::BootInfo::new(
        bootinfo::FramebufferInfo {
            physical_base: framebuffer.base,
            size_bytes: framebuffer.size,
            width: framebuffer.width,
            height: framebuffer.height,
            stride: framebuffer.stride,
            pixel_format: framebuffer.format,
        },
        rsdp_address,
        bootinfo::FinalMemoryMap {
            physical_base: memory_map.physical_base,
            byte_len: memory_map.byte_len,
            descriptor_size: memory_map.descriptor_size,
            descriptor_version: memory_map.descriptor_version,
        },
    ) {
        Ok(info) => info,
        Err(_error) => {
            uefi::debug_write("VIBRIX: BootInfo v2 validation failed\r\n");
            return EFI_LOAD_ERROR;
        }
    };
    // SAFETY: UEFI AllocatePages granted one page of EfiLoaderData, writable
    // and mapped in the firmware address space. The checked address is aligned
    // for BootInfo and the 88-byte object fits in the exclusive 4096-byte page.
    unsafe { (boot_info_address as *mut bootinfo::BootInfo).write(boot_info) };
    let _boot_info_page_owner = boot_info_physical;
    uefi::debug_write("VIBRIX: BootInfo v2 staged\r\n");

    // Do not attempt to dereference the physical map address as a kernel
    // pointer, activate incomplete page tables, or exit boot services here.
    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
