#![no_std]
#![no_main]

mod arch;
mod debugcon;
mod framebuffer;

use core::panic::PanicInfo;

// Same representation and validation code is compiled by both loader and kernel.
#[path = "../../shared/bootinfo.rs"]
#[allow(dead_code)]
mod bootinfo;
pub use bootinfo::BootInfo;

/// BootInfo is loader-owned and identity-mapped while the initial kernel
/// page tables are active. Check the v1-prefix version *before* reading the
/// 8-byte v2 tail, then validate all remaining metadata as untrusted input.
///
/// # Safety
/// Loader must provide an aligned, mapped 88-byte BootInfo page that remains
/// owned until the kernel copies it. Non-null/alignment checks alone do not
/// establish that pointer provenance or mapping.
unsafe fn read_boot_info(raw: *const BootInfo) -> Result<BootInfo, ()> {
    if raw.is_null() || !(raw as usize).is_multiple_of(core::mem::align_of::<BootInfo>()) {
        return Err(());
    }
    // SAFETY: loader precondition guarantees the v1 common prefix is mapped.
    let version = unsafe { raw.cast::<u8>().add(8).cast::<u32>().read() };
    if version != bootinfo::BOOTINFO_VERSION {
        return Err(());
    }
    // SAFETY: version check above and loader's full v2 mapping/lifetime
    // establish the 88-byte readable object before dereferencing its tail.
    let info = unsafe { raw.read() };
    info.validate().map_err(|_| ())?;
    Ok(info)
}

/// Enter after firmware services are terminated and the loader has
/// activated verified PML4 and a dedicated 16-byte-aligned entry stack.
///
/// # Safety
/// BootInfo must point to one loader-owned, aligned, mapped and readable v2
/// page retained until this function copies and validates the object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vibrix_kernel_entry(boot_info: *const BootInfo) -> ! {
    // This marker is executed from the *kernel*, after successful firmware
    // exit and the assembly CR3/stack switch, never by the UEFI loader.
    debugcon::write("VIBRIX: kernel entry after ExitBootServices\r\n");
    let info = match unsafe { read_boot_info(boot_info) } {
        Ok(info) => info,
        Err(()) => {
            debugcon::write("VIBRIX: BootInfo v2 rejected by kernel\r\n");
            loop {
                core::hint::spin_loop();
            }
        }
    };
    debugcon::write("VIBRIX: kernel BootInfo v2 validated\r\n");

    // Re-enumerate on each boot; a portable USB may boot on a different CPU.
    let _cpu = arch::x86_64::cpuid::discover();

    // Initialize GDT + TSS before any privilege-level transitions. IRQs
    // remain disabled; RSP0/IST must be provisioned before enabling them.
    unsafe { arch::x86_64::gdt::init() };
    debugcon::write("VIBRIX: kernel GDT/TSS loaded\r\n");

    // Independent 16550 COM1 output now originates from the real kernel.
    arch::x86_64::serial::init();
    crate::println!("Vibrix kernel started.");
    debugcon::write("VIBRIX: kernel serial initialized\r\n");

    // Physical GOP BAR is explicitly identity-mapped UC in the active PML4.
    if unsafe { framebuffer::draw_boot_marker(&info) }.is_ok() {
        debugcon::write("VIBRIX: kernel framebuffer wrote pixels\r\n");
    } else {
        debugcon::write("VIBRIX: kernel framebuffer rejected\r\n");
    }

    // Separate QEMU-only smoke configuration exercises the *real* kernel
    // panic handler after the ordinary post-firmware boot path succeeded.
    #[cfg(feature = "panic-probe")]
    panic!("VIBRIX: kernel panic probe");

    #[cfg(not(feature = "panic-probe"))]
    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // Neither path calls UEFI or allocates. COM1 output may fail/bail out if
    // it is not initialized; QEMU debugcon remains independent.
    debugcon::write("VIBRIX: kernel panic\r\n");
    crate::println!("kernel panic: {}", info);
    loop {
        core::hint::spin_loop();
    }
}
