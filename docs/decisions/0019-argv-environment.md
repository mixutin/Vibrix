# ADR 0019: initial argv and environment stack

Status: accepted as the M5 process-entry stack foundation; 2026-09-28.

## Decision

Build the first userspace entry stack without allocation in
`kernel/src/user_stack.rs`. The initial RSP points at a 64-bit argc, followed
by argv pointers and a NULL terminator, then envp pointers and a NULL
terminator. NUL-terminated argument/environment strings live above the vector
table in the same mapped stack.

The builder enforces:

- at most 16 argv entries and 16 environment entries;
- at most 2048 total bytes of strings including terminators;
- no embedded NUL bytes in supplied values;
- a stack wholly within the canonical lower-half userspace range and above the
  protected null page;
- checked address/size arithmetic;
- enough room for strings, pointer vectors and alignment padding;
- 16-byte aligned initial RSP;
- complete validation before any stack byte is modified.

The returned `InitialStack` exposes RSP plus argc/argv/envp addresses for the
future process-entry path. It does not rely on host pointer values.

## Scope boundary

This is argv/environment **construction policy**, not yet completion of the
ROADMAP `argv/environment` item. Completion requires mapping the destination
as a process-owned userspace stack, building these vectors there, entering a
real loaded userspace image in QEMU, and proving that userspace observes the
expected values.

No C runtime ABI, auxiliary vector, TLS, dynamic linker, inherited file
descriptors, credentials, locale or shell expansion is defined here.

## Validation

Host tests verify exact pointer-table layout and string bytes, empty vectors,
alignment, bounds and no mutation on invalid input. The module is linked into
the production kernel target so Clippy/build cover the no_std implementation.
The eventual checkbox waits for the real process-entry QEMU proof.
