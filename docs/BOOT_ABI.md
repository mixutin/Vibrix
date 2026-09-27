# Vibrix Boot ABI

The boot ABI is the contract between the Vibrix UEFI loader and the Vibrix kernel.

## Goals

- simple C-compatible memory layout
- versioned from the first kernel
- no dependency on UEFI types after firmware services are released
- architecture-specific details kept outside generic kernel code where practical

## BootInfo v1

The initial structure carries:

- magic/version
- framebuffer physical address and geometry
- framebuffer pixel format
- ACPI RSDP physical address
- UEFI-derived physical memory map
- memory descriptor size

The kernel must treat all pointers as untrusted boot-time inputs until validated.

## Proposed address-space semantics

[ADR 0001](decisions/0001-bootinfo-address-spaces.md) proposes physical
addresses for the framebuffer, ACPI RSDP and final memory-map copy, but
a virtual pointer for the entry argument `*const BootInfo` under the
active page tables. It also discusses buffer lifetimes and map-key retry
constraints. The ADR is **Proposed**; no ABI changes or working handoff
are implied until a reviewed implementation passes QEMU tests.

## Handoff

The intended sequence is:

1. loader opens `kernel.elf`
2. loader validates ELF64/x86-64 headers
3. loader allocates physical pages for PT_LOAD segments
4. loader maps/copies segments and clears BSS
5. loader discovers GOP framebuffer
6. loader discovers ACPI RSDP
7. loader obtains final UEFI memory map
8. loader constructs BootInfo
9. loader calls ExitBootServices
10. loader transfers control to `vibrix_kernel_entry`

After step 9 the kernel must not call UEFI Boot Services.
