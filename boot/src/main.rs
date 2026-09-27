#![no_std]
#![no_main]

mod elf;
mod loader;
mod uefi;

use core::panic::PanicInfo;

// BootInfo layout must match kernel::BootInfo for the handoff pointer.
#[repr(C)]
pub struct BootInfo {
    pub magic: u64,
    pub version: u32,
    pub _reserved: u32,
    pub framebuffer_base: u64,
    pub framebuffer_size: u64,
    pub framebuffer_width: u32,
    pub framebuffer_height: u32,
    pub framebuffer_stride: u32,
    pub framebuffer_format: u32,
    pub rsdp: u64,
    pub memory_map: u64,
    pub memory_map_len: u64,
    pub memory_descriptor_size: u64,
}

use uefi::{Console, EFI_LOAD_ERROR, Handle, Status, SystemTable};

const BOOT_MAGIC: u64 = 0x5649_4252_4958_3031; // "VIBRIX01"
const BOOT_VERSION: u32 = 1;

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

    // Capture the final UEFI firmware memory map before ExitBootServices.
    // This must happen while Boot Services are still available.
    let (map_buffer, map_size, map_key, desc_size, desc_ver) = match unsafe {
        uefi::capture_uefi_memory_map(system_table)
    } {
        Ok(data) => data,
        Err(status) => {
            console.write("VIBRIX: failed to capture UEFI memory map\r\n");
            return status;
        }
    };

    // Build the BootInfo structure that the kernel will use after ExitBootServices.
    // The firmware memory map physical addresses will be forwarded via BootInfo,
    // and the kernel will re-map them after taking over page tables.
    let boot_info = BootInfo {
        magic: BOOT_MAGIC,
        version: BOOT_VERSION,
        _reserved: 0,
        framebuffer_base: framebuffer.base,
        framebuffer_size: framebuffer.size,
        framebuffer_width: framebuffer.width,
        framebuffer_height: framebuffer.height,
        framebuffer_stride: framebuffer.stride,
        framebuffer_format: framebuffer.format,
        rsdp: rsdp_address as u64,
        memory_map: map_buffer as u64,
        memory_map_len: map_size as u64,
        memory_descriptor_size: desc_size as u64,
    };

    console.write("VIBRIX: BootInfo populated\r\n");

    // Set up stack top for the kernel. The stack will be established at a high address
    // in the higher-half kernel mapping. For now use a default kernel stack top.
    let stack_top = 0xFFFF_FFFF_FFF0_0000; // 4 KiB-aligned top of kernel stack

    // Call ExitBootServices to release firmware services.
    // After this, the kernel must own all physical memory it used through Boot Services.
    console.write("VIBRIX: exiting boot services\r\n");
    let exit_status = unsafe {
        ((*system_table).boot_services.exit_boot_services)(
            image,
            map_key,
        )
    };
    if exit_status != EFI_SUCCESS {
        console.write("VIBRIX: ExitBootServices failed\r\n");
        loop {
            core::hint::spin_loop();
        }
    }
    console.write("VIBRIX: boot services released\r\n");

    // Transfer control to the kernel entry point.
    // The kernel will set up its own page tables and continue initialization.
    console.write("VIBRIX: transferring to kernel entry\r\n");

    // SAFETY: The bootloader has:
    // - Constructed a valid BootInfo with physical addresses
    // - Called ExitBootServices, releasing firmware ownership
    // - The kernel.elf has been staged in loader-owned physical pages
    // - The UEFI memory map is no longer valid after ExitBootServices
    // The kernel entry will set up its own page tables.
    unsafe {
        core::arch::asm!(
            "jmp {target}",
            target = in(reg) loaded_kernel.entry,
            options(noreturn),
        );
    }

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