# Userspace shell extras

The normal interactive QEMU/VNC boot path and framebuffer terminal are provided by the merged userspace-display implementation. This document covers the userspace-shell additions layered on top of that runtime.

## Commands

The shell keeps the existing file/process builtins and adds:

- `vfetch` — original Vibrix system summary
- `neofetch` / `fastfetch` — aliases for `vfetch`
- `uname` and `uname -a`
- `pid`
- `clear`

`clear` emits a bounded form-feed control handled natively by the framebuffer terminal, clearing the visible cells and homing the cursor without adding an ANSI parser or repeated full-screen scrolling.

## vfetch

`vfetch` is allocation-free and reports only values available from the running system:

- Vibrix / x86_64 identity
- shell version
- sanitized CPUID vendor and brand strings
- current x86 privilege ring
- PID from the userspace syscall
- UEFI boot and volatile bootstrap-RAM root

Unavailable memory accounting is printed as unavailable rather than estimated. Hardware strings are sanitized to printable ASCII before display.

The `neofetch` and `fastfetch` command names are only aliases for Vibrix's original `vfetch`; no code from those projects is included.

## Current limits

This remains the bounded bootstrap shell: RAM-backed files disappear when the VM stops, PID 1 exits end the session, and there is no general job control, executable search, package manager, ANSI/VT terminal layer, persistent USB root, or physical-hardware qualification.

The graphical terminal stays supervisor-owned and userspace output reaches it through the existing TTY/syscall path. These shell additions do not add framebuffer mappings, device access, or new syscalls.

## Validation

The shell parser and `vfetch` formatter have host tests, and the ordinary Vibrix CI continues to build and execute the userspace shell through the existing Ring 3/private-CR3/syscall/QEMU regression path.
