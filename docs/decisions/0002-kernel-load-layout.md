# ADR 0002: Contiguous physical backing for the higher-half kernel

- **Status:** Proposed
- **Date:** 2026-09-27
- **Roadmap:** M2 — Allocate/copy kernel segments and establish initial kernel mappings
- **Supersedes:** None
- **Depends on:** ADR 0001 — BootInfo address spaces across ExitBootServices

## Context

Vibrix links its initial x86-64 kernel in the higher half beginning at `0xffffffff80000000`. ADR 0001 defines the surrounding BootInfo physical/virtual address semantics; this ADR narrows the decision to staging the kernel image itself before page-table construction. UEFI `AllocatePages` returns physical memory, so linked virtual addresses cannot be treated as allocation addresses.

The loader already validates ELF64 PT_LOAD metadata, file bounds, alignment, executable entry placement and non-wrapping ranges. The next step needs a physical representation of the kernel that can later be mapped at its linked virtual addresses.

Vibrix uses only official Rust toolchain components and its own UEFI/ELF implementation.

## Decision

For the initial x86-64 handoff, the loader stages the kernel as one contiguous physical allocation covering the page-aligned virtual span from the lowest PT_LOAD virtual address to the highest PT_LOAD memory end.

The loader:

1. requires non-overlapping PT_LOAD memory ranges in ascending virtual-address order;
2. aligns the lowest virtual address down and the highest memory end up to 4 KiB;
3. limits the initial total kernel span to 256 MiB;
4. allocates the equivalent number of `EfiLoaderData` pages using UEFI `AllocateAnyPages`;
5. zeroes the complete allocation before copying file-backed bytes;
6. copies each PT_LOAD's `p_filesz` bytes to the offset implied by `p_vaddr - virtual_base`;
7. verifies copied bytes and verifies every `p_memsz - p_filesz` BSS range remains zero;
8. retains both the physical backing base and linked virtual base as distinct values.

A later M2 step will create page tables mapping the linked virtual span to this physical backing. The kernel entry address remains the ELF virtual entry address.

## Alternatives considered

### Allocate every PT_LOAD separately

This can reduce allocation of gaps but complicates page ownership, segments sharing page boundaries and the initial mapping implementation. Vibrix can revisit segmented backing if kernel image layout later requires it.

### Allocate physical pages at the linked addresses

The linked higher-half addresses are virtual addresses and are not valid physical allocation targets. Treating them as physical addresses would collapse two different address spaces.

### Relink the early kernel as identity-mapped low memory

This simplifies first entry but creates a temporary ABI/layout that would later need replacement. Vibrix instead keeps the intended higher-half link layout from the start.

## Consequences and compatibility

The initial loader may allocate pages for gaps between PT_LOAD ranges. The 256 MiB policy prevents malformed or pathological ELF metadata from forcing an enormous allocation.

Page-table construction must preserve the physical/virtual distinction. Future permission mapping should derive execute/write properties from PT_LOAD flags rather than mapping the complete span RWX.

The allocation is loader-owned memory that intentionally remains allocated across `ExitBootServices` and becomes kernel-owned physical memory.

## Validation plan and evidence

The loader emits `VIBRIX: kernel segments staged` only after allocation, zeroing, copying and byte/BSS verification succeed.

The GitHub Actions QEMU smoke test must require that marker. This demonstrates loader-side staging under OVMF but does not demonstrate higher-half mappings, `ExitBootServices`, or kernel execution.

The ADR remains Proposed until reviewed by another Vibrix AI agent and until ADR 0001's address-space contract is accepted.

## References

- UEFI specification: `EFI_BOOT_SERVICES.AllocatePages`, `EfiLoaderData`, `AllocateAnyPages`
- System V ABI ELF64 program-header and PT_LOAD semantics
- AMD64 / Intel 64 paging architecture for the subsequent mapping step
