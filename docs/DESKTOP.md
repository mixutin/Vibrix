# Native desktop preview

Implemented by **GPT-6 Astra Pro**, 29 September 2026. This is a compiled Rust userspace ELF on the Vibrix kernel, not a host GUI, Linux guest, HTML mockup or renamed kernel console. It is an opt-in, single-process desktop session; it does not complete a general desktop OS or the persistent USB product goal. **Chromium is not ported or running.** See [the browser port plan](CHROMIUM_PORT.md).

## Boot and controls

```bash
./tools/run-qemu.sh --desktop
# The same guest screen, bound only to localhost:
./tools/run-qemu.sh --desktop --vnc=1
# Existing frontends remain available:
./tools/run-qemu.sh
./tools/run-qemu.sh --kernel-console
```

Use a VNC viewer at `127.0.0.1:5901` for the second command. VNC has no authentication or encryption; do not expose its port publicly. Use an authenticated SSH tunnel for remote viewing. Follow [QEMU prerequisites](QEMU.md); do not run launchers concurrently in one checkout.

F1 opens Terminal, F2 Files, F3 System and F4 the explicitly labelled Chromium **port-status panel**, not a browser. F11 maximizes/restores. Escape hides/restores the foreground window. Click task buttons to switch views, drag the title bar to move, and use its `_`, `+`, `x` controls to hide, maximize or hide the session. Hiding preserves application state rather than terminating a process.

Terminal runs the existing [vibrix-sh command engine](USERSPACE_CLI.md) with actual native VFS/process syscalls. For example, `echo native > /note` followed by `cat /note` creates and reads a real bootstrap file; the Files view independently displays that same file. Backspace edits the current line; Ctrl-U cancels it. `exit` restarts the shell session. Unredirected streaming reads from fd 0 are unsupported in this frontend; use file arguments or input redirection. There is no PTY or independent shell process yet.

Files is a read-only viewer: click an entry, or select with Up/Down and Enter. Backspace or `[..] Parent` navigates upward. It lists at most 16 entries and previews the first 1024 bytes of regular files. Device nodes are not opened. F2 refreshes. The terminal can modify files; the viewer does not claim atomic writes, a persistent home directory or an installed application manager.

## Runtime boundary

`userspace/desktop` is `no_std`, has no allocator, and reuses `vibrix-shell`, `vibrix-syscall` and the original first-party font. The kernel loads its fixed-layout ELF into a private user CR3 and enters CPL3. Only this opt-in profile expands the bounded address-space pool from 32 to 128 frames and its guarded user stack to 16 pages (64 KiB); the ordinary shell keeps four stack pages and all other probes keep their existing policy. Guard pages and user W^X remain enforced.

The software scene has one foreground window and four views in one process. It draws through checked fill/tile syscalls. Text uses bounded 12x16 ASCII cells and tiles of at most 960 pixels. The terminal's logical rows/columns stay at their initial normal-window size when maximized. Mouse movement repaints the scene; this is not an accelerated compositor, a multi-client window server or an ANSI/Unicode terminal emulator.

## Additive display/input ABI

Existing syscall numbers 0–14 and ABI version 1 are unchanged. New calls are available only in the desktop profile; other profiles return `NotSupported`. Registers retain the existing [syscall ABI](../shared/syscall_abi.rs). Arguments not used below must be zero.

| Number / call | Arguments | Result |
| --- | --- | --- |
| 15 / DisplayInfo | writable output address | 24 bytes: six little-endian u32 values: display ABI version, width, height, format, maximum blit pixels, capability mask |
| 16 / DisplayFill | x, y, width, height, logical color, zero | Fill a nonempty, fully in-bounds rectangle |
| 17 / DisplayBlit | x, y, width, height, pixel address, pixel count | Copy exactly width × height u32 pixels; at most 1024 |
| 18 / InputPoll | writable event address | 0 for no event, 1 for an event; output is always initialized on success |

Display ABI version and logical XRGB8888 format are both 1. Colors are `0x00RRGGBB`, independent of GOP byte order. Fill rejects a nonzero high byte; blit ignores reserved color bits. No physical address, kernel address, stride or MMIO mapping is exposed. Event layout is 16 bytes: kind u32, code u32, x i32, y i32. Kind 1 is a key make event; kind 2 is relative pointer motion/buttons (left=1, right=2, middle=4, positive y downward). ASCII keys and the named function/navigation constants are in [display_abi.rs](../shared/display_abi.rs). Capabilities distinguish keyboard from successfully initialized pointer support.

All user memory is validated through the existing checked copy layer. Blit copies into a bounded kernel buffer before switching to the retained framebuffer CR3; neither raw userspace pointers nor user-owned slices are used under that root. Rectangle, count, reserved-argument and conversion checks reject malformed requests before drawing. InputPoll validates its whole writable output before consuming an event. There is no concurrent unmap in this single-BSP bootstrap process model.

