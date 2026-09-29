# Vibrix process-creation compatibility semantics

This document defines the M24 process-creation compatibility target. It is a
design/ABI contract, not a claim that POSIX `fork()` exists today.

## Current direction

Vibrix uses explicit process creation rather than implicit address-space cloning.
The long-term portable interface is a spawn/exec model:

1. the caller supplies an executable path, argv, environment and descriptor
   actions;
2. the kernel creates a new process identity and private userspace address space;
3. the executable image is validated and loaded before the child becomes
   runnable;
4. requested descriptor inheritance/duplication/closure is applied explicitly;
5. credentials are inherited unless an authorized execution transition says
   otherwise;
6. failure before publication is transactional from the caller's perspective:
   no runnable half-created child is exposed.

The native kernel may keep an internal primitive that creates a process record
before image activation, but that is not the portable userspace contract.

## Compatibility target

Portable software should prefer a `posix_spawn()`-like compatibility layer.
A future libc may implement portable spawn calls directly over the native
Vibrix primitive.

Traditional `fork()` is **not** currently promised. If a later compatibility
layer adds it, it must either implement true copy-on-write address-space cloning
with correct threads/signals/locks semantics, or document a deliberately
restricted compatibility subset. It must not silently emulate `fork()` by
running a fresh executable while claiming fork semantics.

`exec` semantics replace the calling process image while retaining the process
identity and explicitly permitted descriptors/credentials. Successful exec does
not return. Failed exec leaves the old image runnable and unchanged.

## Security requirements

- executable mapping validation preserves W^X;
- argv/environment copying is bounded and validated before publication;
- inherited descriptors are opt-in/explicit under the compatibility layer;
- set-ID or capability transitions, if later supported, occur only after image
  authentication/validation and before userspace entry;
- no-new-privileges and sandbox restrictions, once present, are monotonic across
  spawn/exec unless a stricter rule applies;
- a failed load cannot expose a partially initialized child.

## Non-claims

This contract does not claim implemented POSIX fork, pthread-at-fork behavior,
copy-on-write cloning, signals, job control, dynamic linking, descriptor actions,
or a complete libc. Those remain separate roadmap work.
