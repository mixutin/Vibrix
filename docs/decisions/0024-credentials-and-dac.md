# ADR 0024: Process credentials and Unix discretionary access control

- **Status:** Proposed
- **Date:** 2026-09-29
- **Roadmap:** [M11 — Security and multi-user](../../ROADMAP.md#m11--security-and-multi-user)
- **Supersedes:** None

## Context

VibrixFS already stores traditional mode, uid and gid metadata, while the
process table has no credential identity to interpret it. M11 requires users,
groups, credentials and permissions before persistent multi-user operation can
be meaningful.

The first slice must remain allocation-free and usable by the current
single-BSP process model. It must not imply authentication, a persistent account
database, set-id execution, ACLs, capability policy or VFS permission
enforcement before those layers exist.

OpenBSD's documented process model distinguishes real, effective and saved user
and group IDs, plus a group access list. File access is selected from owner,
group or other mode classes using the effective identity, with effective UID 0
receiving superuser treatment while still not executing a regular file that has
no execute bit set.

## Decision

Introduce a kernel-owned, fixed-capacity credential object containing:

- real, effective and saved user IDs;
- real, effective and saved group IDs;
- up to eight supplementary group IDs;
- a superuser predicate based on effective UID 0;
- a fail-closed Unix owner/group/other DAC evaluator.

PID 1 starts with root credentials. New child process-table entries inherit the
parent's complete credential object unchanged. Credential mutation is
deliberately not exposed yet.

The DAC evaluator accepts validated VibrixFS-style mode bits and selects exactly
one class:

1. owner bits when effective UID equals the inode owner;
2. group bits when effective GID or a supplementary group matches the inode
   group;
3. other bits otherwise.

Effective UID 0 bypasses read/write DAC and directory search DAC. Executing a
regular file still requires at least one execute bit. Mode bits outside the
low twelve bits fail closed.

The model is a Vibrix implementation informed by documented Unix/OpenBSD
semantics. No OpenBSD source code is copied.

## Alternatives considered

### Effective IDs only

Smaller state, but it would force an ABI/model redesign as soon as set-id
execution or temporary privilege dropping is introduced. Real/effective/saved
IDs are cheap in the current fixed-capacity model.

### Store credentials outside process entries

A second identity table would add lifetime synchronization problems before the
scheduler/process model needs them. Credentials belong to process identity, so
process-table inheritance is the simpler ownership boundary.

### Implement VFS permission checks immediately

Rejected for this slice. The bootstrap RAM VFS and read-only VibrixFS backend
do not yet consistently expose owner/group/mode metadata through one syscall
path. First establish the identity and access-decision primitive; integrate it
into pathname traversal/open/create/mutation as a separately tested change.

## Consequences and compatibility

Existing PIDs, syscall numbers and userspace ABI records are unchanged. The
kernel Process structure gains credentials, but the public ProcessInfo ABI does
not expose them yet.

The bounded supplementary group list is intentionally conservative. Raising the
limit later does not change on-disk VibrixFS metadata, but exposing group lists
through a userspace ABI will require an explicit ABI contract.

This slice does not make Vibrix multi-user. Until VFS/pathname operations call
the DAC evaluator, file permissions remain metadata rather than an enforced
security boundary.

## Validation plan and evidence

The kernel library tests must cover:

- root and non-root identity construction;
- bounded supplementary-group membership;
- exclusive owner/group/other class selection;
- root DAC behavior including the regular-file execute exception;
- rejection of invalid mode bits;
- process credential inheritance.

The normal post-firmware subsystem self-test also executes the credential DAC
policy and emits a marker only after the checks pass. The M11 top-level
users/groups/credentials and permissions checkboxes remain open until the
persistent account model, credential-transition policy and VFS enforcement have
their own exact-head evidence.

## References

Primary behavior references checked 2026-09-29:

- OpenBSD intro(2), process credentials and file access permissions:
  https://man.openbsd.org/intro.2
- OpenBSD setresuid(2), real/effective/saved identity model:
  https://man.openbsd.org/setresuid.2
- OpenBSD setgroups(2), group access list:
  https://man.openbsd.org/setgroups.2
- OpenBSD chmod(1), traditional owner/group/other mode bits:
  https://man.openbsd.org/chmod.1

No external implementation source was copied.