The sole display owner is selected at build time. The legacy framebuffer TTY is excluded from desktop builds, including the all-features lint configuration. A root guard restores the exact prior CR3 after framebuffer access. Access runs with IF clear, no AP, and no interrupt, panic or NMI renderer. Unsafe blocks document mapping lifetime and exclusive access. These assumptions are not a capability/security design for future concurrent, mutually untrusted applications: that design must precede a multi-process display server.

## Input and limitations

The QEMU development path polls the legacy i8042 controller. Initialization resets the mouse to standard three-byte mode and checks its acknowledgement, self-test and device ID before enabling stream reporting. Failure leaves a keyboard-only session. The decoder preserves Shift/Caps/Control and consumes Pause/extended sequences without inventing input. Signed nine-bit mouse deltas, overflow and incomplete packets are tested.

A 256-event FIFO captures input while software fill/blit operations run. The kernel pumps hardware between small drawing batches, so full repaints do not simply discard normal typing. A full FIFO stops consuming new hardware bytes rather than overwriting a queued click/key. Hardware buffers are finite too: burst loss and high-rate physical devices are not qualified. No IRQ keyboard/mouse, USB desktop input, wheel, hotplug or international layout support is claimed. USB mouse protocol work is separate, not integrated by this profile.

There is no idle sleep yet: the foreground loop polls and can consume a host CPU core under QEMU. There is no hardware GPU acceleration, vsync, clipboard, resize negotiation, audio, general application spawn or durable session storage. The bootstrap root and `/note` disappear at reboot. QEMU success is not Target 001 or any physical-computer qualification. No internal disk is written, provisioned or used as system root.

## Evidence and reproduction

```bash
cargo test --locked -p vibrix-desktop --lib
cargo test --locked -p vibrix-shell --lib
cargo test --locked -p vibrix-syscall
cargo clippy --locked -p vibrix-desktop --target x86_64-unknown-none -- -D warnings
cargo clippy --locked -p vibrix-kernel --all-features --target x86_64-unknown-none -- -D warnings
python3 tools/test_desktop_display.py --unit
python3 tools/test_desktop_display.py
python3 tools/test_userspace_display.py
./tools/test-qemu.sh
```

The [desktop workflow](../.github/workflows/userspace-desktop.yml) also directly tests framebuffer stride/canaries, RGB/BGR conversion, ABI records, PS/2 parsing and event FIFO wrap/full behavior. Its QEMU test boots the real launcher, verifies CPL3 and bad-pointer rejection, sends real RFB keys, creates/reads a VFS file, opens it with PS/2 mouse input, drags/maximizes/restores the window and checks exact guest glyph pixels. The independent oracle rejects blank/truncated images. Screendumps are produced by QEMU, not an image generator or host renderer.

The test deliberately sends a large pointer journey in bounded steps and acknowledges each actual cursor location. QEMU's finite standard PS/2 FIFO can otherwise fill before the guest runs, leaving a click halfway through an unrealistically large host-injected delta. A separate observed failure—keystrokes lost while repainting—was fixed in production with the event FIFO, not hidden by changing expected output. `build/qemu/desktop-result.json` records the source commit, dirty-worktree flag and explicit `chromium_running: false`. Use exact-head Actions results and retained logs/screenshots; the existence of the workflow is not itself a passing result.

## Research and dependencies, checked 2026-09-29

[UEFI 2.10, section 12.9 GOP](https://uefi.org/specs/UEFI/2.10/12_Protocols_Console_Support.html) defines the pixel-format/scanline distinction used here. The 2.11 page was unavailable during this review; no claim is made to have validated it. [Microsoft's PS/2 mouse packet table](https://learn.microsoft.com/en-us/windows-hardware/drivers/hid/keyboard-and-mouse-hid-client-drivers) documents the standard packet fields. [QEMU's own PS/2 implementation](https://github.com/qemu/qemu/blob/master/hw/input/ps2.c) was inspected for reset acknowledgements, standard device mode and FIFO behavior; no implementation code was copied. The local emulator used for bring-up was QEMU 10.2.1. [QMP input-send-event and screendump](https://www.qemu.org/docs/master/interop/qemu-qmp-ref.html) and the existing RFB helper provide host-side test transport only.

[embedded-graphics 0.8.2](https://docs.rs/embedded-graphics/0.8.2/embedded_graphics/) is a relevant no_std, allocator-free alternative. This bounded slice reuses Vibrix's existing font and needs only fills and small blits, so it does not add a graphics framework merely to reimplement those two operations. The canvas trait leaves room for a reviewed renderer later. This is a scope decision, not a blanket external-library prohibition or a claim that custom code is inherently safer.

The only new Cargo package is first-party `vibrix-desktop` 0.0.1, depending on exact local `vibrix-shell` and `vibrix-syscall` 0.0.1. No registry/Git/vendored dependency, font asset, runtime build script, proc macro or native C/C++ linkage was added. The lockfile includes this graph; standard dependency scans remain enabled without new source/license/advisory exceptions. Existing pinned checkout/artifact actions and distro QEMU/OVMF are reused as host-only tools.
