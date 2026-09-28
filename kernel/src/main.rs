#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

mod arch;
#[cfg(not(feature = "panic-probe"))]
mod console;
mod debugcon;
mod device;
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
    let claimed_frames = match unsafe { memory::smoke_claim_two_frames() } {
        Ok(frames) => frames,
        Err(_) => {
            debugcon::write("VIBRIX: kernel conventional frame claims failed\r\n");
            loop {
                core::hint::spin_loop();
            }
        }
    };
    let memory_descriptor_count = info.memory_map_len / u64::from(info.memory_descriptor_size);
    debugcon::write("VIBRIX: kernel conventional frames allocated\r\n");

    // Legacy PCI config mechanism #1 reads segment-zero vendor/class/BAR
    // metadata without relying on firmware protocols after ExitBootServices.
    // This scans all 256 bus numbers but does NOT touch any device BAR
    // memory, enable bus mastering or identify the persistent boot USB.
    let mut shown = 0usize;
    let mut device_model = device::DiscoverySummary::default();
    let pci = unsafe {
        arch::x86_64::pci::discover_legacy_segment_zero(|device| {
            let identity = device::DeviceIdentity::Pci(device::PciIdentity {
                address: device::PciAddress {
                    segment: 0,
                    bus: device.bdf.bus,
                    device: device.bdf.device,
                    function: device.bdf.function,
                },
                vendor: device.vendor,
                device_id: device.id,
                class: device.class,
                subclass: device.subclass,
                programming_interface: device.programming_interface,
            });
            let _candidate = device_model.observe(identity);
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
    crate::println!(
        "Vibrix device model: {} devices, {} driver candidates, {} xHCI candidates, {} RTL8168 candidates",
        device_model.devices,
        device_model.driver_candidates,
        device_model.xhci_candidates,
        device_model.rtl8168_candidates
    );
    if device_model.devices == pci.devices {
        debugcon::write("VIBRIX: kernel device model populated\r\n");
    } else {
        debugcon::write("VIBRIX: kernel device model count mismatch\r\n");
    }
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
    let mut acpi_console = None;
    let acpi_mcfg = unsafe { parse_boot_rsdp(&info) }.and_then(|rsdp| {
        // SAFETY: sole boot CPU, IF=0, physical allocator initialized and
        // mapping-window smoke test has unmapped every temporary leaf.
        unsafe { arch::x86_64::acpi_runtime::inspect(&info, &rsdp) }.map_err(|error| {
            crate::println!("kernel ACPI/ECAM validation failed: {:?}", error);
        })
    });
    match acpi_mcfg {
        Ok(discovery) => {
            acpi_console = Some((
                discovery.allocations,
                discovery.ecam.devices,
                discovery.ioapics,
                discovery.timer_gsi,
            ));
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

    // Compile the IRQ path in every feature combination so all-feature
    // Clippy validates it. Feature-probe binaries take the false branch and
    // preserve IF=0; only the ordinary development kernel activates the PIT.
    // The default interactive QEMU kernel activates the validated timer
    // route only after all temporary mapping-window users and fault probes.
    // Feature-probe kernels preserve the historical IF=0 environment.
    if !cfg!(any(
        feature = "panic-probe",
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

    // Separate QEMU-only smoke configuration exercises the *real* kernel
    // panic handler after the ordinary post-firmware boot path succeeded.
    #[cfg(feature = "panic-probe")]
    panic!("VIBRIX: kernel panic probe");

    #[cfg(not(feature = "panic-probe"))]
    {
        // Development QEMU keyboard: read only legacy i8042 ports after
        // ExitBootServices; input remains polled after timer IRQ enablement.
        let mut ps2 = arch::x86_64::ps2::SetOne::new();
        let mut line = console::LineEditor::new();
        debugcon::write("VIBRIX: kernel PS2 polling ready\r\n");
        crate::print!("vibrix> ");
        debugcon::write("VIBRIX: kernel console prompt ready\r\n");
        loop {
            // SAFETY: sole boot CPU; no interrupt handler consumes i8042 data.
            if let Some(scan) = unsafe { arch::x86_64::ps2::poll_scancode() }
                && let Some(ascii) = ps2.feed(scan)
            {
                crate::println!("kernel PS2 ascii {}", ascii);
                debugcon::write("VIBRIX: kernel PS2 ASCII accepted\r\n");
                match line.feed(ascii) {
                    console::Edit::Echo(ch) => crate::print!("{}", char::from(ch)),
                    console::Edit::Erase => {
                        crate::print!("\x08 \x08");
                        debugcon::write("VIBRIX: kernel console backspace accepted\r\n");
                    }
                    console::Edit::Complete(bytes) => {
                        crate::println!();
                        match console::command(bytes) {
                            console::Command::Empty => {}
                            console::Command::Help => {
                                crate::println!("commands: help info mem pci acpi uptime");
                                debugcon::write("VIBRIX: kernel console command help\r\n");
                            }
                            console::Command::Info => {
                                crate::println!(
                                    "Vibrix kernel build {}",
                                    env!("CARGO_PKG_VERSION")
                                );
                                debugcon::write("VIBRIX: kernel console command info\r\n");
                            }
                            console::Command::Mem => {
                                crate::println!(
                                    "mem: descriptors={} claimed_frames={:#x},{:#x} early_heap_bytes={}",
                                    memory_descriptor_count,
                                    claimed_frames.0,
                                    claimed_frames.1,
                                    memory::heap::CAPACITY
                                );
                                debugcon::write("VIBRIX: kernel console command mem\r\n");
                            }
                            console::Command::Pci => {
                                crate::println!(
                                    "pci: devices={} bars={} xhci={} malformed_bars={}",
                                    pci.devices,
                                    pci.assigned_bars,
                                    pci.xhci_controllers,
                                    pci.malformed_bars
                                );
                                debugcon::write("VIBRIX: kernel console command pci\r\n");
                            }
                            console::Command::Acpi => {
                                if let Some((allocations, ecam_devices, ioapics, timer_gsi)) =
                                    acpi_console
                                {
                                    crate::println!(
                                        "acpi: mcfg_allocations={} ecam_bus0_devices={} ioapics={} timer_gsi={}",
                                        allocations,
                                        ecam_devices,
                                        ioapics,
                                        timer_gsi
                                    );
                                } else {
                                    crate::println!("acpi: discovery unavailable");
                                }
                                debugcon::write("VIBRIX: kernel console command acpi\r\n");
                            }
                            console::Command::Uptime => {
                                let ticks = arch::x86_64::irq::timer_ticks();
                                crate::println!(
                                    "uptime: {} ticks (~{}.{:02}s)",
                                    ticks,
                                    ticks / 100,
                                    ticks % 100
                                );
                                debugcon::write("VIBRIX: kernel console command uptime\r\n");
                            }
                            console::Command::Unknown => {
                                crate::println!("unknown command");
                                debugcon::write("VIBRIX: kernel console unknown command\r\n");
                            }
                        }
                        line.reset();
                        crate::print!("vibrix> ");
                    }
                    console::Edit::Full => crate::print!("\x07"),
                    console::Edit::Ignore => {}
                }
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
