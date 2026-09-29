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

Numbers 9–14 are compatible ABI v1 extensions: the original 0–8 assignments
remain unchanged. A reserved number does **not** by itself imply that every
runtime configuration implements the call.

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
