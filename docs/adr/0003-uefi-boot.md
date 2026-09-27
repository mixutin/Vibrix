# ADR-0003: UEFI as the boot firmware interface

- **Status:** Accepted
- **Date:** 2026-09-27
- **Deciders:** Project bootstrap

## Context

The boot firmware interface defines how the OS loader starts and what services it can use before exiting boot services. Options include legacy BIOS, UEFI, and coreboot.

## Decision

Vibrix boots via UEFI. The bootloader is a UEFI application (`x86_64-unknown-uefi` target) that uses UEFI boot services to load the kernel ELF, discover hardware, capture the memory map, and call `ExitBootServices`.

## Consequences

- **Modern hardware:** UEFI is the current standard on x86-64 systems. Legacy BIOS is deprecated.
- **Boot services:** UEFI provides filesystem access, memory allocation, GOP framebuffer, ACPI discovery, and a rich pre-OS environment.
- **Complexity:** UEFI's protocol-based architecture requires defining protocol structures in Rust. This is manageable and well-specified.
- **ExitBootServices boundary:** After `ExitBootServices`, the kernel must manage all hardware itself. The handoff point is well-defined by the BootInfo ABI.
- **Secure Boot:** UEFI Secure Boot is not a current target but may be considered in the future.

## Alternatives considered

- **Legacy BIOS:** Simpler boot protocol but limited to 16-bit real mode, no filesystem access, no GOP, and deprecated on modern hardware. Rejected.
- **coreboot:** Open-source firmware with a clean boot flow. Rejected because it is not pre-installed on most target hardware and would require flashing firmware on Target 001.
- **Custom bootloader (no firmware):** Writing a bootloader that runs directly on hardware without UEFI. Rejected as a massive undertaking that duplicates what UEFI already provides.

## References

- UEFI Specification: https://uefi.org/specifications
- OVMF (Open Virtual Machine Firmware): https://github.com/tianocore/edk2
- BootInfo ABI: `docs/BOOT_ABI.md`
