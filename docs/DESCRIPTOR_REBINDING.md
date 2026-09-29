# Exact descriptor rebinding

Authoring model: **GPT-6 Astra Pro**.

`Files::dup2(source, target)` makes an exact descriptor refer to the source's
existing open description. It returns `target`, preserves shared file offsets
and access/append options, and works even when every descriptor is occupied.
This is the kernel primitive needed for later redirection and pipeline setup;
it is not a new syscall or a claim that the shell supports those features.

The source must be live and the target must be inside this table's capacity.
Both are checked before target closure. Self-duplication and an already-aliased
target succeed without changing reference counts. An invalid source or target
leaves all open descriptions intact. Replacing another description uses the
existing close path, so last-reader/last-writer transitions and pipe slot reuse
remain consistent. No temporary free descriptor or open description is needed.

The table's exclusive mutable borrow serializes the operation. This is atomic
with respect to this table's callers, not an SMP synchronization guarantee or a
persistent-storage transaction. There are no close-on-exec flags in this API.

## Evidence and boundaries

Seven production-linked host tests cover a full table, shared offsets, released
victim descriptions, invalid descriptors, repeated self/alias operations,
last-reader and last-writer replacement, pipe reuse, exact free targets, and
zero/single-descriptor capacities. The existing post-firmware VFS proof now
replaces an open null-device descriptor with a file, verifies the shared read
offset, and duplicates a pipe writer into exact slot 7. Its established COM1 and
debugcon markers remain conditional on every assertion passing.

Run `cargo test --locked --package vibrix-kernel --lib vfs::` and the ordinary
QEMU regression. Exact-head Actions evidence belongs in the implementation PR.
There are no new dependencies, unsafe Rust, ABI changes or physical disk writes.
No roadmap checkbox is changed.

## Research

Checked 2026-09-29: OpenBSD's `dup(2)` interface manual (2025-08-04 revision),
DESCRIPTION and ERRORS, documents shared open descriptions, exact target
replacement and self-duplication. Only documented interface behavior informed
this original Rust implementation; no BSD or other OS implementation code was
copied. The Open Group Issue 8 HTML endpoint returned HTTP 403 in this environment.
Rust core checked arithmetic and the existing exclusive `Files` owner supply
the implementation mechanisms; adding an external descriptor library would
not improve this small extension to the already-owned table.
