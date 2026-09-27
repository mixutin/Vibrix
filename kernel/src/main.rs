#![no_std]
#![no_main]

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
pub extern "C" fn vibrix_kernel_entry(_boot_info: *const BootInfo) -> ! {
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
