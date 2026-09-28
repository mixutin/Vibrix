# ADR 0021: Bounded userspace ELF mapping into the private lower-half CR3

## Status

Accepted for the current single-BSP M5 bootstrap.

## Context

Vibrix already validates userspace ELF64 images through the shared parser and
M5 policy layer, and it owns a private lower-half PML4 slot for userspace.
Those two pieces are not enough to claim executable loading until validated
PT_LOAD segments are actually backed by private userspace pages, populated,
protected, and executed.

The early private VM intentionally has a small fixed frame pool. The general
ELF policy remains wider than this bootstrap sink; an image may be valid policy
input but still fail the current sink because it exceeds early VM capacity.

## Decision

The feature-gated M5 loader sink:

- accepts only page-aligned PT_LOAD starts in the current bootstrap;
- allocates guarded user mappings from the private lower-half VM;
- initially maps payload pages user RW/NX;
- stages file bytes and BSS zeros through a kernel-owned scratch slot while the
  kernel CR3 remains active;
- validates every staged range against the allocation that owns it;
- applies final RO/RX/RW permissions only during commit;
- rejects writable+executable images in the shared policy before sink mutation;
- verifies the ELF entry resolves to a user executable page;
- builds a separate guarded RW user stack;
- moves to the permanent higher-half transition stack before activating the
  private CR3; and
- enters CPL3 at the ELF header's validated entry address.

A sink failure aborts and releases every partial image allocation. The policy's
64 MiB validation ceiling is not a promise that the current fixed 32-frame
bootstrap VM can back a 64 MiB image.

## Evidence boundary

The QEMU proof uses a real ELF64 image with one RX PT_LOAD at 0x400000. The
payload executes a DPL3 diagnostic interrupt after loading, so the evidence
requires both the loader marker and the existing independent-CR3 CPL3 trap
marker under the same private root.

This proves the bounded executable-loading mechanism, not filesystem-backed
exec, process image replacement, demand paging, dynamic linking, ASLR, fork,
SMP TLB shootdown, or PID 1.
