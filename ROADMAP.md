# Vibrix Roadmap

Vibrix is an independent Rust-native Unix-like operating system that **lives on persistent USB storage**.

There is no internal-disk edition. A checkbox is completed only when functionality is implemented and demonstrated on its stated target.

## M0 — Bootstrap
- [x] Project identity and independence policy
- [x] Rust-native system policy
- [x] Define persistent USB-only product scope
- [x] Repository structure
- [x] Hardware inventory tooling
- [x] Sanitized Target 001 profile
- [x] AI/agent contribution policy
- [x] GitHub Actions CI definition
- [x] CI green on loader + kernel + QEMU smoke test
- [x] Adopt project license
- [x] Architecture decision record process
- [x] Preserve executable bits for scripts

## M1 — First QEMU boot
- [x] Rust UEFI application
- [x] Dependency-free UEFI console output
- [x] OVMF/QEMU development environment
- [x] First successful QEMU boot
- [x] Separate Rust kernel crate
- [x] Built-in bare-metal x86-64 Rust target
- [x] Initial linker layout
- [x] BootInfo v1 design

## M2 — Firmware-to-kernel handoff
- [x] Loader filesystem access
- [x] Locate /vibrix/kernel.elf
- [x] Vibrix-owned ELF64 parser
- [x] Validate ELF class/machine/endianness
- [x] Parse PT_LOAD headers
- [x] Allocate/copy kernel segments and zero BSS
- [x] Establish initial kernel mappings
- [x] Discover GOP framebuffer
- [x] Discover ACPI RSDP
- [x] Capture final UEFI memory map
- [x] Populate BootInfo
- [x] ExitBootServices
- [x] Transfer to vibrix_kernel_entry
- [x] Kernel framebuffer output without UEFI
- [x] Kernel panic output

