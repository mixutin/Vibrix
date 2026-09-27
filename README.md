# Vibrix

**An independent Rust-based Unix-like operating system built from scratch through AI-assisted development.**

> How far can vibe coding go?

Vibrix is an experiment in building a complete operating system from first principles. It is **not a Linux distribution**, does not use Linux or BSD kernels, and is designed as a **Rust-native operating system from boot to userspace**.

## Rust all the way down

Rust is Vibrix's implementation language across the operating system:

- UEFI loader
- kernel
- memory management and scheduler
- drivers
- filesystem and storage stack
- networking stack
- system libraries and syscall wrappers
- init and service management
- shell and core utilities
- installer
- package tooling
- first-party applications

Tiny architecture-specific assembly is permitted only where hardware interfaces make it unavoidable, such as early CPU entry/context transitions. It must remain isolated behind Rust interfaces.

Vibrix does **not** intend to ship a traditional C libc as its native system interface. Its native userspace API and standard system libraries will be Rust-first. C/POSIX compatibility, if implemented later, will be a compatibility layer rather than the foundation of the OS.

## Principles

- Rust-native kernel and userspace
- Independent implementation from boot to applications
- No third-party runtime code shipped as part of Vibrix
- Specifications, hardware manuals and development tools are allowed
- x86-64 and UEFI first
- QEMU first, bare metal as the hardware layer matures
- Persistent USB installation is a first-class target
- Separate HDD/SSD installation path
- Unix/POSIX-inspired semantics where useful
- Memory safety by default; `unsafe` is isolated, justified and reviewed
- Document the role of AI throughout development

## Deployment targets

### Vibrix Portable
A full persistent installation whose system disk is a USB drive. Programs, configuration, accounts and user data survive reboots and movement between machines.

### Vibrix Installed
A permanent installation to an internal HDD, SATA SSD or NVMe device.

Both editions are the same operating system. Only installation and storage policy differ.

## Current status

**Phase 0 — Bootstrap.** Vibrix does not boot yet. Rust has been selected as the system implementation language. The next target is the first x86-64 UEFI boot path.

See [ROADMAP.md](ROADMAP.md), [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), and [docs/adr/](docs/adr/) for architecture decision records.

## Planned source tree

```
boot/       Rust Vibrix UEFI loader
kernel/     Rust Vibrix kernel
sys/        native Rust userspace/system interfaces
user/       Rust init, shell and core utilities
drivers/    Rust device drivers
fs/         Rust filesystem + tooling
installer/  Rust portable + fixed-disk installer
tools/      development utilities
docs/       architecture and specifications
```

## Independence rule

Vibrix may be developed using existing compilers, assemblers, emulators, debuggers, firmware and source-control tools. Those tools are not part of Vibrix.

Code from Linux, BSD, GNU, third-party bootloaders, third-party libc implementations, BusyBox or other operating systems must not be copied into or shipped with Vibrix.

## Mascot

Vibrix's mascot is a curious black-and-white fox carrying the Vibrix **V**. A name will be chosen separately.

## License

Licensing is intentionally undecided during bootstrap. Do not assume code is open-source licensed until a license is explicitly adopted.
