#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

mod arch;
#[cfg(not(feature = "panic-probe"))]
mod console;
mod debugcon;
#[cfg(feature = "userspace-desktop")]
mod desktop_input;
mod framebuffer;
mod memory;
mod thread;
#[cfg(feature = "userspace-io-probe")]
mod userspace_io;

use core::panic::PanicInfo;
use vibrix_kernel::device;

// Same representation and validation code is compiled by both loader and kernel.
#[path = "../../shared/bootinfo.rs"]
#[allow(dead_code)]
mod bootinfo;
pub use bootinfo::BootInfo;

/// BootInfo is loader-owned and identity-mapped while the initial kernel
/// page tables are active. Check the v1-prefix version *before* reading the
/// v2/v3/v4 tails, then validate all remaining metadata as untrusted input.
///
/// # Safety
/// Loader must provide an aligned, mapped 168-byte BootInfo page that remains
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
    // SAFETY: version check above and loader's full v4 mapping/lifetime
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
            debugcon::write("VIBRIX: BootInfo v4 rejected by kernel\r\n");
            loop {
                core::hint::spin_loop();
            }
        }
    };
    debugcon::write("VIBRIX: kernel BootInfo v4 validated\r\n");
    if info.boot_usb_identity().is_some() {
        debugcon::write("VIBRIX: kernel boot USB identity available\r\n");
    }

    // Re-enumerate on each boot; a portable USB may boot on a different CPU.
    let _cpu = arch::x86_64::cpuid::discover();

    // Initialize GDT + TSS before any privilege-level transitions. IRQs
    // remain disabled; init provisions the permanent BSP RSP0 stack. IST
    // remains intentionally unconfigured.
    unsafe { arch::x86_64::gdt::init() };
    debugcon::write("VIBRIX: kernel GDT/TSS loaded\r\n");

    // Independent 16550 COM1 output now originates from the real kernel.
    arch::x86_64::serial::init();
    crate::println!("Vibrix kernel started.");
    debugcon::write("VIBRIX: kernel serial initialized\r\n");
    #[cfg(feature = "developer-mode")]
    {
        crate::println!(
            "Vibrix operational mode: developer (diagnostic features explicitly enabled)"
        );
        debugcon::write("VIBRIX: operational mode developer\r\n");
    }
    #[cfg(not(feature = "developer-mode"))]
    {
        crate::println!("Vibrix operational mode: standard");
        debugcon::write("VIBRIX: operational mode standard\r\n");
    }
    #[cfg(feature = "verbose-boot")]
    {
        crate::println!(
            "Vibrix verbose boot: bootinfo memory_map_bytes={} descriptor_bytes={} framebuffer={}x{} stride={}",
            info.memory_map_len,
            info.memory_descriptor_size,
            info.framebuffer_width,
            info.framebuffer_height,
            info.framebuffer_stride
        );
        debugcon::write("VIBRIX: verbose boot bootinfo reported\r\n");
    }

    #[cfg(feature = "qemu-debugcon")]
    vibrix_kernel::subsystem_self_test(|marker| {
        crate::println!("{}", marker);
        debugcon::write(marker);
        debugcon::write("\r\n");
    });

    // Install synchronous exception/IRQ gates while IF stays clear. The TSS
    // has a valid BSP RSP0 stack; no gate selects an IST stack yet.
    unsafe { arch::x86_64::idt::init() };
    debugcon::write("VIBRIX: kernel IDT installed\r\n");

    #[cfg(any(feature = "syscall-probe", feature = "process-syscall-probe"))]
    unsafe {
        arch::x86_64::syscall::init()
            .unwrap_or_else(|error| panic!("SYSCALL/SYSRETQ initialization failed: {:?}", error));
        debugcon::write("VIBRIX: kernel SYSCALL MSRs configured\r\n");
    }
    #[cfg(feature = "process-syscall-probe")]
    arch::x86_64::syscall::init_process_probe()
        .unwrap_or_else(|error| panic!("process syscall probe initialization failed: {:?}", error));

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
    let _claimed_frames = match unsafe { memory::smoke_claim_two_frames() } {
        Ok(frames) => frames,
        Err(_) => {
            debugcon::write("VIBRIX: kernel conventional frame claims failed\r\n");
            loop {
                core::hint::spin_loop();
            }
        }
    };
    let _memory_descriptor_count = info.memory_map_len / info.memory_descriptor_size;
    debugcon::write("VIBRIX: kernel conventional frames allocated\r\n");

    // Legacy PCI config mechanism #1 reads segment-zero vendor/class/BAR
    // metadata without relying on firmware protocols after ExitBootServices.
    // This scans all 256 bus numbers but does NOT touch any device BAR
    // memory, enable bus mastering or identify the persistent boot USB.
    let mut shown = 0usize;
    let mut device_model = device::DiscoverySummary::default();
    let mut compatibility = device::CompatibilitySummary::default();
    let mut driver_binder = device::Binder::new();
    let mut bind_failures = 0u32;
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
            compatibility.observe(identity);
            if device_model.observe(identity).is_some()
                && driver_binder.bind_identity(identity).is_err()
            {
                bind_failures = bind_failures.saturating_add(1);
            }
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
    crate::println!(
        "Vibrix driver bindings: {} total, {} xHCI, {} RTL8168, {} failures",
        driver_binder.len(),
        driver_binder.count_driver(device::DriverKind::Xhci),
        driver_binder.count_driver(device::DriverKind::Rtl8168),
        bind_failures
    );
    let driver_diagnostics =
        device::DiagnosticSummary::new(device_model, &driver_binder, bind_failures);
    crate::println!(
        "Vibrix driver diagnostics: discovered={} candidates={} bound={} missing_driver={} binding_failures={} healthy={}",
        driver_diagnostics.discovered,
        driver_diagnostics.candidates,
        driver_diagnostics.bound,
        driver_diagnostics.missing_driver,
        driver_diagnostics.binding_failures,
        driver_diagnostics.healthy()
    );
    debugcon::write("VIBRIX: kernel driver diagnostics ready\r\n");
    crate::println!(
        "Vibrix hardware compatibility: discovered={} driver_candidates={} missing_driver={} known_limitations={}",
        compatibility.discovered,
        compatibility.driver_candidates,
        compatibility.missing_driver,
        compatibility.limited_by_known_quirk
    );
    debugcon::write("VIBRIX: kernel hardware compatibility report ready\r\n");
    #[cfg(feature = "verbose-boot")]
    {
        crate::println!(
            "Vibrix verbose boot: pci devices={} bars={} xhci={} candidates={} bound={}",
            pci.devices,
            pci.assigned_bars,
            pci.xhci_controllers,
            device_model.driver_candidates,
            driver_binder.len()
        );
        debugcon::write("VIBRIX: verbose boot PCI summary reported\r\n");
    }
    #[cfg(all(
        feature = "xhci-init-probe",
        not(feature = "usb-enum-probe"),
        not(feature = "usb-hub-probe"),
        not(feature = "usb-hid-keyboard-probe"),
        not(feature = "usb-hid-mouse-probe"),
        not(feature = "usb-storage-probe")
    ))]
    {
        // SAFETY: still single-BSP with IF=0. PCI discovery identified the
        // controller and all temporary MMIO/RAM scratch mappings are retired
        // before ACPI/APIC mapping-window ownership begins.
        let xhci = unsafe { arch::x86_64::xhci::initialize(&info) }
            .unwrap_or_else(|error| panic!("xHCI initialization failed: {:?}", error));
        crate::println!(
            "kernel xHCI: {:02x}:{:02x}.{} bar={:#x} version={:#x} slots={} ports={} dboff={:#x} rtsoff={:#x}",
            xhci.bdf.bus,
            xhci.bdf.device,
            xhci.bdf.function,
            xhci.bar,
            xhci.version,
            xhci.max_slots,
            xhci.max_ports,
            xhci.doorbell_offset,
            xhci.runtime_offset
        );
        debugcon::write("VIBRIX: kernel xHCI reset and running\r\n");
    }

    #[cfg(all(
        feature = "usb-enum-probe",
        not(feature = "usb-hub-probe"),
        not(feature = "usb-hid-keyboard-probe"),
        not(feature = "usb-storage-probe")
    ))]
    {
        // SAFETY: bounded single-BSP enumeration probe owns the directly
        // attached QEMU device and retires its temporary mappings before the
        // later ACPI/APIC runtime users begin.
        let usb = unsafe { arch::x86_64::xhci::enumerate_first_device(&info) }
            .unwrap_or_else(|error| panic!("USB enumeration failed: {:?}", error));
        crate::println!(
            "kernel USB device: port={} slot={} speed={} vid={:04x} pid={:04x} class={:02x}:{:02x}:{:02x} mps0={}",
            usb.port,
            usb.slot_id,
            usb.speed_id,
            usb.vendor_id,
            usb.product_id,
            usb.class,
            usb.subclass,
            usb.protocol,
            usb.max_packet_size0
        );
        debugcon::write("VIBRIX: kernel USB device addressed and descriptor read\r\n");
    }

    #[cfg(all(
        feature = "usb-hub-probe",
        not(feature = "usb-hid-keyboard-probe"),
        not(feature = "usb-hid-mouse-probe"),
        not(feature = "usb-storage-probe")
    ))]
    {
        // SAFETY: the bounded probe exclusively owns the QEMU xHCI controller,
        // addresses the root-attached USB2 hub and retires temporary mappings
        // before later runtime mapping-window users.
        let hub = unsafe { arch::x86_64::xhci::inspect_first_hub(&info) }
            .unwrap_or_else(|error| panic!("USB hub probe failed: {:?}", error));
        crate::println!(
            "kernel USB hub: root_port={} slot={} ports={} child_port={} child_status={:#06x} pwr_good_units={}",
            hub.root_port,
            hub.slot_id,
            hub.downstream_ports,
            hub.child_port,
            hub.child_status,
            hub.power_good_units
        );
        debugcon::write("VIBRIX: kernel USB hub downstream port reset and enabled\r\n");
    }

    #[cfg(all(
        feature = "usb-hid-keyboard-probe",
        not(feature = "usb-hid-mouse-probe"),
        not(feature = "usb-storage-probe")
    ))]
    {
        // SAFETY: the bounded probe owns the directly attached QEMU USB
        // keyboard, configures HID Boot Protocol and observes real interrupt-IN
        // input reports before releasing all temporary xHCI mappings.
        let keyboard = unsafe { arch::x86_64::xhci::probe_hid_boot_keyboard(&info, 0x0b) }
            .unwrap_or_else(|error| panic!("USB HID keyboard probe failed: {:?}", error));
        crate::println!(
            "kernel USB HID keyboard: root_port={} slot={} interface={} endpoint={:#04x} mps={} interval={} modifiers={:#04x} usage={:#04x}",
            keyboard.root_port,
            keyboard.slot_id,
            keyboard.interface,
            keyboard.endpoint_address,
            keyboard.endpoint_max_packet,
            keyboard.interval,
            keyboard.modifiers,
            keyboard.usage
        );
        debugcon::write("VIBRIX: kernel USB HID keyboard input observed\r\n");
    }

    #[cfg(feature = "usb-hid-mouse-probe")]
    {
        // SAFETY: the bounded probe owns the directly attached QEMU USB mouse,
        // configures HID Boot Protocol and observes real interrupt-IN reports
        // before releasing all temporary xHCI mappings.
        let mouse = unsafe { arch::x86_64::xhci::probe_hid_boot_mouse(&info) }
            .unwrap_or_else(|error| panic!("USB HID mouse probe failed: {:?}", error));
        crate::println!(
            "kernel USB HID mouse: root_port={} slot={} interface={} endpoint={:#04x} mps={} interval={} buttons={:#04x} x={} y={}",
            mouse.root_port,
            mouse.slot_id,
            mouse.interface,
            mouse.endpoint_address,
            mouse.endpoint_max_packet,
            mouse.interval,
            mouse.buttons,
            mouse.x,
            mouse.y
        );
        debugcon::write("VIBRIX: kernel USB HID mouse motion observed\r\n");
    }

    #[cfg(all(
        feature = "usb-storage-probe",
        not(feature = "usb-hid-keyboard-probe"),
        not(feature = "usb-hid-mouse-probe"),
        not(feature = "usb-hub-probe")
    ))]
    {
        // SAFETY: the bounded probe owns one directly attached QEMU USB
        // mass-storage device. It configures native bulk endpoints, executes
        // BOT/SCSI commands, restores the tested block, disables the slot and
        // retires temporary mappings before later ACPI/APIC users.
        let storage = unsafe { arch::x86_64::xhci::probe_mass_storage(&info) }
            .unwrap_or_else(|error| panic!("USB mass-storage probe failed: {:?}", error));
        crate::println!(
            "kernel USB storage: root_port={} slot={} interface={} bulk_in={:#04x}/{} bulk_out={:#04x}/{} blocks={} block_bytes={} verified_lba={}",
            storage.root_port,
            storage.slot_id,
            storage.interface,
            storage.bulk_in_address,
            storage.bulk_in_max_packet,
            storage.bulk_out_address,
            storage.bulk_out_max_packet,
            storage.blocks,
            storage.block_bytes,
            storage.verified_lba
        );
        debugcon::write("VIBRIX: kernel USB mass-storage SCSI commands verified\r\n");
    }

    if device_model.devices == pci.devices {
        debugcon::write("VIBRIX: kernel device model populated\r\n");
    } else {
        debugcon::write("VIBRIX: kernel device model count mismatch\r\n");
    }
    if bind_failures == 0 {
        debugcon::write("VIBRIX: kernel driver binding registry ready\r\n");
    } else {
        debugcon::write("VIBRIX: kernel driver binding rejected candidate\r\n");
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
    let mut _acpi_console = None;
    let acpi_mcfg = unsafe { parse_boot_rsdp(&info) }.and_then(|rsdp| {
        // SAFETY: sole boot CPU, IF=0, physical allocator initialized and
        // mapping-window smoke test has unmapped every temporary leaf.
        unsafe { arch::x86_64::acpi_runtime::inspect(&info, &rsdp) }.map_err(|error| {
            crate::println!("kernel ACPI/ECAM validation failed: {:?}", error);
        })
    });
    match acpi_mcfg {
        Ok(discovery) => {
            _acpi_console = Some((
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
                    let bsp = vibrix_kernel::per_cpu::bind_bsp(u32::from(apic.lapic_id))
                        .unwrap_or_else(|error| panic!("per-CPU BSP binding failed: {:?}", error));
                    crate::println!(
                        "kernel per-CPU BSP: uid={} apic={} slots={}",
                        bsp.firmware_uid,
                        bsp.apic_id,
                        vibrix_kernel::per_cpu::slots().len()
                    );
                    crate::println!("VIBRIX: kernel per-CPU BSP bound");
                    debugcon::write("VIBRIX: kernel per-CPU BSP bound\r\n");
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

    #[cfg(feature = "qemu-debugcon")]
    {
        let stats = thread::smoke_test().unwrap_or_else(|error| {
            panic!("cooperative kernel thread validation failed: {:?}", error)
        });
        crate::println!(
            "kernel threads: cooperative A1 B1 A2 B2 switches={} completed={}",
            stats.switches,
            stats.completed
        );
        debugcon::write("VIBRIX: kernel cooperative threads verified\r\n");
    }

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

    #[cfg(feature = "userspace-desktop")]
    // SAFETY: firmware exited; sole BSP with IF=0 before input publication.
    unsafe {
        desktop_input::init()
    };

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
        // SAFETY: APIC setup has retained only its documented window slots;
        // IF is still clear and runtime VM scratch slot 511 is unused.
        unsafe { memory::managed::init_runtime(&info) }.unwrap_or_else(|error| {
            panic!("runtime managed VM initialization failed: {:?}", error)
        });
        debugcon::write("VIBRIX: kernel managed VM runtime initialized\r\n");
        #[cfg(feature = "address-space-probe")]
        unsafe {
            memory::address_space::init(&info).unwrap_or_else(|error| {
                panic!("userspace address-space initialization failed: {:?}", error)
            });
            debugcon::write("VIBRIX: kernel userspace CR3 prepared\r\n");
        }
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

        memory::managed::runtime_smoke_test()
            .unwrap_or_else(|error| panic!("runtime managed VM validation failed: {:?}", error));

        #[cfg(feature = "preempt-thread-probe")]
        {
            // SAFETY: one BSP, IF=1, and the permanent PIT/LAPIC route just
            // delivered successfully. The probe restores IF=1 before return.
            let stats = unsafe { thread::preemptive_smoke_test() }.unwrap_or_else(|error| {
                panic!("preemptive kernel thread validation failed: {:?}", error)
            });
            crate::println!(
                "kernel scheduler: preemptive switches={} preemptions={} completed={}",
                stats.switches,
                stats.preemptions,
                stats.completed
            );
            debugcon::write("VIBRIX: kernel preemptive scheduler verified\r\n");
        }

        #[cfg(all(feature = "ring3-probe", not(feature = "address-space-probe")))]
        {
            let (user_rip, user_rsp) = memory::managed::prepare_ring3_probe()
                .unwrap_or_else(|error| panic!("ring3 probe mapping failed: {:?}", error));
            debugcon::write("VIBRIX: kernel CPL3 mappings ready\r\n");
            crate::println!(
                "kernel ring3 probe: entering rip={:#x} rsp={:#x}",
                user_rip,
                user_rsp
            );
            // SAFETY: prepare_ring3_probe leaves guarded user RX/RW mappings
            // live; GDT/TSS/IDT are permanent and the DPL3 probe gate exists.
            unsafe { arch::x86_64::ring3::enter(user_rip, user_rsp) };
        }

        #[cfg(all(feature = "address-space-probe", not(feature = "elf-load-probe")))]
        {
            // SAFETY: the feature-gated owner was initialized pre-STI.
            let probe =
                unsafe { memory::address_space::activate_probe() }.unwrap_or_else(|error| {
                    panic!("address-space probe activation failed: {:?}", error)
                });
            crate::println!(
                "kernel address space probe: kernel_cr3={:#x} user_cr3={:#x} rip={:#x} rsp={:#x}",
                probe.kernel_root,
                probe.user_root,
                probe.user_rip,
                probe.user_rsp
            );
            #[cfg(feature = "user-stack-guard-probe")]
            debugcon::write("VIBRIX: kernel userspace stack guard write armed\r\n");
            // SAFETY: activate_probe staged and validated the private mappings.
            // enter_probe moves to a higher-half kernel stack before loading
            // the private CR3, then immediately enters CPL3.
            unsafe { memory::address_space::enter_probe(probe) };
        }

        #[cfg(all(
            feature = "elf-load-probe",
            not(feature = "rust-init-probe"),
            not(feature = "rust-shell-probe"),
            not(feature = "package-metadata-probe")
        ))]
        {
            // SAFETY: the private address-space owner is initialized and the
            // ELF loader stages only into its owned inactive lower-half root.
            let probe = unsafe { memory::address_space::load_elf_probe() }
                .unwrap_or_else(|error| panic!("userspace ELF load probe failed: {:?}", error));
            debugcon::write("VIBRIX: kernel userspace ELF loaded\r\n");
            crate::println!(
                "kernel ELF probe: kernel_cr3={:#x} user_cr3={:#x} entry={:#x} rsp={:#x}",
                probe.kernel_root,
                probe.user_root,
                probe.user_rip,
                probe.user_rsp
            );
            // SAFETY: load_elf_probe committed W^X mappings and a guarded stack.
            unsafe { memory::address_space::enter_probe(probe) };
        }

        #[cfg(all(
            feature = "rust-init-probe",
            not(feature = "rust-shell-probe"),
            not(feature = "package-metadata-probe")
        ))]
        {
            // SAFETY: build-qemu produced the fixed-layout no_std Rust init ELF
            // before compiling the kernel. The same validated ImageSink stages
            // it into the private lower-half CR3 with final W^X permissions.
            let probe = unsafe { memory::address_space::load_elf_probe() }
                .unwrap_or_else(|error| panic!("Rust init ELF load failed: {:?}", error));
            debugcon::write("VIBRIX: kernel Rust init ELF loaded\r\n");
            crate::println!(
                "kernel Rust init: kernel_cr3={:#x} user_cr3={:#x} entry={:#x} rsp={:#x}",
                probe.kernel_root,
                probe.user_root,
                probe.user_rip,
                probe.user_rsp
            );
            // SAFETY: the compiled init image and guarded stack were validated.
            unsafe { memory::address_space::enter_probe(probe) };
        }

        #[cfg(all(feature = "package-metadata-probe", not(feature = "rust-shell-probe")))]
        {
            // SAFETY: build-qemu produced the fixed-layout no_std package probe
            // before compiling the kernel. The normal ELF loader validates its
            // W^X mappings and guarded userspace stack before CPL3 entry.
            let probe = unsafe { memory::address_space::load_elf_probe() }
                .unwrap_or_else(|error| panic!("package metadata ELF load failed: {:?}", error));
            debugcon::write("VIBRIX: kernel package metadata ELF loaded\r\n");
            crate::println!(
                "kernel package metadata: kernel_cr3={:#x} user_cr3={:#x} entry={:#x} rsp={:#x}",
                probe.kernel_root,
                probe.user_root,
                probe.user_rip,
                probe.user_rsp
            );
            unsafe { memory::address_space::enter_probe(probe) };
        }

        #[cfg(feature = "rust-shell-probe")]
        {
            #[cfg(feature = "rescue-mode")]
            {
                debugcon::write("VIBRIX: rescue single-user mode active\r\n");
                crate::println!("Vibrix rescue mode: single-user root administration");
            }
            debugcon::write("VIBRIX: kernel process probe initialized for userspace I/O\r\n");
            userspace_io::init()
                .unwrap_or_else(|error| panic!("userspace stdio init failed: {:?}", error));
            // SAFETY: build-qemu produced the fixed-layout no_std Rust shell
            // ELF before compiling the kernel; load_elf_probe validates it.
            let probe = unsafe { memory::address_space::load_elf_probe() }
                .unwrap_or_else(|error| panic!("Rust shell ELF load failed: {:?}", error));
            #[cfg(feature = "userspace-desktop")]
            debugcon::write("VIBRIX: kernel Rust desktop ELF loaded\r\n");
            #[cfg(not(feature = "userspace-desktop"))]
            debugcon::write("VIBRIX: kernel Rust shell ELF loaded\r\n");
            crate::println!(
                "kernel Rust {}: kernel_cr3={:#x} user_cr3={:#x} entry={:#x} rsp={:#x}",
                if cfg!(feature = "userspace-desktop") {
                    "desktop"
                } else {
                    "shell"
                },
                probe.kernel_root,
                probe.user_root,
                probe.user_rip,
                probe.user_rsp
            );
            // SAFETY: the compiled shell image and guarded stack were validated.
            unsafe { memory::address_space::enter_probe(probe) };
        }
    }

    // Separate QEMU-only smoke configuration exercises the *real* kernel
    // panic handler after the ordinary post-firmware boot path succeeded.
    #[cfg(feature = "panic-probe")]
    panic!("VIBRIX: kernel panic probe");

    #[cfg(not(feature = "panic-probe"))]
    {
        // Development QEMU keyboard: read only legacy i8042 ports after
        // ExitBootServices; input remains polled after timer IRQ enablement.
        let mut root =
            vibrix_kernel::vfs::console::BootstrapRoot::new().expect("bootstrap memory filesystem");
        let mut devices = vibrix_kernel::vfs::devfs::DevFs::new();
        let mut files = vibrix_kernel::vfs::console::bootstrap(&mut root, &mut devices)
            .expect("bootstrap filesystem mounts");
        struct FsOutput;
        impl core::fmt::Write for FsOutput {
            fn write_str(&mut self, text: &str) -> core::fmt::Result {
                crate::print!("{}", text);
                Ok(())
            }
        }
        let mut ps2 = arch::x86_64::ps2::SetOne::new();
        let mut line = console::LineEditor::new();
        debugcon::write("VIBRIX: kernel PS2 polling ready\r\n");
        crate::print!("vibrix> ");
        debugcon::write("VIBRIX: kernel console prompt ready\r\n");
        let mut tty_input_seen = false;
        loop {
            // SAFETY: sole boot CPU; no interrupt handler consumes i8042 data.
            if let Some(scan) = unsafe { arch::x86_64::ps2::poll_scancode() }
                && let Some(ascii) = ps2.feed(scan)
            {
                crate::println!("kernel PS2 ascii {}", ascii);
                debugcon::write("VIBRIX: kernel PS2 ASCII accepted\r\n");
                if files.device_input("/dev/tty", ascii).is_ok() && !tty_input_seen {
                    tty_input_seen = true;
                    debugcon::write("VIBRIX: kernel PS2 byte entered TTY\r\n");
                }
                match line.feed(ascii) {
                    console::Edit::Echo(ch) => crate::print!("{}", char::from(ch)),
                    console::Edit::Erase => {
                        crate::print!("\x08 \x08");
                        debugcon::write("VIBRIX: kernel console backspace accepted\r\n");
                    }
                    console::Edit::Complete(bytes) => {
                        crate::println!();
                        // The development console and /dev/tty observe the same
                        // physical PS/2 stream. Drain the committed canonical
                        // line here so repeated commands cannot fill the TTY.
                        if let Ok(tty) =
                            files.open("/dev/tty", vibrix_kernel::vfs::files::Open::READ)
                        {
                            let mut tty_line = [0u8; 128];
                            let _ = files.read(tty, &mut tty_line);
                            let _ = files.close(tty);
                        }
                        match console::command(bytes) {
                            console::Command::Empty => {}
                            console::Command::Help => {
                                crate::println!(
                                    "commands: help clear info mem pci acpi uptime reboot"
                                );
                                crate::println!(
                                    "RAM files: ls [path], cat path, write path text, mkdir path, rm path, pipe"
                                );
                                debugcon::write("VIBRIX: kernel console command help\r\n");
                            }
                            console::Command::Clear => {
                                crate::print!("\x1b[2J\x1b[H");
                                debugcon::write("VIBRIX: kernel console command clear\r\n");
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
                                    _memory_descriptor_count,
                                    _claimed_frames.0,
                                    _claimed_frames.1,
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
                                    _acpi_console
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
                            console::Command::Reboot => {
                                crate::println!("reboot: requesting i8042 reset");
                                debugcon::write("VIBRIX: kernel console command reboot\r\n");
                                // SAFETY: QEMU q35 exposes the legacy i8042 used
                                // by this same development console. This command
                                // is terminal and intentionally never returns.
                                unsafe { arch::x86_64::reset::reboot_i8042() }
                            }
                            console::Command::Unknown => {
                                let text = core::str::from_utf8(bytes).unwrap_or("");
                                if vibrix_kernel::vfs::console::execute(
                                    &mut files,
                                    text,
                                    &mut FsOutput,
                                ) {
                                    debugcon::write(
                                        "VIBRIX: kernel filesystem command completed\r\n",
                                    );
                                } else {
                                    crate::println!("unknown command");
                                    debugcon::write("VIBRIX: kernel console unknown command\r\n");
                                }
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
