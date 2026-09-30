# Synchronization primitives

M12 introduces two allocation-free primitives intended for future SMP use:

- a FIFO ticket lock that gives each waiter a unique monotonically increasing
  ticket, publishes protected writes with Release ordering and observes the
  served ticket with Acquire ordering;
- a one-shot boot barrier with a fixed participant count, suitable for bringing
  a bounded CPU set through a startup phase exactly once.

Ticket allocation fails closed on counter exhaustion rather than wrapping into a
live ticket. The boot barrier rejects zero participants and any extra arrival
after release. Host tests use real threads to contend on the ticket lock and
arrive concurrently at the barrier; the normal post-firmware kernel executes an
uncontended production self-test.

This completes synchronization *primitives*, not SMP itself. AP startup,
preemptive multi-CPU scheduling, lock integration across existing subsystems and
TLB shootdowns remain separate M12 work.
