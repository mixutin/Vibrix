# ADR 0017: bounded PID and process lifecycle

Status: accepted for the early single-kernel process model; 2026-09-28.

## Decision

Introduce a fixed-capacity, allocation-free process table in `kernel/src/process.rs`.
The table owns monotonically allocated process IDs, parent relationships and the
`Running -> Zombie -> reaped` lifecycle. A process remains present after exit
until its live parent waits for it, so exit status is not lost and storage is
not reused early.

PID 1 is reserved as the first process created by `spawn_init`. Child creation
requires a live running parent. PIDs are never reused during the lifetime of a
table even when a process slot is reaped and reused. The implementation fails
closed on table capacity or PID-space exhaustion.

When a non-init process exits, its live children are adopted by a running PID 1.
If PID 1 is absent or is itself exiting, those children become parentless rather
than retaining a reference to a dead parent. Waiting may select any child or one
specific PID. A matching running child reports Pending; a matching zombie is
atomically removed and returns its status; a non-child reports NoChild.

## Scope boundary

This ADR completes only the process identity/lifecycle model. It does not claim:

- scheduler-integrated userspace processes;
- a per-process CR3 allocator or context switch;
- syscall dispatch or userspace pointer copying;
- executable loading;
- argv/environment construction;
- blocking wait queues or signals;
- file-descriptor ownership/inheritance;
- credentials, sessions, process groups or SMP synchronization;
- a running PID 1 userspace program.

Those remain separate ROADMAP work. In particular, the M5 `wait/exit` checkbox
requires syscall-visible behavior rather than only this kernel lifecycle model.

## Validation

Production `vibrix-kernel` links the module and its normal subsystem self-test.
The self-test requires PID 1 allocation, multiple children, zombie retention,
wait/reap, non-reuse of a reaped PID, orphan adoption and pending wait behavior.
Host unit tests additionally cover capacity failure, invalid parents and
non-child waits. Exact-head CI must pass the real target build/Clippy and QEMU
regressions before the roadmap item is checked.
