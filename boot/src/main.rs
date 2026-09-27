#![no_std]
#![no_main]

mod elf;
mod loader;
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

    // The physical address must eventually be forwarded in BootInfo.
    let Some(_rsdp_address) = (unsafe { uefi::find_rsdp(system_table) }) else {
        console.write("VIBRIX: ACPI RSDP not found or invalid\r\n");
        return EFI_LOAD_ERROR;
    };
    console.write("VIBRIX: ACPI RSDP validated\r\n");

    let Some(framebuffer) = (unsafe { uefi::discover_framebuffer(system_table) }) else {
        console.write("VIBRIX: GOP framebuffer unavailable\r\n");
        return EFI_LOAD_ERROR;
    };
    // This is discovery only: BootInfo population and kernel mapping follow later.
    let _ = (
        framebuffer.base,
        framebuffer.size,
        framebuffer.width,
        framebuffer.height,
        framebuffer.stride,
        framebuffer.format,
    );
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
    console.write("Next: establish initial kernel mappings.\r\n");

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
