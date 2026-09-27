# Vibrix Boot ABI

The boot ABI is the contract between the Vibrix UEFI loader and the Vibrix kernel.

## Goals

- simple C-compatible memory layout
- versioned from the first kernel
- no dependency on UEFI types after firmware services are released
- architecture-specific details kept outside generic kernel code where practical

## Shared Rust BootInfo v2 layout

The original v1 design specified magic/version, framebuffer physical address
and geometry/pixel format, ACPI RSDP physical address, and UEFI-derived
memory-map address, byte length and descriptor size.

Both `boot/src/main.rs` and `kernel/src/main.rs` now compile the same
`shared/bootinfo.rs` `#[repr(C)]` definition (88 bytes, alignment 8 on
x86-64). The first 80 bytes retain v1 offsets. The v2 tail is a firmware
`u32 memory_descriptor_version` at offset 80, followed by a zero
`u32 _reserved_v2` at offset 84. `_reserved` at offset 12 is also zero.

The immutable `BOOTINFO_MAGIC = 0x4942_5849_5242_4956` encodes
`VIBRIXBI` in little endian; `BOOTINFO_VERSION = 2` distinguishes the
layout. The memory-map key remains loader-only. The same first-party source
contains scalar validation and host regression tests for layout, unsupported
ABI/firmware versions, descriptor stride/byte count and address overflow.
No UEFI type appears in the stable ABI.

The kernel must treat all physical addresses as untrusted integers until
its page tables map them and it validates the corresponding backing.

## Accepted address-space semantics

[ADR 0001](decisions/0001-bootinfo-address-spaces.md) defines physical
addresses for the framebuffer, ACPI RSDP and final memory-map copy, but
a virtual pointer for the entry argument `*const BootInfo` under the
active page tables. Loader-owned handoff buffers use `EfiLoaderData`;
Vibrix must reserve those physical pages until consumed or copied.
The ADR also defines map-key retry and descriptor-stride constraints.
This accepted address-space design is not yet an implemented
post-`ExitBootServices` handoff. The shared ABI type is present, but the
kernel's initial mappings and transfer remain separate work.

## Accepted descriptor-version migration

[ADR 0004](decisions/0004-memory-descriptor-version.md) defines **BootInfo
v2** for the first implemented UEFI memory-map handoff. It appends an
explicit `u32 memory_descriptor_version` and a zero `u32 _reserved_v2`,
without reusing the existing reserved field or changing the physical
address / byte-count / stride semantics from ADR 0001. UEFI's descriptor
version and Vibrix's BootInfo version are independent numbers.

The Rust ABI type has now migrated together on loader and kernel to v2.
The loader stages a validated v2 value into a loader-owned
`EfiLoaderData` page allocated **before** the final `GetMemoryMap`;
it copies the physical map-buffer address, byte count, returned stride and
returned descriptor version from the **same** successful capture. The
QEMU-specific `VIBRIX: BootInfo v2 staged` marker proves loader-side
population only. It does not prove that page is mapped under the future
kernel CR3, that `ExitBootServices` succeeded or that the kernel read it.

## Kernel image staging

[ADR 0002](decisions/0002-kernel-load-layout.md) defines the loader-side
physical backing policy for the higher-half kernel image. Physical backing
addresses and linked virtual addresses are distinct concepts; later page-table
code must preserve that distinction explicitly.

The current staging step allocates loader-owned `EfiLoaderData` pages,
zeroes the complete image span, copies validated file-backed `PT_LOAD` bytes
and verifies BSS remains zero. The loader separately constructs and
software-verifies **inactive** higher-half page tables; it has not switched
CR3, mapped the BootInfo handoff page under the new tables or transferred
execution to the kernel.

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
