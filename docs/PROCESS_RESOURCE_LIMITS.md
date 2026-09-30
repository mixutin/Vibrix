# Per-process resource budget foundation

Vibrix now carries a monotonic resource budget in every process identity. The
budget covers five independently accounted classes:

- CPU ticks;
- resident memory pages;
- open-file reservations;
- socket reservations;
- live child processes.

A process may only replace its limits with an equal-or-narrower budget, and a
limit cannot be lowered beneath already-accounted use. Children inherit the
parent's current limits. Reservations fail before mutating usage when they would
overflow or exceed a limit. File, socket and memory reservations have explicit
release operations; CPU accounting is cumulative. Child capacity remains
charged while a child is a zombie and is released only after the parent reaps
it, so exit cannot be used to bypass a live-process ceiling.

This is deliberately a process-policy foundation. The M23 roadmap checkbox stays
open until the scheduler, address-space allocator, descriptor owner and socket
owner all call the corresponding admission/release hooks on their real resource
paths.
