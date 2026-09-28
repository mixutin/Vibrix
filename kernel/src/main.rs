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

    // Actual retained firmware SDTs are not identity mapped. Read only
    // ACPI-type WB pages via the now-empty temporary v3 mapping window,
    // unmapping all leaves before later kernel facilities use that window.
    let mut timer_setup = None;
    let acpi_mcfg = unsafe { parse_boot_rsdp(&info) }.and_then(|rsdp| {
        // SAFETY: sole boot CPU, IF=0, physical allocator initialized and
        // mapping-window smoke test has unmapped every temporary leaf.
        unsafe { arch::x86_64::acpi_runtime::inspect(&info, &rsdp) }.map_err(|error| {
            crate::println!("kernel ACPI/ECAM validation failed: {:?}", error);
        })
    });
    match acpi_mcfg {
        Ok(discovery) => {
            crate::println!(
                "kernel ACPI: {} validated MCFG allocations",
                discovery.allocations
            );
            crate::println!(
                "Vibrix ECAM segment0 bus0: {} devices, {} xHCI",
                discovery.ecam.devices,
                discovery.ecam.xhci_controllers
            );
            debugcon::write("VIBRIX: kernel ACPI XSDT and MCFG mapped and parsed\r\n");
            debugcon::write("VIBRIX: kernel PCI ECAM bus0 read\r\n");
            crate::println!(
                "kernel ACPI APIC topology: LAPIC {:#x}, {} IOAPIC(s), first {:#x}",
                discovery.lapic_physical,
                discovery.ioapics,
                discovery.ioapic_physical
            );
            // SAFETY: MADT physical addresses are checksummed ACPI metadata;
            // probe independently validates firmware MMIO ownership/PAT and
            // uses the empty v3 window while IF remains cleared.
            match unsafe {
                arch::x86_64::apic::probe(
                    &info,
                    discovery.lapic_physical,
                    discovery.ioapic_physical,
                )
            } {
                Ok(apic) => {
                    crate::println!(
                        "Vibrix APIC: LAPIC id={} version={:#x} max_lvt={}, IOAPIC id={} version={:#x} max_redir={}",
                        apic.lapic_id,
                        apic.lapic_version,
                        apic.lapic_max_lvt,
                        apic.ioapic_id,
                        apic.ioapic_version,
                        apic.ioapic_max_redirection_entry
                    );
                    debugcon::write("VIBRIX: kernel LAPIC and IOAPIC registers read\r\n");
                    timer_setup = Some((
                        discovery.lapic_physical,
                        discovery.ioapic_physical,
                        discovery.ioapic_gsi_base,
                        discovery.timer_gsi,
                        discovery.timer_active_low,
                        discovery.timer_level_triggered,
                        apic,
                    ));
                }
                Err(error) => {
                    crate::println!("kernel APIC validation failed: {:?}", error);
                    crate::println!(
                        "kernel LAPIC UEFI descriptor {:?}, IOAPIC descriptor {:?}",
                        unsafe { memory::firmware_descriptor_at(discovery.lapic_physical) },
                        unsafe { memory::firmware_descriptor_at(discovery.ioapic_physical) }
                    );
                    debugcon::write("VIBRIX: kernel APIC discovery rejected\r\n");
                }
            }
        }
        Err(()) => debugcon::write("VIBRIX: kernel ACPI or ECAM discovery rejected\r\n"),
    }

    // Static BSS backing is already supervisor RW/NX in the loader mappings.
    // SAFETY: sole boot CPU, IF=0, no interrupt or reentrant heap users.
    if unsafe { memory::heap::smoke_test() }.is_err() {
        panic!("early kernel heap validation failed");
    }
    debugcon::write("VIBRIX: kernel heap allocation and reuse verified\r\n");
    crate::println!("kernel heap: aligned allocations, RAM writes and reuse verified");

    // Physical GOP BAR is explicitly identity-mapped UC in the active PML4.
    // The large display draws actual glyph pixels over the old UEFI splash,
    // while the small or bitmask display retains the original minimal marker.
    match unsafe { framebuffer::draw_boot_marker(&info) } {
        Ok(banner) => {
            debugcon::write("VIBRIX: kernel framebuffer wrote pixels\r\n");
            if banner {
                debugcon::write("VIBRIX: kernel framebuffer status banner drawn\r\n");
            }
        }
        Err(()) => debugcon::write("VIBRIX: kernel framebuffer rejected\r\n"),
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
    {
        // The default interactive QEMU kernel activates the validated timer
        // route only after all temporary mapping-window users and fault probes.
        // Feature-probe kernels preserve the historical IF=0 environment.
        if !cfg!(any(
            feature = "breakpoint-probe",
            feature = "page-fault-probe",
            feature = "vm-write-probe",
            feature = "vm-unmap-probe"
        )) {
            let (
                lapic_physical,
                ioapic_physical,
                ioapic_gsi_base,
                timer_gsi,
                timer_active_low,
                timer_level_triggered,
                apic,
            ) = timer_setup.unwrap_or_else(|| panic!("validated APIC timer topology unavailable"));
            let route = arch::x86_64::irq::Route::new(
                timer_gsi,
                arch::x86_64::irq::TIMER_VECTOR,
                apic.lapic_id,
            )
            .unwrap_or_else(|_| panic!("timer route policy rejected"))
            .with_signal(timer_active_low, timer_level_triggered);
            // SAFETY: sole BSP, IF=0, all transient v3-window consumers are
            // complete; APIC addresses and GSI policy came from validated
            // MADT plus the immediately preceding architectural APIC probe.
            unsafe {
                arch::x86_64::apic::activate_pit_timer(
                    &info,
                    lapic_physical,
                    ioapic_physical,
                    ioapic_gsi_base,
                    apic.ioapic_max_redirection_entry,
                    route,
                )
            }
            .unwrap_or_else(|error| panic!("timer routing activation failed: {:?}", error));
            let before = arch::x86_64::irq::timer_ticks();
            // SAFETY: the only unmasked external source is the validated PIT
            // route into a permanent timer gate; handler state is atomic.
            unsafe { arch::x86_64::apic::enable_interrupts() };
            while arch::x86_64::irq::timer_ticks() == before {
                // SAFETY: IF=1 and the PIT route should wake this BSP. Failure
                // to deliver is intentionally observable as a QEMU timeout.
                unsafe { core::arch::asm!("hlt", options(nomem, nostack)) };
            }
            crate::println!("kernel timer: tick {}", arch::x86_64::irq::timer_ticks());
            debugcon::write("VIBRIX: kernel timer IRQ delivered\r\n");
        }

        // Development QEMU keyboard: read only legacy i8042 ports after
        // ExitBootServices; IRQs remain disabled and no USB HID is implied.
        let mut ps2 = arch::x86_64::ps2::SetOne::new();
        debugcon::write("VIBRIX: kernel PS2 polling ready\r\n");
        loop {
            // SAFETY: sole boot CPU, IF=0, i8042 data has no other consumer.
            if let Some(scan) = unsafe { arch::x86_64::ps2::poll_scancode() }
                && let Some(ascii) = ps2.feed(scan)
            {
                crate::println!("kernel PS2 ascii {}", ascii);
                debugcon::write("VIBRIX: kernel PS2 ASCII accepted\r\n");
            }
            core::hint::spin_loop();
        }
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
