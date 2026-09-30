# Descriptor-rights restriction

Vibrix descriptor rights are a monotonic per-file-descriptor authority mask layered
under the existing open-mode checks. The mask has three current rights:

- read,
- write,
- seek.

Opening a file derives the initial mask from its open mode. Pipe endpoints begin
with only their directional right. A process may query a descriptor's current
mask or replace it with a strict subset through additive syscall ABI v1 number
25 (`FdRights`). Unknown bits, attempts to add a removed right, and invalid
descriptors fail closed.

Rights belong to descriptor slots, not shared open descriptions. `dup` and
`dup2` copy the source descriptor's current reduced mask while preserving the
existing shared-offset semantics. Restricting one duplicate therefore does not
silently mutate another already-existing descriptor, while a newly duplicated
descriptor cannot regain authority that its source had already discarded.
Closing a descriptor clears its mask before the slot can be reused.

The existing process-promise gate treats descriptor-right operations as I/O, so
a process that has dropped the I/O promise cannot use the rights syscall to
inspect or manipulate descriptors.

## Evidence boundary

Production-linked host tests exercise monotonic reduction, failed expansion,
duplication after reduction, close/reuse behavior, and enforcement on
read/write/seek. The normal post-firmware kernel VFS self-test also reduces a
live descriptor and verifies write and seek denial before emitting
`VIBRIX: kernel descriptor rights verified`.

The additive userspace wrapper and process syscall dispatch path are compiled
and policy-tested. This is the bounded descriptor-rights primitive; it is not a
Capsicum-compatible API, does not yet provide per-ioctl/socket rights, does not
create filesystem namespaces, and does not claim general multi-process
descriptor inheritance or a complete application sandbox.
