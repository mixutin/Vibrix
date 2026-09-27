#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

mod arch;
mod debugcon;
mod framebuffer;
mod memory;

use core::panic::PanicInfo;

// Same representation and validation code is compiled by both loader and kernel.
#[path = "../../shared/bootinfo.rs"]
#[allow(dead_code)]
mod bootinfo;
pub use bootinfo::BootInfo;

/// BootInfo is loader-owned and identity-mapped while the initial kernel
/// page tables are active. Check the v1-prefix version *before* reading the
/// 16-byte v2/v3 tail, then validate all remaining metadata as untrusted input.
///
/// # Safety
/// Loader must provide an aligned, mapped 96-byte BootInfo page that remains
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
    // SAFETY: version check above and loader's full v3 mapping/lifetime
    // establish the 96-byte readable object before dereferencing its tail.
    let info = unsafe { raw.read() };
    info.validate().map_err(|_| ())?;
    Ok(info)
}

/// Read only the firmware-validated RSDP prefix/window under the explicit
/// identity mappings established for the post-ExitBootServices kernel.
///
/// # Safety
/// `info.rsdp` must identify the RSDP discovered/validated by the loader;
/// ADR 0006 maps the 4096-byte window spanning that physical address as
/// readable. ACPI 2.0+ firmware validated at least 36 bytes before handoff.
unsafe fn parse_boot_rsdp(info: &BootInfo) -> Result<arch::x86_64::acpi::Rsdp, ()> {
    let base = usize::try_from(info.rsdp).map_err(|_| ())?;
    if base == 0 || base.checked_add(36).is_none() {
        return Err(());
    }
    // SAFETY: mapped and firmware-validated legacy 20-byte RSDP prefix.
    let prefix = unsafe { core::slice::from_raw_parts(base as *const u8, 20) };
    let bytes = if prefix[15] >= 2 {
        // SAFETY: loader verified the ACPI 2.0+ extension and mapped both
        // pages when an RSDP crosses a 4-KiB page boundary. Fail closed for
        // future firmware reporting more than the 36 readable bytes here.
        unsafe { core::slice::from_raw_parts(base as *const u8, 36) }
    } else {
        prefix
    };
    arch::x86_64::acpi::parse_rsdp(bytes).map_err(|_| ())
}

