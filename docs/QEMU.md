# QEMU development target

QEMU is Vibrix's first development platform. Target 001 remains the first physical reference machine. QEMU results are not physical-hardware qualification.

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

Install Rust with [rustup](https://rustup.rs/) and restart your shell. The pinned toolchain and required Rust targets are managed by `tools/build-qemu.sh`. A successful Rust build does not install QEMU or OVMF. The launcher checks those dependencies before building.

## First boot: real userspace in the QEMU window

```bash
./tools/run-qemu.sh
```

The interactive launcher builds with `userspace-shell` by default, enters the existing compiled Rust shell at Ring 3, and presents `/dev/tty` output on the guest framebuffer:

```text
Vibrix shell
vibrix$ 
```

Click the guest window to type. Start with `help`, `echo hello`, `pwd`, `ls /` or `cat /welcome`. The prompt and results are produced by userspace syscalls, not by a kernel command parser. Accepted keyboard input is echoed live, Backspace erases input, and output scrolls. The existing native PS/2 decoder remains a limited unshifted ASCII subset; this is not full keyboard-layout/modifier support.

The kernel-owned renderer uses retained GOP pixels after `ExitBootServices`, not firmware text output. Small or unsupported bitmask GOP modes retain a safe marker and serial fallback. See [userspace display](USERSPACE_DISPLAY.md) for ownership, limits, tests and exact profile selection.

The bootstrap filesystem is RAM-only. This does not provide persistent USB root, native USB keyboard-to-TTY integration, a graphical desktop or a multi-process service manager. `exit` retains the early PID 1 exit behavior rather than automatically respawning the shell.

## VNC: the same guest display and keyboard

```bash
./tools/run-qemu.sh --vnc
```

Connect a VNC viewer to `127.0.0.1:5900` (display `:0`). To use port 5901:

```bash
./tools/run-qemu.sh --vnc=1
```

Only display numbers 0 through 99 are accepted. The listener binds to localhost, never all interfaces. VNC itself is unencrypted and unauthenticated; do not expose it publicly. For a remote QEMU host, create an authenticated SSH tunnel and connect the viewer locally:

```bash
ssh -N -L 5900:127.0.0.1:5900 user@qemu-host
```

This is a native VNC endpoint, not a bundled browser/noVNC server. Local users may still reach a localhost listener.

## Firmware selection

`tools/run-qemu.sh` and `tools/test-qemu.sh` share `tools/ovmf.inc`. It searches common Ubuntu/Debian, Fedora, Arch and EDK2 paths and requires a matching OVMF CODE/VARS pair rather than mixing 2 MiB and 4 MiB images. The VARS template is copied to a per-run writable file; the system-installed firmware is not modified.

For a nonstandard installation:

```bash
OVMF_CODE=/path/to/OVMF_CODE.fd \
OVMF_VARS=/path/to/OVMF_VARS.fd \
./tools/run-qemu.sh
```

`OVMF_SEARCH_DIRS` can extend the searched locations with colon-separated paths. The missing-firmware diagnostic lists supported settings. Launch only one VM at a time per checkout because the build directory and variable copy are shared development state.

## Serial/debug logging and diagnostic boot

Both interactive modes retain independent native logs:

- `build/qemu/interactive-debugcon.log`: loader/kernel port 0xE9 evidence, including firmware exit, kernel entry and userspace setup.
- `build/qemu/interactive-serial.log`: native COM1 diagnostics and mirrored TTY output/input echo.

```bash
tail -f build/qemu/interactive-debugcon.log build/qemu/interactive-serial.log
```

A TianoCore splash is not proof of a running userspace shell. A visible shell plus the independent logs distinguishes successful post-firmware execution from a stalled boot. In the interactive profile, `VIBRIX: framebuffer terminal initialized` marks the renderer setup; the shell's later prompt and command responses prove its actual use.

To boot the older `vibrix>` kernel development console instead:

```bash
./tools/run-qemu.sh --kernel-console
```

That console remains COM1-oriented, with its historical boot banner. Its diagnostic commands include `help`, `clear`, `info`, `mem`, `pci`, `acpi`, `uptime` and `reboot`. An explicit `VIBRIX_KERNEL_FEATURES` value also selects a custom profile; unset it to restore the launcher's automatic userspace default.

To expose a virtual xHCI controller for diagnostic discovery:

```bash
VIBRIX_QEMU_XHCI=1 ./tools/run-qemu.sh --kernel-console
```

Adding that virtual PCI device alone is not native USB storage or keyboard integration. Existing dedicated USB proof profiles remain separate.

## Automated verification

The new graphical/VNC integration test uses the actual interactive launcher, a real localhost RFB keyboard client, and captured guest pixels:

```bash
python3 tools/test_userspace_display.py --unit
python3 tools/test_userspace_display.py
```

It verifies automatic `vibrix$` display, keyboard echo and Backspace before Enter, then the actual userspace `echo vnc` result and next prompt. The `Userspace display` workflow uploads real `.ppm` screenshots and serial/debug logs. Test definitions are not passing evidence; consult the exact-head Actions result.

The original headless diagnostic smoke test is unchanged:

```bash
bash tools/test-qemu.sh
```

Direct `tools/build-qemu.sh` and this diagnostic test retain the `qemu-debugcon` default, preserving existing fault/IRQ/console probe semantics. Their logs are `build/qemu/debugcon.log` and `build/qemu/serial.log`, separate from interactive log names. The FAT directory is development media, not a native persistent USB root.

The opt-in panic proof remains:

```bash
VIBRIX_KERNEL_FEATURES=qemu-debugcon,panic-probe bash tools/build-qemu.sh
VIBRIX_SKIP_BUILD=1 bash tools/test-qemu.sh
grep -F 'VIBRIX: kernel panic' build/qemu/debugcon.log
grep -F 'kernel panic:' build/qemu/serial.log
```

Historical kernel-console and keyboard checks remain available with `VIBRIX_QEMU_CONSOLE_PROBE=1` and `VIBRIX_QEMU_KEYBOARD_PROBE=1` on diagnostic builds. They inject real monitor keys and verify native COM1/debugcon behavior, but by themselves do not prove a userspace framebuffer terminal. The console reboot test requires actual QEMU termination under `-no-reboot`; a logged reboot request is insufficient. See [the roadmap](../ROADMAP.md) for milestone evidence and [CI](CI.md) for the wider matrix.

## Independence boundary

OVMF is development firmware supplied to the VM, not part of Vibrix. QEMU, the Python RFB/QMP test client and host build utilities are development infrastructure, not the Vibrix runtime. See [AGENTS.md](../AGENTS.md), [INDEPENDENCE.md](INDEPENDENCE.md) and the [display design](USERSPACE_DISPLAY.md).
