# Vibrix UI toolkit

`vibrix-ui` is the allocation-free userspace GUI toolkit used by the native
Vibrix desktop. It extracts reusable UI policy from the original desktop
renderer without introducing a hosted window system or third-party runtime.

## Implemented primitives

The crate currently owns:

- the userspace `Canvas` fill/blit boundary;
- `NativeCanvas`, backed by the existing checked display syscalls;
- `Painter`, which clips fills to the display and emits bounded ASCII glyph
  tiles;
- a reusable Vibrix `Theme`;
- stable taskbar-button geometry and painting;
- bounded window-frame/title/control painting.

The desktop uses these primitives for the actual F1–F4 task buttons, window
frame and text drawing. Pointer hit testing delegates to the same task-button
geometry, avoiding a separate visual/input layout.

## Safety and limits

No raw framebuffer mapping enters userspace. The toolkit only reaches the
existing checked `DisplayFill` and `DisplayBlit` syscalls. Text tiles remain
bounded to 960 pixels, within the existing display ABI's 1024-pixel limit.

This is a deliberately small immediate-mode toolkit. It does not provide:

- a multi-process compositor or window server;
- dynamic allocation;
- Unicode/font shaping;
- GPU acceleration;
- retained widget trees;
- accessibility metadata;
- themes loaded from disk;
- arbitrary application spawning.

Those are later desktop/application-platform work.

## Verification

Host tests use a checking canvas that rejects out-of-bounds rectangles and
pixel-count mismatches. The actual target builds both `vibrix-ui` and
`vibrix-desktop` as `no_std` x86-64 userspace code.

The dedicated GUI-toolkit evidence workflow also runs the existing desktop
QEMU/RFB oracle. That proof boots the real userspace desktop, drives its
window/taskbar, and checks actual guest framebuffer pixels, so toolkit
integration is not inferred from host rendering alone.

## Design references and alternatives

The existing desktop design review (2026-09-29) evaluated
`embedded-graphics 0.8.2` as a maintained no_std alternative:
https://docs.rs/embedded-graphics/0.8.2/embedded_graphics/

The current Vibrix surface needs only the already-existing checked fill/blit
ABI, small fixed glyph tiles and a handful of widgets. Extracting those proven
first-party primitives avoids adding a larger drawing framework solely for
operations Vibrix already owns. This is a scoped choice, not a prohibition on
community GUI crates for future needs.
