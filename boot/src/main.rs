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

    let mut page_tables = match unsafe {
        paging::build_kernel_page_tables(system_table, &kernel, &info, &loaded_kernel)
    } {
        Ok(page_tables) => page_tables,
        Err(error) => {
            console.write(error.message());
            return error.status();
        }
    };
    console.write("VIBRIX: kernel page tables verified\r\n");

    // Preserve the loader's actual firmware-resident PE image when the new
    // hierarchy becomes active, rather than identity-mapping arbitrary RAM.
    let (loader_image_base, loader_image_size) =
        match unsafe { uefi::loader_image_range(image, system_table) } {
            Ok(region) => region,
            Err(status) => {
                console.write("VIBRIX: loader image range unavailable\r\n");
                return status;
            }
        };
    // A dedicated, loader-owned 64 KiB stack avoids depending on reclaimable
    // firmware stack pages after ExitBootServices.
    const STACK_PAGES: usize = 16;
    let kernel_stack_base = match unsafe { uefi::allocate_loader_pages(system_table, STACK_PAGES) }
    {
        Ok(physical) => physical,
        Err(status) => {
            console.write("VIBRIX: kernel stack allocation failed\r\n");
            return status;
        }
    };
    let Some(_kernel_stack_top) = kernel_stack_base.checked_add(STACK_PAGES as u64 * 4096) else {
        console.write("VIBRIX: kernel stack range invalid\r\n");
        return EFI_LOAD_ERROR;
    };
    // Allocate permanent loader-owned BootInfo backing *before* final map.
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

    // Reserve the map buffer early. Extending page tables may allocate more
    // pages and invalidate this provisional key; refresh in place afterwards.
    let mut memory_map = match unsafe { memory_map::capture(system_table) } {
        Ok(map) => map,
        Err(status) => {
            // No successful map/key is retained on failure, so console use is safe.
            console.write("VIBRIX: final memory map capture failed\r\n");
            return status;
        }
    };
    // Narrow 4 KiB identity mappings support transition code, dedicated
    // stack and physical handoff memory without identity-mapping all RAM.
    // The GOP BAR is explicitly uncached (PCD), not treated as write-back RAM.
    let identity_regions = [
        paging::IdentityRegion {
            physical_base: loader_image_base,
            byte_len: loader_image_size,
            writable: true, // Temporary pre-kernel PE code/data interval.
            executable: true,
            uncached: false,
        },
        paging::IdentityRegion {
            physical_base: kernel_stack_base,
            byte_len: STACK_PAGES as u64 * 4096,
            writable: true,
            executable: false,
            uncached: false,
        },
        paging::IdentityRegion {
            physical_base: boot_info_physical,
            byte_len: 4096,
            writable: true,
            executable: false,
            uncached: false,
        },
        paging::IdentityRegion {
            physical_base: memory_map.physical_base,
            byte_len: memory_map.capacity as u64,
            writable: true,
            executable: false,
            uncached: false,
        },
        paging::IdentityRegion {
            physical_base: rsdp_address,
            byte_len: 4096,
            writable: false,
            executable: false,
            uncached: false,
        },
        paging::IdentityRegion {
            physical_base: framebuffer.base,
            byte_len: framebuffer.size,
            writable: true,
            executable: false,
            uncached: true,
        },
    ];
    if let Err(error) =
        unsafe { paging::map_identity_regions(system_table, &mut page_tables, &identity_regions) }
    {
        console.write(error.message());
        return error.status();
    }
    let _ = (
        page_tables.root_physical,
        page_tables.table_pages,
        page_tables.mapped_pages,
        kernel_stack_base,
        memory_map.pages,
    );
    console.write("VIBRIX: transition mappings verified\r\n");

    // GetMemoryMap from the *same preallocated buffer* after all allocations.
    // This refresh performs no AllocatePages, FreePages or firmware logging.
    if let Err(status) = unsafe { memory_map::refresh(&mut memory_map) } {
        console.write("VIBRIX: final memory map refresh failed\r\n");
        return status;
    }
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

    // The kernel PT_LOAD and identity transition regions are now software
    // verified but CR3 remains untouched. ExitBootServices and the direct
    // kernel jump belong to the subsequent separately validated step.
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
