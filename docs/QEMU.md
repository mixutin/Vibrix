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

## GUI splash versus kernel progress

The interactive QEMU window can keep the **TianoCore splash** and the
pre-exit UEFI console text at `VIBRIX: transition mappings verified`.
That screen is **not** a reliable kernel progress indicator: after
`ExitBootServices`, the loader stops calling firmware text output and
the standalone kernel writes to two native logging channels instead.
It now overlays a **small, readable "VIBRIX / KERNEL LIVE" banner**
on a sufficiently large RGB/BGR framebuffer after reaching the independent
kernel. This is a bounded pixel/glyph status indicator—not a general
text console, window manager or interactive shell. Small/bitmask GOP modes
keep a minimal safe black/green marker. The banner is separate from native
COM1/debugcon output, which remains the authoritative boot trace.

`./tools/run-qemu.sh` now captures both channels *without hiding the
graphical QEMU window*:

- `build/qemu/interactive-debugcon.log`: loader and kernel QEMU debug
  I/O port 0xE9, including `VIBRIX: ExitBootServices succeeded` and
  `VIBRIX: kernel entry after ExitBootServices`.
- `build/qemu/interactive-serial.log`: the kernel's independent native
  COM1 messages, including `Vibrix kernel started.` and PCI device
  discovery when the kernel actually reaches those stages.

From another terminal, while interactive QEMU runs:

```bash
tail -f build/qemu/interactive-debugcon.log build/qemu/interactive-serial.log
```

The `tools/test-qemu.sh` **headless** test separately uses
`build/qemu/debugcon.log` and `build/qemu/serial.log`. Its filenames
are intentionally distinct so tests do not overwrite GUI-run evidence.
QEMU is still not a persistent USB-root boot; the FAT directory is
development media.

To expose a virtual xHCI controller for native PCI-discovery diagnostics:

```bash
VIBRIX_QEMU_XHCI=1 ./tools/run-qemu.sh
```

This adds a controller in PCI, **not** an initialized xHCI stack or a
booted USB storage device.

Run the headless CI-equivalent smoke test with:

```bash
bash tools/test-qemu.sh
```

The smoke test now verifies **real kernel execution after firmware exit**. Its
QEMU debugcon log requires `ExitBootServices succeeded`, the standalone
kernel entry and BootInfo v3 validation, GDT/TSS initialization, COM1 setup,
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

## Native keyboard input prototype (M4.5, QEMU verified)

The standalone kernel includes a **read-only, polled i8042/PS/2 set-one
keyboard input prototype**. After firmware services terminate it polls legacy
x86 I/O status/data ports 0x64/0x60 with IRQs disabled and translates only a
small unshifted ASCII make-code subset; releases, extended and unsupported
codes are ignored. This is not a USB HID driver, hardware IRQ routing,
interactive command loop, VFS, TTY or Ring 3 userspace. Physical Target 001
may have no PS/2 keyboard and is not covered by this prototype.

The opt-in QEMU smoke variant exercises **actual QEMU keyboard injection** via
its host monitor after the kernel's own `VIBRIX: kernel PS2 polling ready`
marker, rather than fabricating kernel log text. It sends `h` and Return,
and requires the kernel's native COM1 output `kernel PS2 ascii 104` and
`kernel PS2 ascii 10`. Run with:

```sh
VIBRIX_QEMU_KEYBOARD_PROBE=1 bash tools/test-qemu.sh
```

[Actions run 36347623002](https://github.com/mixutin/Vibrix/actions/runs/36347623002)
verified the actual kernel's two distinct COM1 lines
`kernel PS2 ascii 104` and `kernel PS2 ascii 10`, plus the independent
debugcon readiness and accepted-character markers. The first smoke
assertion had a false-positive ASCII prefix bug; the final test uses
CRLF-normalized **exact-line matches** and observed Return separately.
Normal, virtual xHCI and all exception-probe boot configurations also passed
the kernel/QEMU job. There is still no IRQ route, USB HID, interactive
console input editing or physical PS/2 test.

## Bounded native development console (M4.5 candidate)

The post-firmware single-CPU kernel now offers a serial COM1 `vibrix> `
prompt once the i8042 polling loop is ready. Its fixed 80-byte ASCII input
buffer implements backspace/erase, printable-character echo, Return
submission, bounded overflow rejection, and reset between lines.
The first strict command table supports `help` and `info`, and reports
unknown exact commands without shell expansions or untrusted memory access.
The console never uses firmware text output and has no allocator, syscall,
scheduler, native USB HID, filesystem or privilege boundary.

QEMU's opt-in `VIBRIX_QEMU_CONSOLE_PROBE=1 VIBRIX_SKIP_BUILD=1
bash tools/test-qemu.sh` waits for a **kernel-origin** prompt marker,
then uses the host monitor to inject the real key sequence `helx`,
Backspace, `p`, Return. It checks kernel-only acceptance of the
backspace and `help` dispatch and the actual COM1 command listing.
This validates the text path only; the visible framebuffer remains an
independent boot banner, not a graphical interactive terminal. Target 001
USB HID, APIC interrupt delivery, full TTY and the userspace shell are
separate roadmap items. CI evidence is pending on this branch.
