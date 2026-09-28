# Timer-driven preemptive kernel scheduling

Author: **GPT-5.6 Sol**.

This is the second M5 scheduling slice and builds directly on the cooperative
thread/context-switch contract in [KERNEL_THREADS.md](KERNEL_THREADS.md).

## Interrupt-context switch model

The PIT timer enters the permanent x86 interrupt gate with IF cleared by the
CPU. The handler:

1. records the monotonic timer tick;
2. sends LAPIC EOI;
3. calls the scheduler preemption hook.

EOI deliberately occurs **before** any stack switch. If another Ready thread
exists, the hook marks the interrupted thread Ready and switches away while the
Rust x86-interrupt handler call frame remains on that thread's own stack.

When the scheduler later selects that thread again, the saved context resumes
inside the same preemption hook. It returns to the timer handler, whose compiler-
generated x86-interrupt epilogue executes IRETQ and restores the CPU-pushed
interrupted RIP/RFLAGS state. This means the proof does not manufacture a
software interrupt or manually reconstruct an interrupt frame.

## Thread lifecycle under preemption

A newly scheduled preemptive thread begins from the same ABI-validated
synthetic context as a cooperative thread, then executes STI immediately before
calling its Rust entry function. When the entry returns it executes CLI before
changing scheduler state or handing off to another thread.

The boot context starts a preemptive run with IF=0 after the PIT/LAPIC route is
known-good. Once every participating thread exits, the scheduler returns to that
saved boot context with IF=0, disables preemptive mode, and the QEMU proof
restores STI before continuing normal kernel startup.

## Real QEMU proof

The probe threads never call `yield_now()`.

Thread A publishes phase 1 and spins. Only a real timer preemption can let
Thread B publish phase 1. B then spins until a later timer preemption resumes A
for phase 2, and another timer preemption resumes B for phase 2.

CI requires these guest-origin markers in order:

`timer delivered → A1 → B1 → A2 → B2 → preemptive verified → console ready`

It also requires at least three scheduler switches attributed specifically to
timer preemption, at least six total context switches, exactly two thread
completions, and a working console afterward.

## Boundaries

This remains a one-BSP kernel scheduler. There are no locks, SMP run queues,
priorities, sleeps/wakeup, per-process address spaces, user mode, FPU/XSAVE
ownership, RSP0/IST privilege stacks, syscall ABI or process lifecycle yet.
The current PIT frequency is development evidence, not a stable scheduling ABI
or latency guarantee.
