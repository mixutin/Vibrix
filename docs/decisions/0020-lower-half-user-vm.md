# ADR 0020: lower-half ownership for the first process address space

## Decision

The managed kernel VM keeps its existing upper PML4 slot 416 ownership. A
separate constructor may create a VM owner for one explicitly selected empty
PML4 slot. The first userspace address space owns **slot 0** only.

`Page::new` remains restricted to the upper kernel arena. Lower userspace
pages require `Page::new_user`, which accepts page-aligned addresses from
`0x1000` through the end of PML4 slot 0 (512 GiB exclusive). This prevents a
general managed-kernel caller from silently gaining permission to mutate lower
addresses.

The private userspace root no longer inherits kernel slot 0. Every other
supervisor root entry is copied, and any inherited U/S bit is rejected. The
existing CPL3 probe now maps:

- code at `0x0040_0000`, guarded on both sides;
- stack payload at `0x007f_f000`, with top `0x0080_0000`.

This matches ABI v1's lower-half pointer model and the initial ELF loader's
expected executable addresses.

## Safety boundary

Each `Vm` records the one PML4 slot it owns and rejects a `Page` whose root
index differs, even when the Page was created by another valid constructor.
The default `Vm::new` still owns slot 416, preserving existing callers.

The private address-space backend only permits page-table access to its
reserved frames or the selected root slot. It remains single-BSP, bounded,
preallocated, and without PCID or SMP TLB shootdowns.

## Evidence

Host VMM tests cover constructor-domain separation, explicit root-slot
ownership, and guarded lower-user ranges. The existing exact-QEMU
`address-space-probe` must enter CPL3 at `0x400000`, preserve the private
CR3 across the DPL3 diagnostic trap, and leave all existing VM/protection
regressions green.
