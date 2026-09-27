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
- [ ] Adopt project license
- [ ] Architecture decision record process
- [ ] Preserve executable bits for scripts

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
- [ ] Allocate/copy kernel segments and zero BSS
- [ ] Establish initial kernel mappings
- [ ] Discover GOP framebuffer
- [ ] Discover ACPI RSDP
- [ ] Capture final UEFI memory map
- [ ] Populate BootInfo
- [ ] ExitBootServices
- [ ] Transfer to vibrix_kernel_entry
- [ ] Kernel framebuffer output without UEFI
- [ ] Kernel panic output

**Current verified checkpoint:** GitHub Actions builds loader + kernel and boots QEMU far enough to load and validate /vibrix/kernel.elf and parse PT_LOAD headers.

**Exit:** standalone kernel prints after ExitBootServices without firmware boot services.

## M3 — x86-64 kernel foundations
- [ ] Architecture module layout
- [ ] CPUID discovery
- [ ] Serial/debug console
- [ ] GDT + TSS
- [ ] IDT + exception handlers
- [ ] Page-fault diagnostics
- [ ] Physical frame allocator
- [ ] Virtual memory manager
- [ ] Kernel heap
- [ ] Local APIC + I/O APIC
- [ ] Timer + interrupt routing
- [ ] Explicit unsafe-code boundaries

## M4 — Device discovery
- [ ] ACPI parser
- [ ] MCFG/ECAM
- [ ] PCI enumeration
- [ ] BAR parsing
- [ ] MSI/MSI-X
- [ ] Device/driver model
- [ ] Driver binding

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
- [ ] GPT tooling
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
