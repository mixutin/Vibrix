# Userspace display and automatic interactive boot

Authoring model: **GPT-6 Astra Pro**.

## Launch

```bash
./tools/run-qemu.sh
./tools/run-qemu.sh --vnc
./tools/run-qemu.sh --vnc=1
./tools/run-qemu.sh --kernel-console
```

The ordinary launcher selects the `userspace-shell` kernel feature without
requiring an environment variable or a command in the old kernel console.
It builds the actual `vibrix-sh` ELF, uses the existing validated loader/private
CR3 path, enters Ring 3 as PID 1 and connects fd 0/1/2 to `/dev/tty`. The prompt
and command results originate in that userspace executable, not in the renderer.
The kernel-owned terminal presents the real TTY output on retained GOP pixels.

`--vnc` uses display 0, TCP `127.0.0.1:5900`; display 1 uses port 5901. The
same framebuffer and keyboard route are used by QEMU's local graphics window
and its VNC display. Only displays 0 through 99 are accepted. This is native
VNC, not a bundled noVNC browser server. Connect with a VNC viewer.

VNC is intentionally localhost-only and has no built-in encryption or login.
For a remote host use an authenticated SSH tunnel, for example:

```bash
ssh -N -L 5900:127.0.0.1:5900 user@qemu-host
```

Then connect the viewer to the local port. Do not expose this development
listener directly on an untrusted network. Other users of the same host may
still access a localhost listener; this is not a multi-user access-control layer.

`--kernel-console` selects the old diagnostic profile. An explicit
`VIBRIX_KERNEL_FEATURES` value overrides the launcher's normal profile; unset
it to use the automatic shell. Direct `tools/build-qemu.sh` and the legacy
`tools/test-qemu.sh` intentionally retain their diagnostic default so existing
fault/IRQ/probe workflows are not silently changed. Cargo minimal defaults are
unchanged. The interactive feature reuses `rust-shell-probe` internals rather
than duplicating their boot/ELF/syscall implementation.

## Display and input

The first-party renderer supports printable ASCII, replacement glyphs for
non-ASCII bytes, LF/CR, tabs, destructive backspace, delayed wrapping, scrolling
and a visible underline cursor. A fixed cell buffer caps the screen at 80x30
cells, each 12x16 pixels. It needs no heap, firmware text protocol, font file or
external parser. Grayscale pixels work in both supported RGB and BGR layouts;
unknown bitmask layouts are never guessed.

The established native PS/2 decoder feeds the same canonical TTY as before.
Only accepted input is echoed; Backspace cannot erase an empty input line's
prompt. Typed tabs occupy one echo cell so canonical deletion remains aligned.
The input frontend reserves one byte for Enter instead of filling all 256
canonical input bytes and becoming unable to commit a line. Output tabs still
use four-column stops. The decoder remains the existing limited unshifted
set-one subset; this does not add full keyboard layouts or modifier support.
The canonical Ctrl-U hook is handled when that byte reaches the frontend, but
this change does not claim a newly implemented physical Ctrl-U scan-code path.

## Ownership and safety

The loader supplies validated, supervisor-writable, NX, uncached GOP backing.
The boot marker is the last temporary owner; before STI it initializes the
terminal exactly once. `address_space::copy_kernel_root` already copies the
supervisor GOP hierarchy into the private shell CR3 while replacing the user
arena. No framebuffer pages are made user-accessible and no new root switch
is necessary for output.

Each pixel uses checked coordinates, stride and byte arithmetic against the
validated backing size. Host tests include row-padding and tail sentinels.
The terminal owns its raw mapping for its full lifetime; raw pixel storage
never escapes to userspace. The static owner is single-BSP, IF=0, with an atomic
non-reentrant borrow guard. IRQ/NMI/panic paths do not call this renderer.
The guard is a refusal mechanism, not a claim of SMP graphics support.

TTY writes use already copied kernel-owned bytes. Only the `/dev/tty` output
queue is mirrored, never arbitrary file writes. Small/bitmask displays preserve
the original safe marker and serial fallback. Runtime rendering failure disables
the frontend without borrowing it recursively; serial output stays independent.
COM1 and debugcon remain in `build/qemu/interactive-serial.log` and
`build/qemu/interactive-debugcon.log`. Do not run multiple launchers in one
checkout simultaneously: the build directory and firmware-variable copy are
per-checkout development state.

## Verification

```bash
rustc --edition=2024 --test kernel/src/framebuffer.rs -o /tmp/vibrix-framebuffer-tests
/tmp/vibrix-framebuffer-tests
python3 tools/test_userspace_display.py --unit
python3 tools/test_userspace_display.py
```

The last command needs the same Rust/QEMU/OVMF prerequisites as ordinary boot.
It starts the real launcher with no kernel-feature override, negotiates an RFB
3.8 session on localhost display 97, sends actual VNC KeyEvents, and captures
QEMU framebuffer pixels using a private Unix QMP socket. It checks the initial
`vibrix$ ` prompt, visible `echo vnx` corrected to `echo vnc` with Backspace
before Enter, then the userspace command result and next prompt. It also checks
the independent COM1 transcript. No OCR, fake guest marker or generated screen
can satisfy the pixel assertions. Negative tests reject blank/truncated images.

The `Userspace display` workflow retains `.ppm` screenshots and logs. Its
result is separate from the canonical formatting, target, dependency and QEMU
regression checks; all must pass on the tested head before integration. Passing
host tests alone is not VNC evidence. Refer to the PR's exact-head Actions
results for observed status, rather than treating this test specification as a
claim that a run passed.

## Remaining boundaries

This is a bounded ASCII terminal frontend over the first userspace environment,
not a desktop, an ANSI/Unicode terminal emulator, a service manager, shell
respawn, general process scheduling, USB HID-to-TTY integration or persistent
USB root. `exit` retains the existing PID 1 exit behavior. The bootstrap files
remain in RAM and disappear at reboot. Physical hardware is not qualified by
QEMU results.

## Research and alternatives

Checked 2026-09-29: QEMU's [system invocation guide](https://www.qemu.org/docs/master/system/invocation.html)
(`-display`, `-vnc`, `-k`), its [QMP reference](https://www.qemu.org/docs/master/interop/qemu-qmp-ref.html)
(`screendump`), and [RFC 6143](https://www.rfc-editor.org/rfc/rfc6143.html)
sections 7.1, 7.3 and 7.5.4 (RFB initialization/key events). Kernel integration
reuses the existing BootInfo/PixelSurface validation, supervisor PML4 inheritance
and checked TTY/syscall copy-in/out rather than introducing a parallel ABI.

A QEMU serial-console display would not prove native guest framebuffer output.
A full terminal/Unicode library would widen scope beyond this bounded frontend;
the original fixed ASCII table and existing checked pixel writer avoid that
extra dependency and font-data supply chain. No third-party OS or font
implementation is copied, and no runtime dependency or lockfile is changed.
