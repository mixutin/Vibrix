#![no_std]
#![no_main]

mod arch;

use core::panic::PanicInfo;

// Same representation and validation code is compiled by both loader and kernel.
#[path = "../../shared/bootinfo.rs"]
#[allow(dead_code)]
mod bootinfo;
pub use bootinfo::BootInfo;

#[unsafe(no_mangle)]
pub extern "C" fn vibrix_kernel_entry(_boot_info: *const BootInfo) -> ! {
    // Re-enumerate on each boot; a portable USB may boot on a different CPU.
    let _cpu = arch::x86_64::cpuid::discover();

    // Initialize GDT + TSS (required for privilege-level transitions).
    unsafe { arch::x86_64::gdt::init() };

    // Bring up COM1 only after the merged architecture baseline is installed.
    arch::x86_64::serial::init();
    crate::println!("Vibrix kernel started.");

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
