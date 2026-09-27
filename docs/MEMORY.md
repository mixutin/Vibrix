# Early physical frame allocation

The bare-metal kernel now compiles `kernel/src/memory/frame_allocator.rs`,
an allocation-free, monotonic 4-KiB physical frame allocator over a
loader-owned copy of the **final** UEFI memory map. It supports the
firmware descriptor **version 1**, respects the returned firmware
descriptor **stride** (not Rust structure size), validates all descriptors
before exposing any frame, and rejects overlapped/zero-length/misaligned/
overflowing physical ranges. A 16 MiB map-buffer limit matches the loader's
current capture policy. The quadratic overlap check is bounded to 4096
descriptors, with larger maps rejected before the pairwise walk.

Only UEFI `EfiConventionalMemory` (type 7) pages are eligible;
`EfiLoaderData` (type 2) and all other types are never yielded. This keeps
the staged kernel, initial page tables, BootInfo and map buffer reserved
rather than assuming bootloader allocations may be immediately reused.
Page zero is always skipped. Callers may supply additional explicit,
page-aligned `ReservedFrames` exclusion ranges. Each allocation advances a
cursor, so a given frame can be issued only once by one allocator instance.

**Live QEMU runtime checkpoint:** the actual kernel now calls
`memory::init_from_boot_info` after `ExitBootServices`, using the
loader-owned *complete mapped memory-map range* (not a host fixture).
The map is validated once and the sole live allocator is kept in an
`UnsafeCell<Option<FrameAllocator<'static>>>` for this boot CPU. With
interrupts still disabled, it claims two different, nonzero, page-aligned
type-7 frames, emitting **kernel-only** QEMU debugcon markers
`VIBRIX: kernel frame allocator initialized` and
`VIBRIX: kernel conventional frames allocated`. The normal-boot and
panic-probe smoke tests require both markers. [CI run 36339836880](https://github.com/mixutin/Vibrix/actions/runs/36339836880)
passed exact-head Rust checks and both QEMU runs.

The state additionally reserves the entire map-byte span, the GOP
framebuffer and the RSDP window as rounded-up physical pages; these
exclude the corresponding ranges even if firmware reports an unexpected
conventional-memory type. The backing is loader-owned `EfiLoaderData` and deliberately withheld
from type-7 frame allocation for the entire early-kernel lifetime.
The global holder is intentionally **single-core and IRQs-off only**:
access functions are `unsafe` with that precondition. Before enabling
interrupt-driven allocation or SMP, replace its unsynchronized
`UnsafeCell` access with a reviewed, interrupt-safe lock and per-CPU
ownership policy. Do not create a second map allocator that might
independently issue the same frame.

The implementation still returns a **numeric physical address**. It does
not create a new virtual mapping, zero the page, create Rust references
to physical addresses, implement free/reuse, or guarantee all future
memory-map types can be reclaimed. A future mapped, owned frame wrapper
and an explicit reuse path are needed before general VM and heap use.
`from_memory_map` takes a mapped `&[u8]` and validates before issuing
any page; on the runtime path the loader explicitly maps its full range
before the CPU enters the kernel.

Host tests compile **the actual production module** and cover padded
descriptor strides, reserved firmware allocations, extra exclusion ranges,
exhaustion, zero page, unsupported descriptor versions, invalid lengths,
overlaps, page alignment, upper physical-address overflow and runtime-marked
conventional pages. CI also Clippy-checks and builds the real bare-metal
kernel including the module.

This is a limited but live M3 **physical frame allocator**: firmware-map
validation, sole early-kernel ownership and distinct conventional-page
claims are proven in QEMU. The checkbox does not imply a frame-free API,
general-purpose virtual memory management, zeroed/mapped new pages,
allocator reuse, SMP safety, a kernel heap, or user memory. Those are
separate follow-up tasks. It does not provide persistent USB storage.

Primary references: UEFI Specification, `GetMemoryMap`,
`EFI_MEMORY_DESCRIPTOR` and memory type enumeration; Intel x86-64
page-address width conventions; Vibrix ADRs 0001/0004/0006.
The code uses no external crate, allocator or firmware call.
