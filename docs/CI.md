# Continuous Integration

Vibrix CI builds the Rust UEFI loader and bare-metal kernel, checks formatting and lints, and boots the resulting EFI tree in headless QEMU.

The CI job also compiles `boot/src/elf.rs` as a standalone host test harness and runs its parser regression tests. This uses only the official Rust toolchain and exercises malformed ELF metadata without requiring UEFI firmware. Host parser tests do not demonstrate kernel handoff.

The parser also rejects a kernel entry point that does not belong to a file-backed, executable `PT_LOAD` range. Host regression fixtures cover non-executable code, BSS-only entry points and out-of-range entry points. This prevents an invalid future kernel jump but does not yet implement the firmware-to-kernel handoff.

The QEMU smoke test captures Vibrix's QEMU-only debug port and requires the bootloader to prove that it:

1. entered the Vibrix loader,
2. opened `/vibrix/kernel.elf`,
3. validated ELF64 little-endian x86-64 metadata,
4. parsed at least one valid `PT_LOAD` segment,
5. accepted the kernel image,
6. discovered an ACPI RSDP with valid firmware-provided checksum(s),
7. located a linear UEFI GOP framebuffer with sane mode and size metadata.

Run the same smoke test locally:

```bash
sudo apt install -y qemu-system-x86 ovmf
bash tools/test-qemu.sh
```

The debug port is compiled only for QEMU builds. Bare-metal Vibrix builds do not write to the QEMU debug I/O port.
