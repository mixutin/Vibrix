# POSIX compatibility target and conformance matrix

Vibrix is an independent Unix-like operating system. Its compatibility target is
**source-level compatibility with a documented, deliberately selected subset of
POSIX.1-2024 / The Open Group Base Specifications Issue 8**, not certification,
UNIX trademark status, Linux ABI compatibility, or an assertion that currently
familiar command names are fully conforming.

## Status vocabulary

Each interface is classified as:

- **Implemented** — production Vibrix code exists and has runtime or host
  evidence appropriate to the interface.
- **Partial** — a useful subset exists but required POSIX semantics are missing.
- **Planned** — the project intends to provide the interface but has no
  conforming implementation yet.
- **Non-goal for current profile** — intentionally excluded from the current
  compatibility target.

A row may only advance when its cited implementation/evidence changes. The
matrix is therefore a compatibility ledger, not a marketing claim.

## Current matrix

| Area | POSIX-facing target | Vibrix status | Current boundary |
| --- | --- | --- | --- |
| Process identity | pid/getpid-style identity | Partial | Native PID lifecycle and getpid exist; sessions/process groups are not implemented. |
| Process termination/wait | exit/wait family concepts | Partial | Bounded PID 1/child lifecycle exists; full fork/exec/signals semantics do not. |
| Files/descriptors | open/read/write/close concepts | Partial | Native VFS and descriptor paths exist; metadata, locking, full flags and persistent root are incomplete. |
| Directories | mkdir/remove/read-directory concepts | Partial | Bounded VFS directory operations exist; rename/link/symlink and full permission enforcement remain. |
| Standard streams/TTY | byte-stream stdin/stdout/stderr concepts | Partial | Native TTY/syscall shell path exists; termios, PTYs and job control do not. |
| Shell language | command parsing and redirection | Partial | Deliberately bounded shell syntax; no variables, pipelines, command substitution, scripts or job control. |
| Core utilities | familiar text/file utility subset | Partial | Bounded built-ins are documented individually; option compatibility is not implied. |
| Time | clocks/timestamps | Planned | No general POSIX clock API or wall-clock contract. |
| Signals | signal delivery/masks/actions | Planned | No POSIX signal model. |
| Threads | pthread-style API | Planned | Kernel threads exist, not a POSIX pthread userspace contract. |
| Memory mapping | mmap/shared-memory concepts | Planned | Kernel/user VM foundations exist; no userspace mmap/shm ABI. |
| Sockets | socket API | Planned | Packet/TCP foundations are kernel-internal; no general userspace socket ABI. |
| User/group database | passwd/group-facing semantics | Planned | Credential groundwork exists; persistent accounts/login are incomplete. |
| Dynamic linking | shared objects/dlopen family | Planned | Current userspace ELF path is statically linked. |
| Locale/i18n | locale and wide-character interfaces | Non-goal for current profile | Bootstrap userspace is bounded ASCII/UTF-8-oriented and has no locale subsystem. |

## Conformance policy

1. Vibrix does not claim POSIX conformance globally.
2. A compatible name does not imply every standardized option or edge case.
3. Tests must distinguish host fixtures from actual Vibrix runtime evidence.
4. Unsupported behavior fails explicitly instead of silently delegating to a
   Linux host.
5. ABI compatibility and source compatibility are tracked separately.
6. Extensions must not be described as POSIX behavior unless the relevant
   normative requirement has been checked.
7. The matrix is updated whenever a roadmap item materially changes a row.

## References

Primary compatibility reference checked 2026-09-29:

- The Open Group Base Specifications Issue 8 / POSIX.1-2024:
  https://pubs.opengroup.org/onlinepubs/9799919799/

This document summarizes Vibrix's own target and status; no specification text
or implementation source is copied.
