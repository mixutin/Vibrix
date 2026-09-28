# M5 x86-64 SYSCALL/SYSRETQ transport proof

This document defines the bounded completion boundary for the M5
**syscall/sysret** roadmap item. The stable userspace ABI is specified
separately in `docs/SYSCALL_ABI.md`; this slice proves the architectural
entry/return transport only.

## Architectural setup

The `syscall-probe` feature enables the production x86-64 transport module.
On the sole BSP, after the permanent GDT/TSS is loaded, the kernel:

- verifies CPUID reports long-mode `SYSCALL/SYSRET` support;
- programs `IA32_STAR` for kernel CS `0x08` / SS `0x10`;
- uses the SYSRET-compatible user descriptor order, yielding CS `0x23` and
  SS `0x1b`;
- programs `IA32_LSTAR` with the permanent kernel entry symbol;
- programs `IA32_FMASK` to clear TF, IF, DF and AC on entry;
- enables `IA32_EFER.SCE` only after the other MSRs and stack pointer are
  published;
- reads the MSRs back and fails closed if the values do not match.

The GDT user-data descriptor intentionally precedes the user-code descriptor.
This is required because `SYSRETQ` derives user SS as
`STAR[63:48] + 8` and user CS as `STAR[63:48] + 16`.

## Entry stack and register contract

Unlike an interrupt or privilege-changing `IRETQ`, `SYSCALL` does not load
`TSS.RSP0`. The bounded transport therefore owns a separate static, aligned
16 KiB CPL0 syscall stack.

The first assembly instructions do not use the incoming userspace stack. They
save user RSP to single-BSP static storage, switch to the dedicated kernel
stack, preserve RCX/R11 and the ABI argument registers
`RDI, RSI, RDX, R10, R8, R9`, call the Rust proof handler, restore that state,
reload the user RSP and execute `SYSRETQ`. RAX carries the handler result.

This storage and stack design is deliberately not re-entrant and not SMP-safe.

## QEMU proof

The userspace address-space probe stages this instruction stream when
`syscall-probe` is enabled:

1. load an intentionally undefined syscall number (`u64::MAX`) into RAX;
2. execute the real `syscall` instruction;
3. require the returned RAX to equal ABI v1 `NotSupported` (`-5`);
4. execute the existing DPL3 `int 0x80` diagnostic trap only on success;
5. fall into `ud2` if the result is wrong.

The CPL0 proof handler accepts the call only when it observes the expected
number while its live RSP is inside the dedicated syscall stack. The final
DPL3 diagnostic handler additionally requires that the private userspace CR3
is still active and that the syscall observation flag was set. It then reports
the final user selectors, proving control passed through
`SYSCALL -> CPL0 handler -> SYSRETQ -> CPL3` before the diagnostic trap.

## Completion boundary

A successful exact-head QEMU run proves the architectural MSR configuration,
real `SYSCALL` entry, explicit kernel-stack switch, ABI error return, real
`SYSRETQ` return, and continued execution in the same private userspace
address space.

It does **not** provide a general syscall dispatcher, copy-in/copy-out,
per-process kernel stacks, process/PID lifecycle, scheduler-integrated address
spaces, signal handling, nested/re-entrant syscalls, SMP/per-CPU MSRs or stacks,
a userspace Rust wrapper library, file descriptors, executable loading, or
PID 1.

## Evidence rule

Do not check the roadmap item merely because this code exists. The exact branch
must pass formatting, production-linked host tests, all-feature bare-metal
Clippy/build checks and the dedicated QEMU job. The QEMU job must independently
observe:

- `VIBRIX: kernel SYSCALL MSRs configured`;
- `VIBRIX: kernel SYSCALL entry reached`;
- `VIBRIX: kernel SYSCALL/SYSRETQ round trip verified`;
- the COM1 syscall line for number `0xffffffffffffffff`;
- the final CPL3 frame with CS `0x23` and SS `0x1b`.

Record the exact successful Actions run before checking the roadmap item.
