# Vibrix Roadmap

## Phase 0 — Bootstrap
- [x] Establish project identity
- [x] Define independence rule
- [x] Define portable USB and installed editions
- [x] Establish repository structure
- [x] Select implementation language: Rust
- [x] Define Rust-first system policy
- [ ] Pin Rust toolchain and bare-metal targets
- [ ] Define x86-64 ABI and syscall conventions
- [ ] Reproducible host build environment
- [ ] Establish `unsafe` code policy

## Phase 1 — First boot
- [ ] Build our own Rust x86-64 UEFI loader
- [ ] Load Vibrix kernel
- [ ] Exit UEFI boot services
- [ ] Rust kernel entry point
- [ ] Serial console
- [ ] Framebuffer console
- [ ] Panic path
- [ ] QEMU launch/debug scripts

## Phase 2 — Kernel foundations
- [ ] GDT and IDT
- [ ] CPU exceptions
- [ ] Physical page allocator
- [ ] Virtual memory manager
- [ ] Rust kernel allocator
- [ ] APIC/timer support
- [ ] Interrupt subsystem

## Phase 3 — Processes
- [ ] Kernel threads
- [ ] Scheduler
- [ ] Ring 3 Rust userspace
- [ ] Vibrix syscall ABI
- [ ] Native Rust syscall library
- [ ] Process model
- [ ] Executable loader

## Phase 4 — Storage
- [ ] Block-device abstraction
- [ ] PCI/PCIe discovery
- [ ] Initial Rust storage driver
- [ ] Vibrix filesystem design
- [ ] VFS
- [ ] File descriptors
- [ ] Persistent root filesystem

## Phase 5 — Rust-native Unix userspace
- [ ] Native Rust system library
- [ ] Rust init
- [ ] TTY
- [ ] Rust shell
- [ ] pipes
- [ ] signals
- [ ] permissions/users
- [ ] Rust core utilities

## Phase 6 — Vibrix Portable
- [ ] USB mass-storage support
- [ ] Portable hardware rediscovery
- [ ] Persistent USB root
- [ ] RAM-backed temporary storage
- [ ] Flash-write reduction
- [ ] Rust portable installer/imager

## Phase 7 — Bare-metal installation
- [ ] GPT tooling
- [ ] EFI System Partition tooling
- [ ] Vibrix filesystem formatter
- [ ] Safe disk selection
- [ ] HDD/SATA SSD installation
- [ ] NVMe installation
- [ ] Recovery path

## Later
Rust-native networking, SMP, USB/HID expansion, audio, graphics, GUI, package management, ARM64 and eventual self-hosting.
