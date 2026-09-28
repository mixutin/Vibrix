# Native managed VM: ownership and execution boundary

Author: **GPT-6 Astra Pro**. This is slice 9 of the ordered
[M3 managed-VM implementation](MANAGED_VM.md), not independent review.

## Production integration

`shared/vmm` is now the first-party `vibrix-vmm` 0.0.1 workspace crate, under
Vibrix's 0BSD license. Its production sources are the same ones imported by the
host harness. It has no external, transitive, native, build-script, proc-macro,
`std` or allocator dependency. Kernel linkage uses an explicit local path and
exact version; Cargo.lock records the three-package graph. This does not
replace the separately proposed public kernel library or its subsystem work.

After the single early physical allocator is initialized, and before APIC
activation or any other scratch-window owner exists, the kernel constructs a
private native adapter and exercises the actual active page tables. It does
not generate a synthetic success log or use a second copy of the mapper.

## Unsafe adapter invariants

Only the BSP runs, CPL0, IF=0, four-level paging, no PCID, no AP startup, and the
matched loader has enabled NX. The adapter checks CR3/CR4/RFLAGS, the supported
physical width, and PAT entry zero's WB encoding. The active root must be
retained non-runtime EfiLoaderData advertising WB; the 24 supplied frames must
be uniquely claimed EfiConventionalMemory advertising WB. Physical addresses
remain numbers until the existing BootInfo v3 scratch window maps them.

Every PTE read/write maps exactly one owned physical page in scratch slot 0,
performs a bounded aligned volatile access, and unmaps before returning. Root
access is restricted to arena slot 416. Zeroing is restricted to the acquired
pool; the root and arbitrary physical pages cannot be zeroed. No safe Rust
reference escapes. Scratch and arena aliases use WB consistently. Backend
access failures panic rather than continuing with partially published state.

Publication is child-before-parent. Unmapping detaches the leaf and empty
parents, then issues INVLPG before releasing frames to this pool. CR3 never
switches, and the code does not touch other PML4 slots. The private adapter
cannot escape into the IRQ-enabled phase. No device or DMA is activated.

## What the real boot executes

The native path maps distinct pages across 2 MiB and 1 GiB boundaries, reads
zeroed bytes, writes and compares full-page patterns, changes RW to RO, reclaims
and reuses the same physical frame without stale contents, preserves neighboring
pages, allocates and removes a cross-table region, reserves both guard pages,
rejects guard overlap, and tears everything down. A newly constructed scratch
owner independently verifies an empty window and an explicitly zero arena root
entry before success markers are printed on debugcon and COM1.

The physical allocator remains monotonic: the 24-page/96-KiB supply stays claimed
for this boot, even after all its internal frames become free. This is bounded
retention, not global frame reclamation or a growing heap. Dropping a live VM
leaks its retained backing rather than handing still-mapped pages to another
owner. Continuing after a native validation error is forbidden.

## Runtime kernel service after APIC activation

After the early proof and all temporary firmware/MMIO window users finish, APIC
activation intentionally retains window slots 0 and 1. The runtime VMM then
reserves slot 511 as its only scratch PTE, so it can inspect and update its owned
page-table frames without requiring the rest of the window to be empty. The
runtime pool contains 96 uniquely claimed conventional WB frames. It stays
owned for the boot, while table and data frames are recycled internally after
unmap.

The runtime state is a single-BSP `UnsafeCell` protected by saving RFLAGS,
executing CLI, performing one complete VM mutation, and restoring STI only when
the caller entered with IF=1. Calls from an interrupt context therefore remain
IRQ-off; calls from normal kernel context cannot be timer-preempted while page
tables are being changed. This is **not** an SMP lock and does not protect
against NMI reentrancy or a future AP.

The ordinary QEMU boot initializes this service before enabling interrupts.
After native PIT delivery, it maps a guarded two-page payload, checks zero-fill,
writes full-page patterns, keeps the allocation live across another real timer
interrupt, verifies the patterns, releases the allocation and requires all 96
pool frames plus all guarded slots to be free again. APIC window mappings remain
untouched.

## Evidence and remaining work

The additive Managed VM evidence workflow tests the actual crate, compiles and
lints bare-metal code, boots the integrated kernel and requires independent
native markers plus subsequent timer/console readiness. Existing full QEMU
regressions remain unchanged. Record observed exact-head Actions results in
the PR; local Rust/QEMU execution is unavailable in this authoring session.

CPU write-protection, post-unmap, guard and NX fault injection are covered by
slice 10. Slice 11 extends the mapper into the IRQ-enabled kernel lifetime on one
BSP. It still does not provide userspace address spaces, demand paging,
copy-on-write, a growing kernel heap, global kernel W^X, SMP shootdown, live
kernel/IST stack switching or Target 001 support.

References: Intel SDM Volume 3A chapter 4 (four-level paging, PAT and INVLPG),
UEFI memory descriptors, the existing BootInfo v3/Window contract, and
[Rust volatile access requirements](https://doc.rust-lang.org/core/ptr/fn.write_volatile.html).
No external operating-system implementation source was copied.
