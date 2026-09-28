# Managed virtual memory: M3 implementation contract

Author: **GPT-6 Astra Pro**. No independent review is claimed.

The first incomplete functional roadmap item is M3's virtual-memory manager.
This implementation grows that item in dependency order rather than duplicating
open device, storage or networking PRs. The existing BootInfo v3 scratch window
remains unchanged and is not a general VM manager.

## Address and permission policy

The initial managed supervisor arena is PML4 slot 416:
`0xffffd00000000000..0xffffd08000000000` (exclusive upper bound). This is
separate from the scratch window, linked kernel and lower-half userspace.
Only 4 KiB write-back RAM mappings are supported. The physical-address width
must be 36 through 52 bits, and zero, misaligned and unrepresentable frames are
rejected. A validated physical number does not itself grant ownership.

Permission states are read-only, read/write and read/execute. There is no
write/execute state, user-accessible mapping, global page or MMIO mode. Regions
are bounded to 64 pages and cannot cross the arena boundary.

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

Each slice has its own PR. The host harness imports the production files, not
copies of their algorithms. The additive `Managed VM evidence` workflow runs
formatting, host tests and no-std bare-metal compilation/linting. Existing
repository CI is not weakened. Host success alone does not prove hardware
mapping, fault delivery, address-space switching, SMP or Target 001 support.

## Initial slice boundary

Only the checked contract is implemented in this first slice. No native PTE
is modified and no physical frame is acquired. The broad M3 checkbox remains
unchecked. Later slices must record their exact observed Actions results;
there is no local Rust or QEMU execution evidence from this authoring session.

## References and compatibility

Original first-party implementation; no external OS implementation source or
new dependency. BootInfo, ELF, driver, filesystem and syscall ABIs do not change.
Primary architectural reference: Intel 64 and IA-32 SDM Volume 3A, chapter 4
(paging, access rights and translation-cache invalidation), available through
[the Intel manual catalog](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html).
Native accesses must also satisfy [Rust volatile access requirements](https://doc.rust-lang.org/core/ptr/fn.write_volatile.html),
which do not provide inter-CPU synchronization.
