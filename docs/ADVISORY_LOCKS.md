# Advisory file-lock core

Vibrix now has a fixed-capacity advisory lock primitive in the VFS descriptor
owner. A regular-file open description may hold either a shared or exclusive
lock.

- multiple independent shared locks may coexist;
- an exclusive lock conflicts with every lock held by another open description;
- upgrading a shared lock to exclusive succeeds only when no other description
  holds a lock;
- duplicated descriptors share the same open description and therefore the same
  lock;
- closing one duplicate does not release the lock; final close does;
- pipes/directories reject file locks;
- locking is non-blocking and reports `WouldBlock` on conflict.

The locks are deliberately advisory: ordinary reads and writes do not
implicitly consult them. This matches the intended Unix-style coordination
role and avoids turning an advisory API into mandatory access control.

This is the kernel core for M24 file locking/advisory locks. The roadmap item
remains unchecked until a userspace ABI and native Ring-3 evidence are layered
on top.


## Userspace ABI extension

This branch now reserves ABI v1 syscall 27 for non-blocking advisory locks.
Syscall 26 is intentionally left available to the concurrent atomic-rename M21
lane. The lock operation argument is one of unlock/shared/exclusive; unknown
operations fail with InvalidArgument. The syscall is covered by the IO process
promise and maps lock conflicts to the existing Busy errno. The safe Rust
userspace wrapper validates the operation before entering the kernel.

A native Ring-3 contention proof is still required before the M24 roadmap
checkbox is marked complete.
