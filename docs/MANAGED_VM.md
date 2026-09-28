# Managed virtual memory: M3 implementation contract

Author: **GPT-6 Astra Pro**. No independent review is claimed.

The first incomplete functional roadmap item is M3's virtual-memory manager.
This implementation grows that item in dependency order rather than duplicating
open device, storage or networking PRs. The existing BootInfo v3 scratch window
remains unchanged and is used as a short-lived physical-access primitive, not
misrepresented as the general VM manager.

## Address and permission policy

The initial managed supervisor arena is PML4 slot 416:
`0xffffd00000000000..0xffffd08000000000` (exclusive upper bound). This is
separate from the scratch window, linked kernel and lower-half userspace.
Only 4 KiB write-back RAM mappings are supported. The physical-address width
must be 36 through 52 bits, and zero, misaligned and unrepresentable frames are
rejected. A validated physical number does not itself grant ownership.

Permission states are read-only, read/write and read/execute. There is no
write/execute state, user-accessible mapping, global page or MMIO mode. Regions
are bounded to 64 pages and cannot cross the arena boundary. The exclusive
`GuardedVm` facade reserves both absent guard pages and up to 62 payload pages.
It never exposes the raw mapper while guarded allocations are live.

## Ordered implementation slices

1. Checked page/frame/range/permission contract.
2. Explicit reserved-frame inventory and recycling.
3. Owned page-table traversal and numeric translation queries.
4. Dynamic table allocation and transactional page mapping.
5. Permission transitions and translation invalidation.
6. Unmapping and empty-table/backing-frame reclamation.
7. Zeroed multi-page regions and allocation rollback.
8. Guard-page-backed allocation layout and lifecycle.
9. Native x86 backend and real post-firmware mapping execution.
10. CPU fault probes and integration regression evidence.
11. Long-lived single-BSP kernel runtime service with IRQ-safe local mutation.

Each slice has its own PR. The host harness imports the production files, not
copies of their algorithms. The same sources form the first-party `vibrix-vmm`
workspace crate used by the kernel. The additive `Managed VM evidence` workflow
runs formatting, host tests, no-std bare-metal compilation/linting, and native
QEMU validation. The separate CPU fault workflow runs actual write-protection,
unmap, upper-guard and NX probes plus strict log-validator rejection tests.
Existing repository CI is not weakened.

## Implementation and evidence boundary

All ten slices contain implementation and validation code. A PR is not called
verified merely because it is open: inspect its current exact-head Actions
results. The ninth slice's corrected head passed real QEMU mapping, reclamation,
region and reserved-guard execution with subsequent timer/console readiness in
[Actions run 36412534469](https://github.com/mixutin/Vibrix/actions/runs/36412534469).
Later synchronized heads and the four CPU fault probes require their own runs.
No local Rust or QEMU execution is claimed from this authoring session.

The early native validation remains bounded, single-root, supervisor-only and
BSP/IRQs-off, retaining its 24-frame/96-KiB proof pool. Slice 11 adds a separate
persistent 96-frame kernel pool after APIC setup. It owns only scratch-window
slot 511, masks and restores local interrupts around page-table mutation, and
keeps ordinary arena mappings live with IF=1. The integrated proof holds a
guarded allocation across a real PIT interrupt, verifies the payload afterward,
then unmaps it and checks complete internal frame reclamation.

This is still one BSP and one kernel CR3. It is not a userspace address-space
manager, demand pager, dynamically growing heap, global physical-frame recycler,
or SMP shootdown implementation. Ring-3/user address spaces remain M5 work and
cross-CPU invalidation remains M12 work. The M3 checkbox should change only
after exact-head runtime evidence passes and the merged roadmap records that
bounded kernel-service definition.

See [native ownership and integration](MANAGED_VM_NATIVE.md) and
[CPU fault probes](MANAGED_VM_FAULTS.md) for the precise safety/evidence scope.

## References and compatibility

Original first-party implementation; no external OS implementation source or
third-party dependency. BootInfo, ELF, driver, filesystem and syscall ABIs do
not change. Primary architectural reference: Intel 64 and IA-32 SDM Volume 3A,
chapter 4 (paging, access rights and translation-cache invalidation), available
through [the Intel manual catalog](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html).
Native accesses must also satisfy [Rust volatile access requirements](https://doc.rust-lang.org/core/ptr/fn.write_volatile.html),
which do not provide inter-CPU synchronization. Deliberately invalid CPU probes
use assembly rather than violating the validity contract of Rust references or
volatile intrinsics.
