# ADR 0007: ExitBootServices and x86-64 kernel entry

- **Status:** Implemented for QEMU/OVMF; Target 001 untested
- **Date:** 2026-09-27
- **Roadmap:** M2 — firmware-to-kernel handoff
- **Depends on:** ADRs 0001, 0002, 0003, 0004 and 0006

## Decision and entry ABI

The bootloader stages ELF64 PT_LOAD segments under their linked higher-half
virtual addresses. It constructs an explicit four-level PML4, verifies all
kernel and narrowly scoped identity transition mappings and allocates one
EfiLoaderData page for BootInfo v2, a separate map buffer and a dedicated
16-page kernel stack **before final firmware map acquisition**.

The final GetMemoryMap into that existing mapped allocation returns one
key/length/stride/descriptor-version tuple. The loader writes a validated
BootInfo v2 instance using the tuple. It then invokes the cached firmware
ExitBootServices callback with the **same key**, making no other firmware
calls or allocations between those operations. If EFI_INVALID_PARAMETER
signals a stale key, it invokes **only** GetMemoryMap into the existing
buffer, rebuilds *all* of BootInfo's map metadata and retries up to four
total attempts. An oversized map, unexpected status or repeated invalid
key causes a QEMU debugcon error and fail-stop, not a return into
partially shut-down firmware.

Before exit the loader checks CPUID NX support, CR4.LA57=0 (four-level
paging), and whether its transition/staged-image/BootInfo/framebuffer
physical regions fit the CPU-reported physical-address width. After
successful ExitBootServices it enables IA32_EFER.NXE while retaining the
other bits, loads CR3 from the software-verified loader-owned PML4,
switches RSP to the top of the mapped 64 KiB kernel stack, places a fake
return slot for SysV x86-64 function-entry alignment and jumps **directly**
to the ELF entry, passing a mapped virtual BootInfo pointer in RDI.
Fixed distinct assembly registers prevent the RDI argument write from
clobbering the entry address.

The kernel checks BootInfo's aligned, non-null pointer and v1-prefix
version before reading the 8-byte v2 tail, then copies and validates its
88-byte contents. The initial PML4 identity-maps the physical framebuffer
as supervisor-writable, NX and PCD (uncached) before any volatile pixel
writes; the kernel does not call GOP or any UEFI service.

## Verification scope

The QEMU debugcon log requires *kernel-originated* post-firmware entry,
BootInfo validation, GDT/TSS loaded, initialized serial and framebuffer
write markers in addition to the loader's successful firmware-exit marker.
A separate QEMU COM1 serial file must contain `Vibrix kernel started.`;
the loader's firmware console and debugcon alone cannot satisfy it.
The optional `panic-probe` feature invokes the real kernel panic handler
after normal entry; CI separately checks kernel panic debugcon and COM1
output. That feature is for QEMU smoke only, not the default kernel build.

## Known limits

The initial memory-map ownership is retained. A later M3 QEMU-tested
monotonic frame allocator now issues conventional physical pages, but
does **not** reclaim loader-owned page tables, the stack, BootInfo or map. The temporary identity alias of
the writable+executable UEFI PE image is retained; W^X, teardown and
firmware-runtime-memory policy require later work. Only the RSDP entry
window and framebuffer are mapped; the whole ACPI table graph is not yet
mapped or parsed. The TSS does not have RSP0/IST stacks; the later ADR 0008 installs
a synchronous-exception IDT and QEMU-tests #BP/#PF, but interrupts remain
disabled and IRQ routing and scheduling are not implemented. Early output is
single-core only. PixelBitMask GOP format draws black until its pixel masks
are represented in a future ABI. No native USB driver, persistent root,
userspace or Target 001 physical evidence is implied.

## Sources

- [UEFI 2.10 Boot Services: GetMemoryMap / ExitBootServices](https://uefi.org/specs/UEFI/2.10/07_Services_Boot_Services.html)
- [Intel SDM vol. 3](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html), long-mode paging / IA32_EFER / CR3 / CR4
- [Vibrix shared BootInfo](../../shared/bootinfo.rs)
- [ADR 0006: identity transition mappings](0006-transition-mappings.md)
