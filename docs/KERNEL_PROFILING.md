# Bounded kernel scheduler profiling

The first M12 profiling slice exposes an observational snapshot of the existing
single-BSP kernel-thread scheduler. It reports cumulative context switches,
thread completions and timer-driven preemptions plus the current counts of
Ready, Running and Exited fixed-capacity thread slots.

The snapshot does not allocate, reset counters or alter scheduling decisions.
It is available only while the boot CPU owns the scheduler with interrupts
disabled and from the boot context, matching the scheduler's existing safety
boundary.

The normal cooperative QEMU smoke path now validates that the profile counters
contain the work it just performed before the existing thread evidence marker is
accepted.

This is deliberately bounded profiling. It is not sampling, stack unwinding,
per-function timing, PMU/performance-counter support, userspace profiling, SMP
accounting, flame graphs, or Target 001 performance characterization.
