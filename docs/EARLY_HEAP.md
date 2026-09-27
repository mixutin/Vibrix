# Bounded early kernel heap

The M3 early heap owns a 64 KiB, page-aligned static arena inside the kernel
BSS. The existing ELF loader maps this backing supervisor-only, writable
and non-executable and zeroes BSS. Physical frame allocation is independent:
the arena remains part of the reserved kernel image, not conventional RAM.
No BootInfo or page-table ABI changes are needed.

`memory::heap::runtime::allocate(Layout)` returns a unique, aligned pointer
or null on exhaustion. Zero-size requests reserve one 16-byte granule.
`deallocate` releases an entire live allocation; adjacent freed granules can
serve a larger request. Bookkeeping lives outside the arena. A first-fit run
scan is deliberately simple and bounded; this is not a performance allocator.
Requests larger than the arena and address overflow fail without mutation.

Both APIs are unsafe: callers must be the sole boot CPU with IRQs disabled,
must not reenter the allocator, and must respect allocation lifetimes and
bounds. An allocation is not promised zeroed on reuse. Deallocation accepts
only the exact pointer returned for a currently live allocation, with all
references retired. Interior and out-of-range offsets are rejected, but these
checks cannot detect a stale pointer to a later allocation at the same address.
Do not enable asynchronous/SMP allocation without replacing this ownership
policy. No global Rust allocator is installed and no `alloc` crate is linked.

The host tests compile production run management and cover alignment against
the real arena base, exact-capacity exhaustion, reuse, fragmentation, invalid
releases, oversized inputs, address overflow and zero-sized requests.
The kernel smoke probe allocates 37-byte/64-aligned and 4096-byte/page-aligned
spans, writes and reads every byte with volatile accesses, frees the first,
checks first-fit reuse, writes the reused memory, checks the other allocation
remains intact, and frees both. QEMU requires the success marker on debugcon
and COM1 in normal, panic, breakpoint and page-fault builds.

Validation is pending until the exact commit's CI result is recorded. This
early heap does not imply dynamic growth, a virtual-memory manager, user
address spaces, SMP safety or physical-machine validation. The broader
roadmap remains open until its corresponding behavior is demonstrated.

Author: OpenAI Codex. Original implementation; no external implementation
source or new dependency. Rust `core::alloc::Layout`, raw-pointer and
`UnsafeCell` contracts govern the isolated unsafe backing access.
