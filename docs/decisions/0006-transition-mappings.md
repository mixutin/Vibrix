# ADR 0006: Narrow identity mappings for the firmware-to-kernel transition

- **Status:** Implemented staging; activation pending
- **Date:** 2026-09-27
- **Roadmap:** M2 — Establish initial kernel mappings / ExitBootServices
- **Depends on:** ADRs 0001, 0002, 0003 and 0004

## Decision

The existing higher-half 4 KiB PT_LOAD mappings remain supervisor-only,
with PF_W and PF_X determining W/NX. Before the final firmware map, the loader
additionally maps **only** the physical regions needed by the transition,
using 4 KiB identity leaves in the same **inactive** hierarchy:

| Region | Permission | Reason |
| --- | --- | --- |
| Actual UEFI loaded PE image | supervisor writable + executable | short transition code path; temporary W+X exception, no untrusted userspace |
| Dedicated 16-page EfiLoaderData kernel stack | supervisor writable + NX | independent of firmware's reclaimable stack |
| One-page EfiLoaderData BootInfo v2 allocation | supervisor writable + NX | valid entry-time virtual pointer via explicit identity mapping |
| Full allocated memory-map buffer capacity | supervisor writable + NX | kernel can validate/copy the final descriptor bytes |
| RSDP page(s) spanning a 4096-byte window | supervisor read-only + NX | entry-time RSDP metadata; ACPI tables require separate mappings |
| GOP framebuffer BAR | supervisor writable + NX + PCD | MMIO must not be mapped with default WB cache policy |

Every range is checked for arithmetic overflow, physical PTE mask,
canonical-48 identity address and a 256 MiB per-region policy bound. The
builder software-walks and verifies the physical destination and W/NX/U/PCD
leaf flags, rejecting conflicting aliases instead of widening permissions.
Regions must be owned/persistent or firmware-described; mappings alone do
not establish pointer provenance. Loader and handoff allocations must remain
reserved when a later physical allocator begins to reclaim memory.

This decision does not identity-map all RAM, map all ACPI SDTs, or promote
firmware handles into persistent USB identity. The temporary PE image
W+X identity alias must be removed by the future kernel mapping transition
before claiming global W^X.

## Final memory-map ordering

The loader now acquires a **provisional** owned EfiLoaderData map buffer
with 64 descriptors of headroom, then allocates the narrow page-table
extensions. It invokes a second `GetMemoryMap` into the **same allocation**
only after those allocations complete; `refresh` updates key/byte length,
descriptor stride and descriptor version as one tuple on success, preserves
the prior tuple on failure and never allocates or frees memory.

The loader constructs BootInfo v2 using the newly refreshed tuple. A future
`ExitBootServices` step must immediately use that key, and if firmware
rejects it as stale, refresh the same preallocated buffer rather than
allocating new pages or using stale metadata.

## Current validation boundary

The QEMU smoke marker `VIBRIX: transition mappings verified` is emitted
only after software leaf walks. `VIBRIX: final memory map captured` follows
the in-place refresh. The page tables are **still inactive** and the loader
still spins without `ExitBootServices`. Neither mapping, marker nor host
tests prove post-firmware kernel execution; the M2 checkbox stays open
until a real CR3 transition and kernel entry are observed.

## Primary references

- [UEFI 2.10 GetMemoryMap / ExitBootServices](https://uefi.org/specs/UEFI/2.10/07_Services_Boot_Services.html)
- [Intel SDM volume 3, paging and memory types](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html)
- [Vibrix ADR 0001](0001-bootinfo-address-spaces.md)
- [Vibrix ADR 0003](0003-initial-kernel-page-tables.md)
