# ADR 0018: shared ELF64 parsing and bounded userspace image loading

Status: accepted as the M5 executable-loader foundation; 2026-09-28.

## Decision

Use one ELF64 parser source, `shared/elf.rs`, for both the UEFI bootloader and
kernel userspace loader. The parser remains the existing fail-closed x86-64
ET_EXEC validator: ELF64 little-endian only, fixed program-header size,
representable non-overlapping PT_LOAD ranges, file-size <= memory-size,
congruent power-of-two alignment, in-file payloads and a file-backed executable
entry point.

The kernel layers additional userspace policy in `kernel/src/user_image.rs`:

- reject mappings below 0x1000 so the null page remains unavailable;
- require the complete segment range to remain in the canonical lower 48-bit
  userspace half;
- reject any writable+executable segment (W^X);
- bound one image to 16 non-empty PT_LOAD segments and 64 MiB of mapped memory;
- validate the complete image before mutating the destination;
- map each segment, copy its file-backed bytes, zero its BSS tail, then commit
  the validated entry point;
- require a destination backend to support abort, and invoke abort on any
  begin/map/write/zero/commit failure.

A destination is represented by a small `Sink` trait so the format and loading
policy are testable without pretending that the current single diagnostic CR3
already provides a general process address-space allocator.

## Scope boundary

This is executable-loader **foundation**, not yet completion of the ROADMAP
`Executable loading` item. Completion requires connecting this loader to a
real process-owned userspace address space and proving entry into the loaded
image after ExitBootServices.

This ADR also does not define argv/environment layout, dynamic linking,
relocations, PIE, interpreters, demand paging, shared libraries, credentials,
file-descriptor inheritance or filesystem path lookup for exec.

## Validation

The existing bootloader ELF tests continue through `boot/src/elf.rs`, which
now re-exports the shared parser. Kernel tests require lower-half/W^X policy,
file copy plus BSS zeroing, no sink mutation for invalid images, and abort on an
injected backend failure. Exact-head target Clippy/build and the full QEMU
regression matrix must pass before this foundation is merged.