**Verified M2 QEMU/OVMF handoff (PR #49):** [Actions run 36337520346](https://github.com/mixutin/Vibrix/actions/runs/36337520346) executed the UEFI loader, refreshed the final map, populated BootInfo v2, successfully exited boot services, switched to verified kernel mappings and dedicated stack, and entered the standalone higher-half kernel. The **kernel's own** debugcon markers confirm BootInfo validation, GDT/TSS initialization, native COM1 output and uncached GOP framebuffer pixel writes; the separate QEMU serial log contains `Vibrix kernel started.`. [Run 36337648665](https://github.com/mixutin/Vibrix/actions/runs/36337648665) also booted an optional panic-probe kernel and observed real post-firmware panic messages over QEMU debugcon **and** COM1. These are QEMU observations, not Target 001 physical boot or a native USB storage/filesystem/interrupts/userspace milestone. At that M2 checkpoint, full ACPI table mappings, native memory/exception subsystems, removable USB reacquisition and persistence were pending. **Since then**, PRs #52/#54 have added and independently QEMU-tested a limited single-CPU IDT, page-fault diagnostics and monotonic physical-only frame allocator; see M3 below. Full ACPI SDT/MCFG mapping, virtual memory, native USB and persistence remain pending.

**Exit:** standalone kernel prints after ExitBootServices without firmware boot services.

## M3 — x86-64 kernel foundations
- [x] Architecture module layout
- [x] CPUID discovery
- [x] Serial/debug console
- [x] GDT + TSS
- [x] IDT + exception handlers
- [x] Page-fault diagnostics
- [x] Physical frame allocator
- [ ] Virtual memory manager
- [x] Kernel heap
- [ ] Local APIC + I/O APIC
- [ ] Timer + interrupt routing
- [x] Explicit unsafe-code boundaries

**Verified M3 mapping groundwork (PR #62):** [Actions run 36343397525](https://github.com/mixutin/Vibrix/actions/runs/36343397525) passed seven QEMU configurations, including actual supervisor-write and post-unmap page faults. BootInfo v3 provides one bounded 2 MiB mapping window; the kernel maps newly owned RAM frames, changes write permissions, unmaps and remaps with local TLB invalidation. CR0.WP is enabled. The general virtual-memory manager checkbox stays **unchecked**: dynamic page tables, address-space management, frame reuse and SMP shootdowns remain unfinished. See [ADR 0009](docs/decisions/0009-early-mapping-window.md).

**Verified M3 bounded early heap (PR #60):** [Actions run 36342588023](https://github.com/mixutin/Vibrix/actions/runs/36342588023) passed production heap host tests, formatting, target Clippy/builds and five QEMU boots (normal, virtual xHCI, panic, breakpoint and page fault). The real kernel allocates aligned spans, writes/reads their RAM backing, frees and reuses an allocation, and reports success independently over debugcon and COM1. This is a **64 KiB fixed-capacity early kernel heap** over reserved, already mapped BSS, with free/reuse. It is single-CPU/IRQs-off, has no global Rust allocator and cannot grow from physical frames. Virtual memory, general-purpose/SMP allocation and physical Target 001 tests remain separate work. See [early heap contract](docs/EARLY_HEAP.md).

**Verified M3 early frame allocator (PR #54):** [Actions run
36339836880](https://github.com/mixutin/Vibrix/actions/runs/36339836880)
compiled the production memory-map allocator, ran malformed-map/ownership
host tests and booted the real kernel after ExitBootServices. Its
kernel-only markers confirm initialization from the retained final UEFI v1
descriptor map and issuance of two different, nonzero, page-aligned
`EfiConventionalMemory` physical frames; both normal and panic-probe
QEMU boots passed. The global allocator is deliberately monotonic,
single-boot-CPU and interrupts-disabled. **This checkbox does not imply**
new physical frames are identity-mapped, zeroed, releasable, SMP-safe, or
available to a heap/userspace: those are separate uncompleted memory tasks.


**Verified M3 early exception handling (PR #52):**
[QEMU run 36340579141](https://github.com/mixutin/Vibrix/actions/runs/36340579141)
synchronized the IDT work with the real kernel's M2 handoff, RSDP and
physical-frame allocator and exercised four separate QEMU builds:
normal kernel, native panic probe, actual CPU `int3` breakpoint
(returning from #BP), and an actual unmapped page read triggering #PF.
The real kernel recorded CR2 = `0x10000000000`, RIP, raw error code and
decoded P/W/U/RSVD/I bits on independent COM1 output, with separate
debugcon proof. The permanent 256-gate IDT handles synchronous #BP,
#DF, #GP and #PF; only #BP and #PF were fault-injected in QEMU.
**Interrupt flag remains cleared** and unconfigured IRQ vectors, APIC,
double-fault IST/privilege stacks, ring-3 exceptions, timer routing
and SMP handling are separate unchecked work. Target 001 is untested.

## M4 — Device discovery
- [x] ACPI parser
- [x] MCFG/ECAM
- [x] PCI enumeration
- [x] BAR parsing
- [ ] MSI/MSI-X
- [ ] Device/driver model
- [ ] Driver binding

**Verified M4 read-only ECAM bootstrap (PR #67):**
[Actions run 36346144132](https://github.com/mixutin/Vibrix/actions/runs/36346144132)
passed formatting, production UEFI-map, ACPI, ECAM and virtual-mapping
host tests, both target Clippy/builds and seven post-firmware QEMU boots.
The native kernel used a checksummed real MCFG allocation for
**segment-zero bus-zero function-zero configuration reads**, mapping each
4 KiB ECAM page supervisor-read-only/NX/UC for aligned volatile vendor
and class dwords, then unmapping. It verified the CPU's actual PAT index 3
and accepted only UEFI reserved or MMIO pages advertising UC capability,
not conventional/loader/ACPI RAM or runtime memory. QEMU OVMF labels this
ECAM aperture `EfiReservedMemoryType` (type 0), UC attribute 1.
QEMU's native COM1 reported **4 function-zero devices** on bus zero
without a virtual xHCI, or **5 devices and 1 xHCI controller** when
`qemu-xhci` was added; seven debugcon boot configurations passed.
This checkbox is **bounded MCFG-selected ECAM read-only discovery**,
not full 256-bus/multifunction/multisegment scanning, PCIe extended
register enumeration, resource sizing, configuration writes, MSI,
native xHCI activation, DMA, interrupts or physical Target 001 support.

**Verified M4 real ACPI SDT parsing (PR #64):** [Actions run
36344773356](https://github.com/mixutin/Vibrix/actions/runs/36344773356)
passed production ACPI/firmware-map host tests, formatting, both target
Clippy/builds and seven post-firmware QEMU configurations. The real
kernel mapped retained WB ACPI RAM as supervisor-read-only/NX in BootInfo
v3's temporary mapping window, checked the full XSDT and child SDT
checksums and parsed **one actual MCFG allocation** in QEMU q35.
Independent kernel debugcon and COM1 confirmed it on normal, virtual
xHCI, panic, breakpoint, page fault and VM protection/unmap probes.
Each table is bounded to 1 MiB; at most 64 XSDT/RSDT entries are read
in this early bootstrap. This is real, limited ACPI firmware table
discovery, not AML interpretation or general ACPI namespace support.
**MCFG/ECAM stays unchecked:** the discovered ECAM memory is only a
physical number, not mapped PCI configuration MMIO or a driver.
No MSI, APIC routing, SMP or Target 001 claims follow from this.

**Verified M4 native PCI segment-zero scan (PR #59):** [Actions run
36341931987](https://github.com/mixutin/Vibrix/actions/runs/36341931987)
compiled the production read-only PCI mechanism-#1 scanner and BAR decoder,
passed host fixtures for multifunction buses, absent devices and 32-/64-bit
BAR pairs, and ran the actual post-ExitBootServices kernel under QEMU q35.
Independent kernel debugcon markers and COM1 observed **6 PCI functions,
9 assigned BARs, 0 xHCI controllers** on that particular QEMU setup, in
normal and exception-probe regression boots. A second QEMU boot with an
explicit `qemu-xhci` PCI controller exercised the same native scanner and
observed **7 PCI functions, 10 assigned BARs and 1 xHCI controller** in
[run 36342144203](https://github.com/mixutin/Vibrix/actions/runs/36342144203).
This M4 enumeration checkbox
is **legacy PCI segment zero on x86-64**, not other PCI segments or extended
configuration space: ACPI MCFG/ECAM, MSI/MSI-X, driver binding, BAR resource
sizing/MMIO activation, a native xHCI driver and Target 001 remain separate
unchecked tasks. Reading an assigned BAR is not using its memory or
writing an internal disk.


## M4.5 — Interactive kernel console

This is an intentionally small bridge between device discovery and the real
M5/M6 userspace stack. It exists so Vibrix becomes directly operable during
kernel development; it does **not** replace Ring 3, syscalls, VFS, TTY or the
future userspace shell.

- [ ] Hardware interrupt path usable in QEMU
- [ ] Monotonic timer source available to the console
- [x] QEMU keyboard input reaches the kernel without UEFI Boot Services
- [ ] Kernel console input buffer and line editing
- [ ] Command parser and dispatch table
- [ ] `help`
- [ ] `clear`
- [ ] `info` / build information
- [ ] `mem` memory diagnostics
- [ ] `pci` PCI discovery output
- [ ] `acpi` ACPI discovery output
- [ ] `uptime`
- [ ] `reboot`
- [ ] Unknown-command and malformed-input handling
- [ ] QEMU smoke test proves prompt → input → command → output

**Verified M4.5 QEMU keyboard input (PR #70):**
[Actions run 36347623002](https://github.com/mixutin/Vibrix/actions/runs/36347623002)
compiled and host-tested the first-party PS/2 scan-code decoder and built
the production post-firmware keyboard polling path. QEMU's host monitor
sent **real virtual `h` and Return keypresses** only after the independent
kernel-side `VIBRIX: kernel PS2 polling ready` marker; the kernel's
native COM1 logged two *distinct exact lines*,
`kernel PS2 ascii 104` and `kernel PS2 ascii 10`.
The initial test incorrectly accepted `104` as a prefix match for
`10`, and that false-positive was corrected before claiming this item.
The successful kernel/QEMU job also passed normal/xHCI boots and all fault
probes. This is a **polled QEMU i8042 set-one ASCII subset** on the single
CPU with IF=0, not USB HID, IRQ-driven keyboard input, line editing,
a command parser, TTY, userspace shell or Target 001 PS/2 hardware.

**Exit:** after `ExitBootServices`, QEMU reaches a `vibrix>` prompt, accepts
real keyboard input and executes diagnostic commands entirely in the Vibrix
kernel. The console is a development milestone only; the M6 Rust shell remains
the first real userspace CLI.


### Target 001
- [ ] AMD xHCI 1022:43ee
- [ ] AMD xHCI 1022:149c
- [ ] Realtek Ethernet 10ec:8168
- [ ] Navi 23 GPU 1002:73ff
- [ ] Samsung NVMe 144d:a808 as optional data device
- [ ] AMD AHCI 1022:43eb as optional data device

## M5 — Processes and syscalls
- [ ] Kernel threads
- [ ] Context switching
- [ ] Preemptive scheduler
- [ ] Ring 3 userspace
- [ ] Userspace address spaces
- [ ] Vibrix syscall ABI v1
- [ ] syscall/sysret
- [ ] Native Rust syscall library
- [ ] PID/process lifecycle
- [ ] Executable loading
- [ ] argv/environment
- [ ] wait/exit

**Exit:** PID 1 executes in userspace and makes Vibrix syscalls.

## M6 — VFS and early userspace
- [ ] File descriptors
- [ ] VFS
- [ ] In-memory bootstrap filesystem
- [ ] /dev
- [ ] pipes
- [ ] TTY
- [ ] Rust init
- [ ] Rust shell
- [ ] Core utilities: cat, echo, ls, pwd, cd, mkdir, cp, mv, rm, ps, kill

**Exit:** boot to an interactive Vibrix userspace shell.

## M7 — USB platform
- [ ] xHCI initialization
- [ ] USB device enumeration
- [ ] USB hub support
- [ ] USB HID keyboard
- [ ] USB HID mouse
- [ ] USB mass-storage transport
- [ ] SCSI transparent command subset for mass storage
- [ ] Block-device abstraction
- [ ] Detect the boot USB device robustly
- [ ] Read/write blocks on the Vibrix USB device

**Exit:** Vibrix can access the same removable USB device it booted from after leaving firmware services.

## M8 — Vibrix filesystem
- [ ] On-disk specification
- [ ] Superblock/allocation metadata
- [ ] files/directories
- [ ] permissions/timestamps
- [ ] crash-consistency design
- [ ] formatter + recovery tool
- [ ] VFS driver
- [ ] persistent root mounted from USB

**Exit:** files created under the Vibrix root filesystem survive shutdown and reboot.

## M9 — Persistent USB operating system
- [x] GPT tooling

**GPT tooling evidence (host only):** The read-only `tools/inspect-gpt.rs`
validates both GPT copies, metadata, unique GUIDs and overlaps. The
new `tools/create-usb-image.rs` creates only a fresh **regular-file**
protective-MBR + reciprocal GPT image with distinct disk/partition GUIDs,
blank ESP and data partition placeholders, and no existing-file overwrite.
CI tests both 512/4096-byte sectors by passing each generated image through
the independent inspector. This checkbox means **offline GPT tooling**,
not a bootable ESP, formatted root, USB device provisioning or QEMU native
USB persistence. Those remain separate unchecked M9/M7 tasks.

- [ ] EFI System Partition layout
- [ ] Vibrix USB system partition layout
- [ ] Persistent root
- [ ] Persistent /home
- [ ] Persistent package database
- [ ] RAM-backed /tmp and runtime state
- [ ] Flash-write reduction
- [ ] Hardware rediscovery every boot
- [ ] Portable configuration policy
- [ ] Safe USB provisioning/imaging tool
- [ ] Recovery partition/environment
- [ ] System update + rollback strategy
- [ ] Target 001 real USB boot
- [ ] Move the same USB drive between two compatible machines

**Exit:** boot from USB, modify files/configuration/apps, power off, move or reboot the drive, and retain all state without touching an internal system disk.

## M10 — Networking
- [ ] NIC abstraction
- [ ] RTL8168-family driver
- [ ] Ethernet + ARP
- [ ] IPv4 + ICMP
- [ ] UDP
- [ ] DHCP
- [ ] DNS
- [ ] TCP
- [ ] sockets
- [ ] network utilities

## M11 — Security and multi-user
- [ ] users/groups/credentials
- [ ] permissions
- [ ] secure random
- [ ] W^X
- [ ] userspace ASLR
- [ ] stack protections
- [ ] IOMMU
- [ ] secure updates
- [ ] optional USB system encryption design

## M12 — SMP and performance
- [ ] CPU enumeration
- [ ] AP startup
- [ ] per-CPU structures
- [ ] SMP scheduler
- [ ] synchronization
- [ ] TLB shootdowns
- [ ] profiling
- [ ] Target 001 8C/16T validation

## M13 — Audio and graphics
- [ ] HDA + basic PCM
- [ ] USB audio
- [ ] graphics architecture
- [ ] framebuffer userspace API
- [ ] compositor/display-server design
- [ ] Navi 23 modesetting research
- [ ] native modesetting
- [ ] acceleration
- [ ] GUI toolkit

## M14 — Packages and development
- [ ] Package format/database/dependencies
- [ ] package manager
- [ ] signed repositories
- [ ] ports/build recipes
- [ ] editor/developer tooling
- [ ] compiler bootstrap plan

## M15 — Self-hosting
- [ ] Compile a Rust userspace program on Vibrix
- [ ] Toolchain usable on Vibrix
- [ ] Build userspace on Vibrix
- [ ] Build kernel on Vibrix
- [ ] Produce a bootable Vibrix USB image from Vibrix

## Optional later storage support

Internal NVMe/SATA disks may be supported as user-accessible **data devices**. They are not Vibrix root/system installation targets.

## Future
- [ ] aarch64 design
- [ ] aarch64 UEFI boot
- [ ] architecture-independent driver boundaries

## Early non-goals

Do not prioritize internal-disk installation, desktop polish, browsers, GPU acceleration, broad hardware support, POSIX completeness or Linux binary compatibility before the USB-root kernel foundation is reliable.
