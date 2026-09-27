# Continuous Integration

Vibrix CI builds the Rust UEFI loader and bare-metal kernel, checks formatting and lints, and boots the resulting EFI tree in headless QEMU.

The QEMU smoke test captures Vibrix's QEMU-only debug port and requires the bootloader to prove that it:

1. entered the Vibrix loader,
2. opened `/vibrix/kernel.elf`,
3. validated ELF64 little-endian x86-64 metadata,
4. parsed at least one valid `PT_LOAD` segment,
5. accepted the kernel image.

Run the same smoke test locally:

```bash
sudo apt install -y qemu-system-x86 ovmf
bash tools/test-qemu.sh
```

The debug port is compiled only for QEMU builds. Bare-metal Vibrix builds do not write to the QEMU debug I/O port.
