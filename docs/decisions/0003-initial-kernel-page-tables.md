# ADR 0003: Initial higher-half kernel page tables

- **Status:** Accepted
- **Date:** 2026-09-27
- **Roadmap:** M2 — Establish initial kernel mappings
- **Depends on:** ADR 0001 — BootInfo address spaces; ADR 0002 — kernel physical staging

## Context

The Vibrix loader now stages validated PT_LOAD contents into one contiguous
EfiLoaderData physical allocation while preserving the linked higher-half
virtual base. The kernel still cannot execute at its linked addresses until an
x86-64 page-table hierarchy maps those virtual pages to the staged physical
backing.

This step intentionally stops before replacing the firmware's active CR3.
Final handoff still needs BootInfo/map-buffer mappings, a transition strategy,
the final UEFI memory map and ExitBootServices sequencing.

## Decision

The initial x86-64 kernel mapping hierarchy uses four-level 4 KiB paging.

The loader will:

1. allocate page-table pages as loader-owned EfiLoaderData;
2. zero each page before use;
3. create only supervisor mappings for nonempty PT_LOAD memory pages;
4. map each linked virtual page to the corresponding offset in ADR 0002's contiguous physical backing;
5. derive write permission from ELF PF_W;
6. derive execute permission from ELF PF_X, setting the x86-64 NX bit on non-executable pages;
7. reject conflicting mappings rather than silently widening permissions;
8. use only 4 KiB leaves in this first implementation;
9. software-walk the newly built hierarchy and verify physical destination plus W/NX/U permissions before reporting success;
10. leave the hierarchy inactive until the later firmware-handoff step.

Gaps inside ADR 0002's contiguous physical allocation remain unmapped unless a
PT_LOAD memory range covers them.

## Activation requirements deferred to the handoff step

Building tables is not permission to load CR3 yet.

Before activation, Vibrix must:

- ensure the CPU supports NX and enable EFER.NXE before using entries with NX;
- ensure the transition uses four-level paging (CR4.LA57 must not remain set for this hierarchy);
- add mappings needed by the actual transition path, kernel stack, BootInfo and memory-map backing;
- deliberately map ACPI/framebuffer regions when needed under ADR 0001;
- acquire the final firmware memory map and perform the ExitBootServices sequence without invalidating its key.

The current PR must not claim kernel execution or post-firmware operation.

## GNU_RELRO scope and deferred hardening

The currently linked kernel is a static ELF64 ET_EXEC image: an independent
build/readelf check found no dynamic section. The linker emits a GNU_RELRO
program header covering `.got`, but the first-stage builder intentionally
reads only PT_LOAD flags; `.got` is PF_W and its constructed 4 KiB leaf is
**writable + NX**, even though it is separately page-aligned. A GNU_RELRO
header does **not** itself change any PTE. This PR makes no RELRO enforcement
claim and does not activate these tables or enter untrusted userspace.

Later handoff/hardening work must explicitly decide when any GOT relocations
are complete and, before claiming a read-only GOT or stronger immutability
contract, either remap the dedicated GOT page read-only or document a reviewed
reason it must remain writable. A future linker change producing additional
writable orphan sections or shared-protection pages requires renewed ELF
page-granularity validation; the measured separation here applies to the
current kernel artifact only.

## Physical-address assumptions

Page-table entries encode the architectural address field through bit 51.
The loader rejects addresses that cannot be represented in that field and
requires page alignment. A later activation/feature-validation step should also
compare allocations against the CPU's reported physical-address width before
loading CR3.

Pre-ExitBootServices access to AllocatePages memory uses the same directly
accessible firmware address-space invariant already exercised by ADR 0002's
kernel staging code.

## Consequences

- kernel text can be read-only executable;
- read-only data can be read-only NX;
- writable data/BSS can be writable NX;
- kernel pages remain supervisor-only;
- unused allocation gaps are not accidentally exposed;
- additional handoff mappings can be added later without changing the staged kernel layout.

The loader retains all page-table allocations across ExitBootServices; the
future kernel physical allocator must reserve them until it deliberately takes
ownership or replaces the hierarchy.

## Validation

The loader emits `VIBRIX: kernel page tables verified` only after a software
walk confirms every nonempty PT_LOAD page maps to the expected physical backing
with exact write/execute and supervisor permissions.

GitHub Actions/QEMU must require that marker.

On the PR #29 linker-layout fix at `e235661968ab2edb0de1348d950040d286318cdc`,
[GitHub Actions run 36324148140](https://github.com/mixutin/Vibrix/actions/runs/36324148140)
passed format, host ELF/GPT/CPUID tests, boot/kernel Clippy and builds,
and the OVMF/QEMU smoke gate emitted `VIBRIX: kernel page tables verified`.
The measured linked kernel ELF places executable .text, read-only .rodata,
writable .got and writable .data on separate 4 KiB page intervals. The
prior conflict arose when read-only .rodata and writable .got were both in
virtual page `0xffffffff80002000`; the linker now starts .got at
`0xffffffff80003000` without broadening page permissions. The acceptance
records the implemented inactive-hierarchy design and this observed behavior.

This proves construction and verification of an inactive hierarchy. It does
not prove CR3 activation, kernel entry, ExitBootServices or Target 001
bare-metal behavior.

## References

- AMD64 Architecture Programmer's Manual, long-mode page translation
- Intel 64 and IA-32 SDM, 4-level paging and page-table entry permissions
- System V ELF PT_LOAD flags
- Vibrix ADR 0001 and ADR 0002

## Later activation — PR #49

The original construction-only scope above remains historical. The subsequent
[ADR 0006](0006-transition-mappings.md) adds verified narrow identity
mappings and [ADR 0007](0007-uefi-exit-kernel-entry.md) activates the hierarchy
and enters the standalone kernel. [QEMU CI 36337520346](https://github.com/mixutin/Vibrix/actions/runs/36337520346)
proves the later firmware exit and kernel-side serial/BootInfo/GOP behavior;
physical Target 001 operation is still untested.
