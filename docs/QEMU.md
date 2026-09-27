# QEMU development target

QEMU is Vibrix's first development platform. Target 001 remains the first physical reference machine.

## Host prerequisites

Install QEMU and OVMF on the **host**, using the distribution's package manager:

```bash
# Ubuntu / Debian
sudo apt update
sudo apt install -y qemu-system-x86 ovmf

# Fedora
sudo dnf install qemu-system-x86 edk2-ovmf

# Arch Linux
sudo pacman -S qemu-system-x86 edk2-ovmf
```

Install Rust with [rustup](https://rustup.rs/) and restart your shell. The
pinned toolchain and required Rust targets are managed by
`tools/build-qemu.sh`.

A successful Rust build does not install QEMU or OVMF. The interactive
launcher and headless smoke test check those host dependencies before
starting the build.

## First boot

```bash
./tools/run-qemu.sh
```

`tools/run-qemu.sh` and `tools/test-qemu.sh` share the firmware selection
logic in `tools/ovmf.inc`. It searches common Ubuntu/Debian, Fedora, Arch
and EDK2 installation paths, then requires a matching OVMF CODE/VARS pair
rather than mixing 2 MiB and 4 MiB images. The VARS template is copied
to a per-run writable file: the system-installed firmware is not modified.

If OVMF is installed somewhere else, supply explicit paths:

```bash
OVMF_CODE=/path/to/OVMF_CODE.fd \
OVMF_VARS=/path/to/OVMF_VARS.fd \
./tools/run-qemu.sh
```

You can also extend the searched firmware locations with a colon-separated
`OVMF_SEARCH_DIRS` value. The missing-firmware diagnostic names the
distribution packages and supported environment variables.

Run the headless CI-equivalent smoke test with:

```bash
bash tools/test-qemu.sh
```

The smoke test now verifies **real kernel execution after firmware exit**. Its
QEMU debugcon log requires `ExitBootServices succeeded`, the standalone
kernel entry and BootInfo v2 validation, GDT/TSS initialization, COM1 setup,
and pixel writes to the uncached GOP framebuffer. The distinct QEMU serial
file must contain `Vibrix kernel started.`. This is not a native USB,
filesystem, userspace or physical Target 001 test.

The optional kernel panic-probe build tests the real post-firmware panic
handler independently of the regular spin-loop boot:

```bash
VIBRIX_KERNEL_FEATURES=qemu-debugcon,panic-probe bash tools/build-qemu.sh
VIBRIX_SKIP_BUILD=1 bash tools/test-qemu.sh
grep -F 'VIBRIX: kernel panic' build/qemu/debugcon.log
grep -F 'kernel panic:' build/qemu/serial.log
```

The panic-probe feature is deliberately opt-in; the regular QEMU build and
bare-metal kernel do not deliberately panic.

## Independence boundary

OVMF is development firmware supplied to the virtual machine, not part of
Vibrix. QEMU is development/testing infrastructure, not shipped as the
Vibrix runtime. Rust package policy is documented in [AGENTS.md](../AGENTS.md) and
[INDEPENDENCE.md](INDEPENDENCE.md).
