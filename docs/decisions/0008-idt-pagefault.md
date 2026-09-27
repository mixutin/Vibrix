# ADR 0008: Single-CPU x86-64 exception table and page-fault diagnostics

- **Status:** Implemented for QEMU after validation; IRQ/SMP deployment pending
- **Date:** 2026-09-27
- **Roadmap:** M3 — IDT + exception handlers; Page-fault diagnostics
- **Depends on:** ADR 0007 — successful post-firmware kernel entry and dedicated stack

## Decision

Following its real post-ExitBootServices entry, Vibrix initializes permanent
GDT/TSS, native COM1, and a kernel-lifetime 256-entry IDT. The first-stage IDT
uses x86-64 **16-byte interrupt gates** with supervisor code selector 0x08,
type/attrs 0x8e and IST=0; the IDTR limit is 4095. Only synchronous vectors
#BP=3, #DF=8, #GP=13 and #PF=14 are present. The remaining gates are absent.
The kernel does not enable IF or claim APIC IRQ routing, privilege-stack
switches, IST emergency stacks, userspace exceptions or SMP handling.

The Rust nightly `extern "x86-interrupt"` ABI (existing pinned nightly
toolchain) ensures machine-frame preservation and appropriate IRETQ on a
returning #BP handler, or error-code popping and a non-returning exception
handler for #DF/#GP/#PF. The first-party gate layout and page-fault error-bit
decoder are in `shared/idt_layout.rs` and host-tested directly.
`kernel/src/arch/x86_64/idt.rs` owns the permanently mapped, one-time
initialized UnsafeCell IDT; `LIDT` is executed after a valid GDT is loaded.

On #PF, read CR2 **before** any diagnostic output so the original faulting
linear address is preserved; report that address, saved RIP, raw error code,
and P/W/U/RSVD/I error bits over independent kernel COM1 output and write a
QEMU-specific debugcon marker. #GP/#DF also fail-stop with diagnostics;
#DF still lacks a dedicated IST and is not claimed safe under stack
exhaustion. #BP returns normally after reporting its RIP.

## Validation and scope

CI host tests assert `size_of::<IdtGate>() == 16`,
`size_of::<IdtTable>() == 4096`, `size_of::<IdtPointer>() == 10`,
offsets, full handler-address round trip and page-fault error-bit decoding.

QEMU's regular post-firmware kernel smoke requires the independent
`VIBRIX: kernel IDT installed` marker. Two opt-in QEMU kernel builds
perform hardware-triggered exception probes:
`breakpoint-probe` executes `int3` and checks the returning
`VIBRIX: kernel breakpoint exception handled`/native COM1 RIP log;
`page-fault-probe` accesses canonical unmapped low 1 TiB and checks
`VIBRIX: kernel page fault diagnostic` and actual COM1 CR2/error bits.
The normal build does not intentionally fault.

[Synchronized-head Actions run 36340579141](https://github.com/mixutin/Vibrix/actions/runs/36340579141)
passed host IDT layout/error-code tests, warning-denying Clippy and both
target builds, and the normal, panic, real breakpoint and real page-fault
QEMU probes **after** the kernel's merged ACPI and early memory allocator
initialization. The tested #BP handler returns with IRETQ; the #PF
handler captures the expected CR2 `0x10000000000` and reports raw error
bits via native COM1. The host fixtures are not substituted for the two
hardware-triggered trap probes.

These tests validate the QEMU boot CPU only, not Target 001, the installed
but not fault-injected #DF/#GP paths, double-fault IST recovery, hardware
IRQs, APIC timer, or user-mode fault isolation.

## References

- [Intel SDM vol. 3](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html), protected-mode exceptions, IDT gates, CR2 and page-fault error codes.
- [Rust x86-interrupt handler ABI](https://docs.rs/x86_64/latest/x86_64/structures/idt/type.HandlerFunc.html) (signature documentation only; Vibrix's implementation is first-party).
- [ADR 0007](0007-uefi-exit-kernel-entry.md).
