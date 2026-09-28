# M5 userspace address-space proof

This document defines the bounded completion boundary for the M5 **Userspace
address spaces** slice. It extends the existing Ring 3 execution proof with an
independently owned x86-64 PML4 root; it is not yet the Vibrix process model.

## Construction

The `address-space-probe` feature reserves one fresh conventional-RAM frame
for a new PML4 plus a private 32-frame managed-VM pool. Scratch slot 510 is
owned only by this feature-gated service; the normal runtime managed VM keeps
its existing slot 511.

Before interrupts are enabled, the kernel copies its supervisor PML4 entries
into the new root while deliberately leaving the managed arena PML4 slot empty.
Copied root entries must not advertise the user bit. A fresh user hierarchy is
then created only beneath the private arena slot. The new root therefore shares
the kernel's supervisor mappings needed for traps and kernel execution, while
its user code/stack page tables and backing frames are owned separately from
the normal kernel CR3.

The probe allocates guarded user RW pages for code and stack. With local
interrupts masked, it switches CR3 to the private root, stages the fixed
`int 0x80; ud2` stream through the now-active user mapping, changes the code
page to RX, and enters CPL3 with the established GDT user selectors. The DPL3
diagnostic interrupt returns through the permanent TSS RSP0 stack without
switching CR3. The handler compares the live CR3 to the exact private root
recorded before user entry.

## Evidence boundary

A successful QEMU proof must establish all of the following on the exact branch
head:

- the private PML4 was prepared before STI;
- the active CR3 changed from the kernel root to a distinct private root;
- CPL3 executed from the private user mapping and trapped through TSS RSP0;
- the trap handler still observed the exact private CR3;
- the existing kernel/QEMU regression matrix remained green.

This checks a **bounded independently owned userspace address-space
foundation**. It does not yet provide a process address-space allocator,
multiple simultaneously runnable processes, CR3 switching in the scheduler,
PCID, SMP TLB shootdowns, copy-in/copy-out, demand paging, executable loading,
or a syscall ABI. Kernel supervisor mappings are intentionally shared between
the two roots.

## Safety constraints

The proof is single-BSP only. Initialization occurs with IF=0 and uses the
monotonic early frame allocator. Probe activation masks local interrupts before
CR3 mutation. Five-level paging, PCID and SMAP are rejected for this bounded
path. The private root and its page-table/data frames are retained for the rest
of the diagnostic boot; the terminal interrupt handler intentionally does not
return.
