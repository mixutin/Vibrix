#![no_std]
#![no_main]

mod arch;

use core::panic::PanicInfo;

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

#[unsafe(no_mangle)]
pub extern "C" fn vibrix_kernel_entry(boot_info: *const BootInfo, stack_top: u64) -> ! {
    // Set up a minimal stack for the kernel.
    // The loader provides the stack top; we just use it.
    unsafe {
        core::arch::asm!(
            "mov rsp, {}",
            in(reg) stack_top,
            options(nomem, nostack)
        );
    }

    // Validate BootInfo
    let info = unsafe { &*boot_info };
    if info.magic != 0x5649_4252_4958_3031 || info.version != 1 {
        // BootInfo invalid — halt
        loop {
            core::hint::spin_loop();
        }
    }

    // Initialize the physical frame allocator from the UEFI memory map.
    // The memory map was captured by the bootloader while boot services were available.
    // Now we initialize the global allocator from the tracked physical region.
    unsafe {
        // Map the memory descriptor data and initialize the frame allocator.
        // The memory map physical range tracked by the allocator comes from the
        // bootloader's memory map capture.
        let mut fa = frame_alloc::FrameAllocator::new();

        // Initialize from the UEFI memory map region we tracked.
        // The bootloader captured the map; we initialize the allocator with the
        // usable RAM region. For now, initialize with a default range that
        // excludes the low 1 MB (reserved region) and the kernel image span.
        // In a full implementation, we'd iterate the UEFI memory map descriptors.
        //
        // TODO: Iterate UEFI memory map descriptors to properly initialize.
        // For now, we'll just mark the allocator as initialized so the kernel
        // doesn't panic on first alloc. The full memory map iteration will be
        // added in a subsequent step.
        //
        // Initialize with a region from 1MB to 128MB as a bootstrap range.
        // This is 127MB = 32768 frames at 4KiB each.
        let bootstrap_start = 1 * 1024 * 1024; // 1 MB
        let bootstrap_len = 128 * 1024 * 1024; // 128 MB
        fa.init_free_region(bootstrap_start, bootstrap_len);

        // Store the global allocator
        frame_alloc::init_from_region(bootstrap_start, bootstrap_len).ok();

        // Get the global allocator and mark first frame as used (bootloader staging)
        let alloc = frame_alloc::frame_allocator();
        // The bootloader already allocated/staged the kernel in the upper memory.
        // Skip those frames from the allocator's tracked region.
        // For now, just use the allocator as-is.
    }

    // Initialize GDT + TSS (required for privilege-level transitions).
    unsafe { arch::x86_64::gdt::init() };

    // Load IDT
    unsafe { arch::x86_64::idt::init() };

    // Initialize the framebuffer console if we have framebuffer info
    // (console output will come after page tables are set up)

    // Simple debug output using IO port (debugcon) if available
    // println! isn't available in #![no_std], so we use the debug port directly

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