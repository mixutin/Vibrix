# QEMU development target

QEMU is Vibrix's first development platform. Target 001 remains the first physical reference machine.

## Host prerequisites (Ubuntu)

```bash
sudo apt update
sudo apt install -y qemu-system-x86 ovmf
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Restart the shell after installing Rust.

## First boot

```bash
chmod +x tools/build-qemu.sh tools/run-qemu.sh
./tools/run-qemu.sh
```

The current milestone boots a Vibrix-authored Rust UEFI executable and writes directly through the UEFI Simple Text Output Protocol.

Expected output includes:

```text
Vibrix bootloader v0.0.1
Rust-native. Independent.
Hello from UEFI.
```

This is deliberately not yet the Vibrix kernel. The next boot milestone is to load a separate kernel image, obtain the firmware memory map, construct a boot-info structure, call ExitBootServices and transfer control to the kernel.

## Independence boundary

OVMF is development firmware supplied to the virtual machine. It is not part of Vibrix. QEMU is also a development/testing tool and is not shipped as part of Vibrix.

The boot executable itself uses no third-party Rust crates.
