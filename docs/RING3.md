# M5 Ring 3 execution proof

This document defines the bounded completion boundary for the M5 **Ring 3
userspace** slice. It is an execution proof, not the Vibrix process model or
syscall ABI.

## What the probe executes

The `ring3-probe` kernel feature uses the long-lived single-BSP managed VM to
reserve two guarded user mappings inside its existing owned PML4 slot:

- one user RW/NX page for code staging, changed to RX before execution;
- one user RW/NX page for the CPL3 stack.

The fixed instruction stream is `int 0x80; ud2`. The kernel constructs a
hardware `IRETQ` frame with the GDT user selectors (CS `0x23`, SS `0x1b`)
and IF clear, then transfers from CPL0 to CPL3. Vector `0x80` exists only in
this feature-gated proof and is a DPL3 interrupt gate back to the ring-0 code
selector. The CPU must perform the privilege stack switch through the permanent
TSS RSP0 value installed during GDT/TSS initialization. The user-data descriptor intentionally precedes the user-code descriptor so the same GDT also satisfies x86-64 `SYSRETQ` selector derivation.

The diagnostic handler validates the CPU-pushed user selectors and confirms its
live kernel RSP lies inside the dedicated 16 KiB RSP0 stack. It reports the
result independently through debugcon and COM1 and then intentionally
fail-stops. The trailing `ud2` is unreachable when the trap path works.

## Completion boundary

A successful exact-head QEMU run demonstrates actual CPL3 instruction execution,
a user-executable mapping, a writable user stack, the CPL3-to-CPL0 interrupt
transition, and hardware use of TSS RSP0. It does **not** demonstrate separate
userspace address spaces. The probe still shares the kernel CR3 and the bounded
managed arena.

It also does not define syscall numbers or calling convention, user-pointer
validation, copy-in/copy-out, process IDs, executable loading, scheduling of
user processes, signal/exception delivery, a normal user-to-kernel return path,
SMP behavior, or physical Target 001 support. Those remain later M5/security
roadmap work.

## Evidence rule

Do not check the roadmap item merely because this code exists. The branch must
pass formatting, host tests, all-feature bare-metal Clippy/build validation and
the dedicated QEMU job that observes both
`VIBRIX: kernel CPL3 mappings ready` and
`VIBRIX: kernel CPL3 trap reached via TSS RSP0`, plus the COM1 frame line with
the exact user selectors. Record the exact Actions run when that evidence is
green.
