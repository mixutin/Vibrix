# Vibrix

**An independent, Rust-native Unix-like operating system that lives on a USB drive.**

> How far can vibe coding go?

Vibrix is an experiment in building a complete operating system from first principles through **AI-only engineering**. It is **not a Linux distribution**, does not use the Linux or BSD kernels, and is designed as a Rust-native system from bootloader to userspace.

## AI-only engineering

Vibrix is developed by AI coding agents.

- implementation is authored by AI agents
- technical documentation is authored by AI agents
- pull requests are opened and discussed by AI agents
- code review is performed by AI agents
- test evidence is collected and reported by AI agents
- architecture proposals and ADRs are authored/reviewed by AI agents

The project owner may provide goals, constraints, priorities and authorization for repository actions, but Vibrix does not use a human-review requirement or a human-authored implementation workflow.

## USB-only by design

Vibrix is not a disposable live environment and it is not an installer for an internal disk.

The USB device is the computer's **persistent Vibrix system disk**:

- the OS lives on USB storage
- the root filesystem lives on USB storage
- installed applications stay on the USB
- user accounts and configuration stay on the USB
- user home data stays on the USB
- updates modify the USB installation
- the same Vibrix drive is intended to move between compatible machines

Internal NVMe/SATA drives may eventually be supported as optional data devices, but they are **not installation targets for Vibrix**.

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
- USB provisioning/image tooling
- package tooling
- first-party applications

Tiny architecture-specific assembly is permitted only where hardware interfaces make it unavoidable. It must remain isolated behind documented Rust interfaces.

Vibrix does **not** intend to ship a traditional C libc as its native system interface. Its native userspace API and standard system libraries will be Rust-first. C/POSIX compatibility, if implemented later, will be a compatibility layer rather than the foundation of the OS.

## Principles

- Rust-native kernel and userspace
- Independent implementation from boot to applications
- No community or third-party Rust crates in Vibrix; only official Rust language/toolchain components
- Specifications, hardware manuals and development tools are allowed
- x86-64 and UEFI first
- QEMU first, then removable-media bare-metal boot
- USB storage is the only supported Vibrix system/root device
- persistent state is a core requirement
- hardware must be rediscovered on every boot
- minimize unnecessary flash writes
- Unix/POSIX-inspired semantics where useful
- memory safety by default; unsafe code is isolated, justified and reviewed
- document the role of AI throughout development

## Current status

Vibrix has completed its first QEMU UEFI boot milestone. A separate Rust kernel artifact now exists, and work is underway on firmware-to-kernel handoff and kernel ELF loading.

See ROADMAP.md, docs/ARCHITECTURE.md, and docs/USB_MODEL.md.

## Planned source tree

    boot/       Rust Vibrix UEFI loader
    kernel/     Rust Vibrix kernel
    sys/        native Rust userspace/system interfaces
    user/       Rust init, shell and core utilities
    drivers/    Rust device drivers
    fs/         Rust filesystem + tooling
    image/      Rust USB image/provisioning tooling
    tools/      development utilities
    docs/       architecture and specifications

## Independence rule

Vibrix may be developed using existing compilers, assemblers, emulators, debuggers, firmware and source-control tools. Those tools are not part of Vibrix.

Code from Linux, BSD, GNU, third-party bootloaders, third-party libc implementations, BusyBox or other operating systems must not be copied into or shipped with Vibrix.

Vibrix also does **not** use crates.io/community packages in the bootloader, kernel, drivers, system libraries or first-party userspace. The only Rust runtime/foundation code allowed is code provided as part of the official Rust toolchain itself, such as core and Rust compiler support.

## Mascot

Vibrix's mascot is a curious black-and-white fox carrying the Vibrix **V**. A name will be chosen separately.

## License

Licensing is intentionally undecided during bootstrap. Do not assume code is open-source licensed until a license is explicitly adopted.
