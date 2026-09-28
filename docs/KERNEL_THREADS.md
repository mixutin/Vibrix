# Cooperative kernel threads

Author: **GPT-5.6 Sol**.

This is the first M5 process/scheduling slice. It implements fixed-capacity
kernel threads and explicit cooperative context switching on the existing
single BSP. It does **not** implement preemption, Ring 3, syscalls or userspace
address spaces.

## Execution model

The kernel owns four static 16 KiB stacks in supervisor RW/NX BSS. Reusing an
exited slot clears its entire previous stack before installing a new synthetic
entry frame. A thread entry is an ordinary Rust `fn()`; returning from it
marks that thread exited and schedules the next Ready thread or the saved boot
context.

`yield_now()` is explicit and valid only from a running kernel thread while
IF=0. `run()` is valid only from the boot context and keeps resuming Ready
threads until none remain.

## x86-64 context contract

`vibrix_context_switch` is a small first-party assembly routine following the
SysV x86-64 call boundary. It saves/restores exactly the callee-saved general
registers:

- RBP
- RBX
- R12-R15
- RSP through the scheduler-owned context slot

Caller-saved registers are already allowed to change across the
`yield_now()` call. Kernel builds use `-C no-redzone=yes`. Synthetic stacks
leave the new Rust function entry at `RSP % 16 == 8`, matching the ABI after
a normal CALL. CR3 is unchanged; all cooperative threads share the current
kernel address space.

No thread-local FS/GS base, userspace state, XSAVE/FPU ownership or
privilege-transition stack is claimed by this slice.

## Ownership and safety

The scheduler and stack pool live in `UnsafeCell` statics under one strict
invariant: exactly one BSP may enter the APIs and maskable interrupts remain
disabled. The public APIs reject IF=1. No Rust reference to scheduler state is
kept across the raw stack switch; only raw pointers into static scheduler
storage are passed to assembly.

This design intentionally does not make the early physical allocator, heap or
managed mapper IRQ-safe. Thread stacks are bounded static allocations and have
no guard pages yet; stack protection remains separate security work.

## Evidence

The QEMU-debug build creates two real kernel threads. Their entry functions
produce guest-origin markers in this required order:

`A phase 1 → B phase 1 → A phase 2 → B phase 2`

The route requires five real context switches:

1. boot → A
2. A → B
3. B → A
4. exited A → B
5. exited B → boot

The normal QEMU smoke runner rejects missing, duplicate or reordered markers
and requires the final COM1 line reporting five switches and two completed
threads. Host tests independently check the synthetic stack alignment and
round-robin Ready-thread selection.

## Remaining M5 work

- timer-driven preemptive scheduling
- interrupt-safe scheduler synchronization
- privilege-transition RSP0/IST policy
- Ring 3 and per-process address spaces
- syscall/sysret ABI
- PID/process lifecycle, executable loading and wait/exit

Those remain separate roadmap items.
