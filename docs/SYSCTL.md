# Bounded sysctl-like runtime query interface

Vibrix exposes a small read-only `sysctl` built-in in the native Ring 3 shell.
It is an administrative query surface, not an OpenBSD/FreeBSD ABI clone.

Supported names:

| Name | Source |
| --- | --- |
| `kern.ostype` | Vibrix identity constant |
| `kern.osrelease` | native shell package version |
| `hw.machine` | current x86-64 userspace build identity |
| `kern.pid` | live `getpid` syscall |
| `kern.processes` | live bounded `process_info` enumeration |
| `vfs.root` | current namespace root lookup |
| `vfs.dev` | current `/dev` lookup |

`sysctl -a` reads every documented key. Unknown names return an error.
Assignments such as `sysctl kern.pid=9` are rejected as usage errors. There is
no kernel-memory address exposure, arbitrary OID traversal, persistence,
write-side tuning, hardware serial/identifier disclosure, or compatibility
promise with another Unix.

The implementation deliberately uses existing syscall/VFS contracts rather than
inventing values for dynamic state. The host shell tests use a fixture where
`/dev` is absent and require the query to report `unavailable`. Exact-head
native desktop/QEMU evidence exercises the same compiled shell with a mounted
`/dev` and live PID/process table.
