# Vibrix

**An independent Unix-like operating system built from scratch through AI-assisted development.**

> How far can vibe coding go?

Vibrix is an experiment in building a complete operating system from first principles. It is **not a Linux distribution** and does not use the Linux or BSD kernels.

## Principles

- Independent kernel and userspace
- No third-party runtime code shipped as part of Vibrix
- Specifications, hardware manuals and development tools are allowed
- x86-64 and UEFI first
- QEMU first, bare metal as the hardware layer matures
- Persistent USB installation is a first-class target
- Separate HDD/SSD installation path
- Unix/POSIX-inspired interfaces where useful
- Document the role of AI throughout development

## Deployment targets

### Vibrix Portable
A full persistent installation whose system disk is a USB drive. Programs, configuration, accounts and user data survive reboots and movement between machines.

### Vibrix Installed
A permanent installation to an internal HDD, SATA SSD or NVMe device.

Both editions are the same operating system. Only installation and storage policy differ.

## Current status

**Phase 0 — Bootstrap.** Vibrix does not boot yet. The repository is establishing architecture, project rules and the first x86-64 UEFI kernel target.

See [ROADMAP.md](ROADMAP.md) and [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Planned source tree

```
boot/       Vibrix UEFI loader
kernel/     Vibrix kernel
libc/       Vibrix C library
user/       init, shell and core utilities
drivers/    device drivers
fs/         filesystem tooling
installer/  portable + fixed-disk installation
tools/      host-side development utilities
docs/       architecture and specifications
```

## Independence rule

Vibrix may be developed using existing compilers, assemblers, emulators, debuggers, firmware and source-control tools. Those tools are not part of Vibrix.

Code from Linux, BSD, GNU, third-party bootloaders, third-party libc implementations, BusyBox or other operating systems must not be copied into or shipped with Vibrix.

## Mascot

Vibrix's mascot is a curious black-and-white fox carrying the Vibrix **V**. A name will be chosen separately.

## License

Licensing is intentionally undecided during bootstrap. Do not assume code is open-source licensed until a license is explicitly adopted.