/// Enter after firmware services are terminated and the loader has
/// activated verified PML4 and a dedicated 16-byte-aligned entry stack.
///
/// # Safety
/// BootInfo must point to one loader-owned, aligned, mapped and readable v3
/// page retained until this function copies and validates the object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vibrix_kernel_entry(boot_info: *const BootInfo) -> ! {
    // This marker is executed from the *kernel*, after successful firmware
    // exit and the assembly CR3/stack switch, never by the UEFI loader.
    debugcon::write("VIBRIX: kernel entry after ExitBootServices\r\n");
    let info = match unsafe { read_boot_info(boot_info) } {
        Ok(info) => info,
        Err(()) => {
            debugcon::write("VIBRIX: BootInfo v3 rejected by kernel\r\n");
            loop {
                core::hint::spin_loop();
            }
        }
    };
    debugcon::write("VIBRIX: kernel BootInfo v3 validated\r\n");

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

    // Install only synchronous exception vectors; IF stays cleared until
    // the IRQ routing model and TSS privilege/IST stacks are ready.
    unsafe { arch::x86_64::idt::init() };
    debugcon::write("VIBRIX: kernel IDT installed\r\n");

    // A kernel-side ACPI read after ExitBootServices, not a loader marker.
    // The XSDT/MCFG table pages are NOT mapped yet: consume RSDP metadata
    // only and leave all subsequent physical addresses as integers.
    match unsafe { parse_boot_rsdp(&info) } {
        Ok(_rsdp) => debugcon::write("VIBRIX: kernel ACPI RSDP parsed\r\n"),
        Err(()) => debugcon::write("VIBRIX: kernel ACPI RSDP rejected\r\n"),
    }

    // The entire final descriptor buffer is loader-owned EfiLoaderData,
    // explicitly identity-mapped under the active kernel PML4. No frame
    // number is dereferenced: a later virtual mapper must map/zero pages.
    // Early single-CPU mode still has interrupts disabled.
    if unsafe { memory::init_from_boot_info(&info) }.is_err() {
        debugcon::write("VIBRIX: kernel frame allocator rejected map\r\n");
        loop {
            core::hint::spin_loop();
        }
    }
    debugcon::write("VIBRIX: kernel frame allocator initialized\r\n");
    if unsafe { memory::smoke_claim_two_frames() }.is_err() {
        debugcon::write("VIBRIX: kernel conventional frame claims failed\r\n");
        loop {
            core::hint::spin_loop();
        }
    }
    debugcon::write("VIBRIX: kernel conventional frames allocated\r\n");

    // Legacy PCI config mechanism #1 reads segment-zero vendor/class/BAR
    // metadata without relying on firmware protocols after ExitBootServices.
    // This scans all 256 bus numbers but does NOT touch any device BAR
    // memory, enable bus mastering or identify the persistent boot USB.
    let mut shown = 0usize;
    let pci = unsafe {
        arch::x86_64::pci::discover_legacy_segment_zero(|device| {
            if shown < 8 {
                crate::println!(
                    "PCI {:02x}:{:02x}.{} {:04x}:{:04x} class {:02x}:{:02x}:{:02x}",
                    device.bdf.bus,
                    device.bdf.device,
                    device.bdf.function,
                    device.vendor,
                    device.id,
                    device.class,
                    device.subclass,
                    device.programming_interface
                );
                shown += 1;
            }
        })
    };
    crate::println!(
        "Vibrix PCI segment0: {} devices, {} assigned BARs, {} xHCI",
        pci.devices,
        pci.assigned_bars,
        pci.xhci_controllers
    );
    if pci.devices == 0 || pci.assigned_bars == 0 || pci.malformed_bars != 0 {
        debugcon::write("VIBRIX: kernel PCI segment0 discovery rejected\r\n");
    } else {
        debugcon::write("VIBRIX: kernel PCI segment0 enumerated\r\n");
        debugcon::write("VIBRIX: kernel PCI BARs parsed\r\n");
    }

    // SAFETY: v3 loader retained the exclusive mapped leaf table; IF=0.
    if unsafe { memory::virtual_memory::runtime::smoke_test(&info) }.is_err() {
        panic!("kernel mapping window validation failed");
    }
    debugcon::write("VIBRIX: kernel virtual mappings verified\r\n");
    crate::println!("kernel VM: map, protect, unmap and remap verified");

    // Static BSS backing is already supervisor RW/NX in the loader mappings.
    // SAFETY: sole boot CPU, IF=0, no interrupt or reentrant heap users.
    if unsafe { memory::heap::smoke_test() }.is_err() {
        panic!("early kernel heap validation failed");
    }
    debugcon::write("VIBRIX: kernel heap allocation and reuse verified\r\n");
    crate::println!("kernel heap: aligned allocations, RAM writes and reuse verified");

    // Physical GOP BAR is explicitly identity-mapped UC in the active PML4.
    if unsafe { framebuffer::draw_boot_marker(&info) }.is_ok() {
        debugcon::write("VIBRIX: kernel framebuffer wrote pixels\r\n");
    } else {
        debugcon::write("VIBRIX: kernel framebuffer rejected\r\n");
    }

    #[cfg(any(feature = "vm-write-probe", feature = "vm-unmap-probe"))]
    unsafe {
        memory::virtual_memory::runtime::fault_probe(&info);
    }

    // QEMU-only probes exercise *actual CPU traps* through the production
    // IDT and print diagnostics over the independent kernel COM1 console.
    #[cfg(feature = "breakpoint-probe")]
    unsafe {
        core::arch::asm!("int3", options(nomem, nostack));
    }

    #[cfg(feature = "page-fault-probe")]
    unsafe {
        // Canonical 1 TiB low address lies outside the loader's narrow
        // identity regions and the linked higher-half PT_LOAD mappings.
        // No Rust reference/pointer dereference is constructed here.
        core::arch::asm!(
            "mov rax, qword ptr [rdx]",
            in("rdx") 0x100_0000_0000u64,
            out("rax") _,
            options(nostack, readonly)
        );
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
