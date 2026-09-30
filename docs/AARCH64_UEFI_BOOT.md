# AArch64 UEFI first boot

The first AArch64 execution target is QEMU `virt` with AAVMF/EDK2 firmware.
The standalone dependency-free `platform/aarch64-uefi` application is compiled
for Rust's `aarch64-unknown-uefi` target and installed as the removable-media
fallback path `EFI/BOOT/BOOTAA64.EFI`.

The application deliberately proves only the architecture/firmware entry
boundary. Firmware calls Rust `efi_main`, the application validates the UEFI
system-table/console pointers, and emits:

`VIBRIX: AArch64 UEFI boot entry verified`

through the firmware console before returning success.

This checks the bounded Future **AArch64 UEFI boot** item once exact-head QEMU
evidence is green. It does not load the Vibrix kernel, exit boot services,
configure EL1 translation, GICv3, the generic timer, PSCI, Device Tree, storage,
or userspace. Those remain later AArch64 implementation milestones described by
ADR 0025.
