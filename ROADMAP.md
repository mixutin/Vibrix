# Vibrix Roadmap

## Phase 0 — Bootstrap
- [x] Establish project identity
- [x] Define independence rule
- [x] Define portable USB and installed editions
- [x] Establish repository structure
- [ ] Select implementation language/toolchain
- [ ] Define x86-64 ABI conventions
- [ ] Reproducible host build environment

## Phase 1 — First boot
- [ ] Build our own x86-64 UEFI loader
- [ ] Load Vibrix kernel
- [ ] Exit UEFI boot services
- [ ] Kernel entry point
- [ ] Serial console
- [ ] Framebuffer console
- [ ] Panic path
- [ ] QEMU launch/debug scripts

## Phase 2 — Kernel foundations
- [ ] GDT and IDT
- [ ] CPU exceptions
- [ ] Physical page allocator
- [ ] Virtual memory manager
- [ ] Kernel heap
- [ ] APIC/timer support
- [ ] Interrupt subsystem

## Phase 3 — Processes
- [ ] Kernel threads
- [ ] Scheduler
- [ ] Ring 3 userspace
- [ ] System-call ABI
- [ ] Process model
- [ ] ELF loader

## Phase 4 — Storage
- [ ] Block-device abstraction
- [ ] PCI/PCIe discovery
- [ ] Initial storage driver
- [ ] Vibrix filesystem design
- [ ] VFS
- [ ] File descriptors
- [ ] Persistent root filesystem

## Phase 5 — Unix userspace
- [ ] libc foundation
- [ ] init
- [ ] TTY
- [ ] shell
- [ ] pipes
- [ ] signals
- [ ] permissions/users
- [ ] core utilities

## Phase 6 — Vibrix Portable
- [ ] USB mass-storage support
- [ ] Portable hardware rediscovery
- [ ] Persistent USB root
- [ ] RAM-backed temporary storage
- [ ] Flash-write reduction
- [ ] Portable installer/imager

## Phase 7 — Bare-metal installation
- [ ] GPT tooling
- [ ] EFI System Partition tooling
- [ ] Vibrix filesystem formatter
- [ ] Safe disk selection
- [ ] HDD/SATA SSD installation
- [ ] NVMe installation
- [ ] Recovery path

## Later
Networking, SMP, USB/HID expansion, audio, graphics, GUI, package management, ARM64 and eventual self-hosting.
