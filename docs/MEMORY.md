# Early physical frame allocation

The bare-metal kernel now compiles `kernel/src/memory/frame_allocator.rs`,
an allocation-free, monotonic 4-KiB physical frame allocator over a
loader-owned copy of the **final** UEFI memory map. It supports the
firmware descriptor **version 1**, respects the returned firmware
descriptor **stride** (not Rust structure size), validates all descriptors
before exposing any frame, and rejects overlapped/zero-length/misaligned/
overflowing physical ranges. A 16 MiB map-buffer limit matches the loader's
current capture policy.

Only UEFI `EfiConventionalMemory` (type 7) pages are eligible;
`EfiLoaderData` (type 2) and all other types are never yielded. This keeps
the staged kernel, initial page tables, BootInfo and map buffer reserved
rather than assuming bootloader allocations may be immediately reused.
Page zero is always skipped. Callers may supply additional explicit,
page-aligned `ReservedFrames` exclusion ranges. Each allocation advances a
cursor, so a given frame can be issued only once by one allocator instance.

The implementation returns a **numeric physical address**. It does not
make a new virtual mapping, zero the page, create Rust references to a
physical address, or claim that it can release a frame. A future mapped,
owned frame wrapper and an explicit free/reuse path are needed before
this becomes the kernel's general-purpose frame allocator.

`from_memory_map` takes an already accessible `&[u8]`, with a lifetime
bound to its firmware-copy allocation. Future kernel call sites must map
and retain the full loader-owned memory-map allocation before creating that
slice; they must not treat a BootInfo physical integer as a Rust pointer.
It must also preserve this allocator's ownership state across all consumers
rather than constructing multiple allocators from the same map.

Host tests compile **the actual production module** and cover padded
descriptor strides, reserved firmware allocations, extra exclusion ranges,
exhaustion, zero page, unsupported descriptor versions, invalid lengths,
overlaps, page alignment, upper physical-address overflow and runtime-marked
conventional pages. CI also Clippy-checks and builds the real bare-metal
kernel including the module.

This is M3 **physical frame allocator groundwork**, not a completed M3
checkbox: it needs safe kernel-lifetime initialization, physical-page
zero/mapping, allocator integration and a real QEMU allocation/reservation
proof. It does not provide persistent USB storage or user memory.

Primary references: UEFI Specification, `GetMemoryMap`,
`EFI_MEMORY_DESCRIPTOR` and memory type enumeration; Intel x86-64
page-address width conventions; Vibrix ADRs 0001/0004/0006.
The code uses no external crate, allocator or firmware call.
