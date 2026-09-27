# ADR 0004: Preserve UEFI memory-descriptor version in BootInfo

- **Status:** Accepted
- **Date:** 2026-09-27
- **Roadmap:** [M2 — Firmware-to-kernel handoff](../../ROADMAP.md)
- **Supersedes:** None
- **Depends on:** [ADR 0001 — BootInfo address spaces](0001-bootinfo-address-spaces.md)
- **Related:** [ADR 0002 — Kernel physical staging](0002-kernel-load-layout.md)

## Context

UEFI `GetMemoryMap` reports a descriptor-buffer byte length, a descriptor
stride (`DescriptorSize`), a map key and a `UINT32 DescriptorVersion`.
The descriptor version identifies the *firmware memory-descriptor format*,
not the Vibrix `BootInfo` format. They are separate version namespaces.

The current x86-64 `#[repr(C)] BootInfo` in `kernel/src/main.rs` has
`version: u32`, a pre-existing `_reserved: u32`, the physical address
`memory_map: u64`, `memory_map_len: u64` in bytes and
`memory_descriptor_size: u64` in bytes. It lacks the firmware descriptor
version. This is the explicit gap noted by accepted ADR 0001, item 7.
There is no implemented `GetMemoryMap`/kernel-entry handoff to preserve as
a working deployed binary ABI.

UEFI currently defines `EFI_MEMORY_DESCRIPTOR_VERSION` as **1**, and
requires software to use the returned `DescriptorSize` rather than
`size_of::<EFI_MEMORY_DESCRIPTOR>()` as the iteration stride.

## Decision (accepted design; no Rust change in this PR)

Define the first *implemented* map-carrying handoff as **Vibrix BootInfo
version 2**. Retain every existing v1 field at its original offset and
append exactly these fields at the end of the x86-64 `#[repr(C)]` struct:

```rust
// Append after the existing memory_descriptor_size: u64.
pub memory_descriptor_version: u32, // raw GetMemoryMap DescriptorVersion
pub _reserved_v2: u32,               // explicitly zero on write
```

`_reserved` at offset 12 retains its existing reserved meaning: **do not
repurpose it** for the descriptor version. `_reserved_v2` is also zero,
not an implicit size field, flag, version extension or firmware field.
Both versions keep their existing `magic` contract unchanged.

### Explicit layout/migration on x86-64

| Property | Existing v1 design | Proposed v2 design |
| --- | --- | --- |
| `version` at byte offset 8 | `1` | `2` |
| `_reserved` at offset 12 | zero | zero |
| `memory_map` at offset 56 | physical address | same |
| `memory_map_len` at offset 64 | byte count | same |
| `memory_descriptor_size` at offset 72 | byte stride | same |
| `memory_descriptor_version` at offset 80 | absent | firmware `u32` |
| `_reserved_v2` at offset 84 | absent | zero `u32` |
| `size_of::<BootInfo>()` | 80 bytes | 88 bytes |
| Alignment | 8 bytes | 8 bytes |

These are the reviewed *target* offsets/sizes for a future coordinated
implementation, not a change made by this documentation PR. A future
host-side ABI-layout test must assert `size_of`, `align_of`, and
`offset_of!` for all fields on the target architecture. A compiler must
not guess at an unversioned trailing field.

The future loader must populate `version = 2`, the final map buffer
physical address, actual returned byte length, returned descriptor stride,
and the **actual returned** `DescriptorVersion` from the *same successful
final* `GetMemoryMap` call. It must not hard-code the firmware version to
1 or carry over a value from an earlier map. If `ExitBootServices`
rejects a stale key, refresh the entire map/key/size/stride/version tuple
in the preallocated buffer before retrying; follow ADR 0001's allocation
and buffer-lifetime rules. The map key is firmware control state used to
exit boot services, **not** a persistent field in `BootInfo`.

