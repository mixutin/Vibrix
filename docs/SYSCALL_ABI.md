# Vibrix syscall ABI v1

This document freezes the first userspace/kernel syscall contract. It defines
the interface only. The x86-64 `SYSCALL/SYSRET` entry path, native Rust wrapper
library, process lifecycle, executable loader, and real userspace services are
separate ROADMAP milestones.

## Version

ABI version is **1**. Existing numbers and register meanings in v1 are stable.
Future incompatible contracts require a new ABI version rather than silently
reinterpreting an existing call.

## x86-64 register convention

On entry:

| Purpose | Register |
| --- | --- |
| syscall number | `RAX` |
| arg0 | `RDI` |
| arg1 | `RSI` |
| arg2 | `RDX` |
| arg3 | `R10` |
| arg4 | `R8` |
| arg5 | `R9` |

On return, `RAX` contains either the success value or a negative errno encoded
in two's-complement. `RCX` and `R11` are never argument registers because
the architectural `SYSCALL/SYSRET` transport clobbers them.

Values `-1..=-4095` are reserved for errors. All other `RAX` bit patterns are
success values.

## v1 syscall numbers

| Number | Name | Intended signature |
| ---: | --- | --- |
| 0 | `exit` | `exit(status)` |
| 1 | `yield` | `yield()` |
| 2 | `getpid` | `getpid()` |
| 3 | `read` | `read(fd, buffer, length)` |
| 4 | `write` | `write(fd, buffer, length)` |
| 5 | `open` | `open(path, path_length, flags)` |
| 6 | `close` | `close(fd)` |
| 7 | `wait` | `wait(pid, status_ptr, options)` |
| 8 | `exec` | `exec(path, path_length, argv, envp)` |
| 9 | `create` | `create(path, path_length)` |
| 10 | `mkdir` | `mkdir(path, path_length)` |
| 11 | `remove` | `remove(path, path_length)` |
| 12 | `readdir` | `readdir(path, path_length, index, entry_ptr)` |
| 13 | `process_info` | `process_info(index, info_ptr)` |
| 14 | `kill` | `kill(pid, status)` |
| 15 | `display_info` | `display_info(info_ptr)` |
| 16 | `display_fill` | `display_fill(x, y, width, height, color)` |
| 17 | `display_blit` | `display_blit(x, y, width, height, pixels, count)` |
| 18 | `input_poll` | `input_poll(event_ptr)` |
| 19 | `getresuid` | `getresuid(id_triple_ptr)` |
| 20 | `setresuid` | `setresuid(real, effective, saved)` |
| 21 | `getresgid` | `getresgid(id_triple_ptr)` |
| 22 | `setresgid` | `setresgid(real, effective, saved)` |

Numbers 9–22 are compatible ABI v1 extensions: the original 0–8 assignments
remain unchanged. A reserved number does **not** by itself imply that every
runtime configuration implements the call.

## Bounded M6 filesystem and process services

The `userspace-io-probe` configuration binds descriptors 0, 1 and 2 to one
kernel-owned `/dev/tty`. Paths are length-delimited UTF-8, absolute, at most
255 bytes, and copied from checked userspace mappings before lookup. Empty
paths, embedded NULs, nonexistent parent components and file-as-directory
traversals fail. No symlinks or hard links are present in this namespace.

`open` accepts access values 0 (read), 1 (write) or 2 (read/write), optionally
ORed with bit 8 (truncate). Truncation requires write access. Other flags fail.
`create` creates an empty regular file and fails if it already exists;
`mkdir` creates a directory; `remove` rejects nonempty directories, mount roots
and files with open descriptors. Files and directories are volatile RAM state.

`readdir` indexes a live directory and returns 1 after copying a 40-byte
`DirEntry`, or 0 at the end. `kind`, `name_len`, six reserved zero bytes and a
32-byte name array form the record; names have an explicit length, not a
required NUL terminator. Mutation invalidates enumeration indices.

`process_info` indexes the bounded process table and returns 1 after copying
a 16-byte `ProcessInfo`; end of table is `NotFound`. The record contains
u32 PID, u32 parent PID (0 if absent), u8 state (1 running, 2 zombie), three
zero reserved bytes, and i32 exit status. `kill` currently allows PID 1 to
terminate a non-init table entry with an explicit status. It does not deliver
POSIX signals or stop a separately scheduled userspace thread. `core-utils-probe`
seeds a child table entry to demonstrate the running-to-zombie transition.

The Rust shell implements `cat`, `echo`, `ls`, `pwd`, `cd`, `mkdir`, `cp`, `mv`,
`rm`, `ps` and `kill` as builtins using these services. `cp`/`mv` reject aliases
of the source before opening a truncating destination. `mv` is copy followed
by remove, not atomic rename, and only handles regular files. No pipelines,
redirection, recursive copies, glob expansion or separately launched utilities
are implemented. Working-directory normalization happens only after a real
kernel directory lookup succeeds; ordinary paths preserve every component for
kernel validation.

## Bounded credential transition calls

`getresuid` and `getresgid` copy one `repr(C)` 12-byte `IdTriple`
(real/effective/saved u32 fields) through the checked userspace-copy path.
`setresuid` and `setresgid` take three scalar IDs; `0xffff_ffff` is reserved
as the v1 “leave unchanged” sentinel and cannot be assigned as an identity.

A process with effective UID 0 may select arbitrary non-sentinel IDs. A
non-root process may only select an ID already present in its current
real/effective/saved tuple. Validation happens before mutation, so a denied
multi-field transition changes none of the IDs. Group-ID transition privilege
is also governed by effective UID 0.

The native Rust PID 1 proof exercises these calls through the real
SYSCALL/SYSRET and copy-out path: it changes effective GID and UID to 1000 while
retaining real/saved 0, verifies both tuples, regains UID 0 through the saved-ID
rule, restores GID 0, verifies root tuples, then exits successfully. This is
credential-transition evidence, not authentication, a persistent account
database, set-user-ID executable semantics or filesystem DAC enforcement.

## Pointer contract

Pointer arguments are userspace virtual addresses, never kernel pointers and
never proof of accessibility. Before dereferencing, the kernel must validate
the complete `[address, address + length)` range against the active process
address space and copy through an explicit user-access primitive. ABI v1 uses
the canonical lower 48-bit half for userspace.

A zero-length buffer does not require the pointed address to be dereferenced.
Non-zero ranges that overflow or enter the upper half are invalid.

## Initial errno assignments

| Value | Name |
| ---: | --- |
| 1 | invalid argument |
| 2 | bad address |
| 3 | bad file descriptor |
| 4 | not found |
| 5 | not supported |
| 6 | no memory |
| 7 | busy |
| 8 | permission denied |
| 9 | interrupted |
| 10 | I/O error |

The shared source of truth is `shared/syscall_abi.rs`. CI compiles and tests
that exact file on the host so future kernel and userspace code can import one
contract rather than duplicating numbers.

## Completion boundary

This milestone means **ABI v1 is specified and machine-tested**. It does not
claim a working syscall instruction path, dispatcher, copy-in/copy-out,
userspace Rust library, PID lifecycle, file descriptors, executable loading, or
PID 1. Those remain separate ROADMAP items.
