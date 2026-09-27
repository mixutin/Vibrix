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

## Accepted address-space semantics

[ADR 0001](decisions/0001-bootinfo-address-spaces.md) defines physical
addresses for the framebuffer, ACPI RSDP and final memory-map copy, but
a virtual pointer for the entry argument `*const BootInfo` under the
active page tables. Loader-owned handoff buffers use `EfiLoaderData`;
Vibrix must reserve those physical pages until consumed or copied.
The ADR also defines map-key retry and descriptor-stride constraints.
This **accepted design is not implemented handoff**; no Rust ABI layout
change or successful post-`ExitBootServices` kernel entry is implied.

## Kernel image staging

[ADR 0002](decisions/0002-kernel-load-layout.md) defines the loader-side
physical backing policy for the higher-half kernel image. Physical backing
addresses and linked virtual addresses are distinct concepts; later page-table
code must preserve that distinction explicitly.

The current staging step allocates loader-owned `EfiLoaderData` pages, zeroes
the complete image span, copies validated file-backed `PT_LOAD` bytes and
verifies BSS remains zero. It does not yet establish the higher-half mappings
or transfer execution to the kernel.

## Handoff

The intended sequence is:

1. loader opens `kernel.elf`
2. loader validates ELF64/x86-64 headers
3. loader computes the page-aligned virtual span covering all PT_LOAD segments
4. loader allocates one contiguous physical backing span with UEFI AllocatePages
5. loader zeroes the backing span, copies file-backed PT_LOAD bytes and verifies BSS remains zero
6. loader establishes mappings from the linked higher-half virtual span to that physical backing
7. loader discovers GOP framebuffer
8. loader discovers ACPI RSDP
9. loader obtains final UEFI memory map
10. loader constructs BootInfo
11. loader calls ExitBootServices
12. loader transfers control to `vibrix_kernel_entry`

After successful `ExitBootServices` the kernel must not call UEFI Boot Services.
