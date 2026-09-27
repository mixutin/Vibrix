# Vibrix Roadmap

Vibrix is an independent Rust-native Unix-like operating system built from scratch as an experiment in how far AI-assisted engineering can go.

A checkbox is completed only when functionality is implemented and demonstrated on its stated target.

## M0 — Bootstrap
- [x] Project identity and independence policy
- [x] Rust-native system policy
- [x] Persistent portable + installed editions
- [x] Repository structure
- [x] Hardware inventory tooling
- [x] Sanitized Target 001 profile
- [x] AI/agent contribution policy
- [ ] Adopt project license
- [ ] Architecture decision record process
- [ ] Automated formatting/lint checks

## M1 — First QEMU boot
- [x] Rust UEFI application
- [x] Dependency-free UEFI console output
- [x] OVMF/QEMU development environment
- [x] First successful QEMU boot
- [x] Separate Rust kernel crate
- [x] Custom x86-64 kernel target
- [x] Initial linker layout
- [x] BootInfo v1 design
- [ ] Reproducible loader + kernel CI build
- [ ] Preserve executable bits for scripts

## M2 — Firmware-to-kernel handoff
- [ ] Loader filesystem access
- [ ] Locate /vibrix/kernel.elf
- [ ] Vibrix-owned ELF64 parser
- [ ] Validate ELF class/machine/endianness
- [ ] Parse PT_LOAD headers
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
- [ ] Samsung NVMe 144d:a808
- [ ] AMD xHCI 1022:43ee
- [ ] AMD xHCI 1022:149c
- [ ] AMD AHCI 1022:43eb
- [ ] Realtek Ethernet 10ec:8168
- [ ] Navi 23 GPU 1002:73ff

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

## M7 — USB and storage
- [ ] xHCI initialization
- [ ] USB enumeration
- [ ] USB HID keyboard/mouse
- [ ] USB mass storage
- [ ] Block-device abstraction
- [ ] NVMe initialization/queues/read/write
- [ ] AHCI discovery + SATA I/O

## M8 — Vibrix filesystem
- [ ] On-disk specification
- [ ] Superblock/allocation metadata
- [ ] files/directories
- [ ] permissions/timestamps
- [ ] crash-consistency design
- [ ] formatter + recovery tool
- [ ] VFS driver
- [ ] persistent root

**Exit:** files survive shutdown and reboot.

## M9 — Vibrix Portable
- [ ] GPT tooling
- [ ] EFI System Partition creation
- [ ] Portable disk layout
- [ ] Robust boot/root device identity
- [ ] Persistent USB root + /home
- [ ] RAM-backed volatile paths
- [ ] Flash-write reduction
- [ ] Hardware rediscovery
- [ ] Safe portable installer/imager
- [ ] Target 001 USB boot

**Exit:** move/reboot the USB system and retain files, programs and configuration.

## M10 — Installed edition
- [ ] Safe disk enumeration
- [ ] Destructive-action confirmations
- [ ] GPT/filesystem creation
- [ ] Install system + UEFI loader
- [ ] NVMe installation
- [ ] SATA installation
- [ ] Recovery
- [ ] Upgrade/rollback

## M11 — Networking
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

## M12 — Security and multi-user
- [ ] users/groups/credentials
- [ ] permissions
- [ ] secure random
- [ ] W^X
- [ ] userspace ASLR
- [ ] stack protections
- [ ] IOMMU
- [ ] secure updates
- [ ] optional disk-encryption design

## M13 — SMP and performance
- [ ] CPU enumeration
- [ ] AP startup
- [ ] per-CPU structures
- [ ] SMP scheduler
- [ ] synchronization
- [ ] TLB shootdowns
- [ ] profiling
- [ ] Target 001 8C/16T validation

## M14 — Audio and graphics
- [ ] HDA + basic PCM
- [ ] USB audio
- [ ] graphics architecture
- [ ] framebuffer userspace API
- [ ] compositor/display-server design
- [ ] Navi 23 modesetting research
- [ ] native modesetting
- [ ] acceleration
- [ ] GUI toolkit

## M15 — Packages and development
- [ ] Package format/database/dependencies
- [ ] package manager
- [ ] signed repositories
- [ ] ports/build recipes
- [ ] editor/developer tooling
- [ ] compiler bootstrap plan

## M16 — Self-hosting
- [ ] Compile a Rust userspace program on Vibrix
- [ ] Toolchain usable on Vibrix
- [ ] Build userspace on Vibrix
- [ ] Build kernel on Vibrix
- [ ] Produce a bootable Vibrix image from Vibrix

## Future
- [ ] aarch64 design
- [ ] aarch64 UEFI boot
- [ ] architecture-independent driver boundaries

## Early non-goals

Do not prioritize desktop polish, browsers, GPU acceleration, broad hardware support, POSIX completeness or Linux binary compatibility before the kernel foundation is reliable.