The future kernel must validate the common v1 prefix, reject any
`BootInfo.version` other than 2 **before reading byte 80 or beyond**, and
verify that the full v2 object is mapped/readable before accessing the
new tail. It must then reject an unknown
`memory_descriptor_version` before parsing or recycling pages.
The initial supported firmware descriptor version is 1. An unfamiliar
future version requires an explicit compatibility review, even if its
prefix looks familiar. A v1 loader and a v2 kernel are **not**
interoperable for memory-map handoff, and v2 is not asserted to work
with an existing v1 kernel.

After the version gate, validate `memory_map_len > 0`, nonzero
`memory_descriptor_size`, a minimum supported descriptor prefix,
whole-descriptor length (`len % stride == 0`), checked map address
range, and accessible mapped backing memory before parsing. Iterate by
the returned stride and preserve any additional firmware descriptor
bytes rather than truncating the entire map to an internal struct
stride. The physical address remains a numeric value, not a
dereferenceable pointer, as ADR 0001 requires.

## Alternatives considered

- **Store DescriptorVersion in the existing `_reserved`:** avoids a size
  change but silently changes a documented reserved field and provides no
  explicit BootInfo version migration; rejected.
- **Use only `BootInfo.version`:** conflates Vibrix's ABI version with a
  firmware-defined descriptor format; rejected.
- **Append a `u64` firmware version:** consumes eight bytes for UEFI's
  `UINT32` without documenting a future-use half; the explicit
  `u32 + zero u32` tail is clearer.
- **Copy and normalize into a new Vibrix-only memory map:** adds a
  separate versioned representation, translation policy and runtime
  attribute compatibility concerns. Defer to a dedicated ADR if needed.

## Consequences and compatibility

The implementation PR must update the loader and kernel together,
including the kernel's `BootInfo` definition and the matching loader
layout or shared first-party ABI module. It must not quietly append a
field to one side only. Accepted ADR 0001's address spaces and ownership
are unchanged. ADR 0002's kernel physical staging is unaffected.

Version 2 describes a migration from an **unimplemented v1 design**,
not a proven on-disk, userspace or runtime compatibility promise.
Unknown versions fail closed until deliberately supported.

## Validation plan and evidence

**This accepted ADR changes documentation only.** It does not add Rust fields,
invoke `GetMemoryMap`, execute `ExitBootServices` or mark any roadmap
checkbox complete. Current QEMU staging evidence proves neither a final
memory map nor kernel execution.

Implementation PR acceptance should require:

1. Target-layout tests for both sides' v2 struct size, alignment and
   offsets, plus zeroed reserved fields and unsupported ABI versions.
2. Controlled descriptor-version tests: firmware version 1 accepted;
   zero/unknown version rejected **before** descriptor parsing.
3. Descriptor stride/byte-count tests (zero, shorter than supported
   prefix, partial descriptor, overflowing address/length, valid padded
   stride).
4. Fresh `GetMemoryMap` result and key after a simulated stale-key
   retry; no allocation between the final successful map and
   `ExitBootServices`.
5. QEMU/OVMF evidence of actual final map metadata, successful
   `ExitBootServices`, and a separate post-firmware **kernel** marker.
   Host-only validation and the existing loader staging marker cannot
   substitute for that behavior.

## Primary references

- [UEFI Specification — GetMemoryMap and ExitBootServices](https://uefi.org/specs/UEFI/2.10/07_Services_Boot_Services.html),
  including `UINT32 DescriptorVersion`, returned descriptor stride,
  and map-key retry requirements.
- [UEFI Specification 2.10](https://uefi.org/sites/default/files/resources/UEFI_Spec_2_10_Aug29.pdf),
  definition `EFI_MEMORY_DESCRIPTOR_VERSION 1`.
- [Vibrix Boot ABI](../BOOT_ABI.md) and accepted
  [ADR 0001](0001-bootinfo-address-spaces.md).

No third-party operating-system implementation source informed this
proposal.


## Review disposition

Accepted after Nyx / RIFT verified the current v1 Rust layout and VESPER performed a synchronized-head second-agent technical review on PR #27. Acceptance defines the future BootInfo v2 contract only; it does not claim that GetMemoryMap, ExitBootServices, or kernel entry are implemented.
