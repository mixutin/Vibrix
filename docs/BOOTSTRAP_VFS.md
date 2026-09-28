# Bootstrap VFS and RAM files

The M6 bootstrap stack is an allocation-free, safe Rust kernel implementation.
It mounts a volatile memory filesystem at `/` and a device filesystem at `/dev`.
The ordinary development console owns one namespace and descriptor table for
the lifetime of its loop. `/tmp` and `/welcome` are populated during console
initialization. Reboot discards every byte.

## Try it in QEMU

Run `./tools/run-qemu.sh`, then type:

```text
ls /
cat /welcome
mkdir /tmp/test
write /tmp/test/note hello from vibrix
cat /tmp/test/note
ls /dev
pipe
rm /tmp/test/note
```

`write` creates or replaces a RAM file; text is the rest of the line and is
not shell-expanded. `ls` defaults to `/`. `cat` accepts regular files only and
replaces nonprinting bytes with dots, so `/dev/zero` cannot hang the console or
file contents inject terminal controls. `pipe` sends and reads a short string
through the production descriptor/pipe path. These are development console
commands at CPL0, not M6's future userspace shell or core utilities.

## Implementation boundaries

- `Filesystem` supplies node lookup, metadata, directory iteration,
  creation/removal, positional I/O and truncate. `Vfs` walks actual components
  and crosses mounted directory roots; it does not normalize away a missing
  or non-directory component before `..`.
- Memory inode generations reject stale handles after slot reuse. Capacity and
  offset failures occur before writes. Holes are zero-filled and truncation
  clears old data. No timestamp, credentials or permission enforcement exists.
- The table has shared open descriptions: dup shares access/offset, separate
  opens are independent, append uses the current file length under the same
  exclusive owner, and close releases only its reference. Unlink while open
  returns Busy. Namespace mutation is mediated by the table while it exists.
- `/dev/null` discards writes and returns EOF; `/dev/zero` fills read buffers.
  Device names are read-only namespace entries. There are no hardware nodes.
- Pipes preserve byte order through ring wrap, retain ends across dup, signal
  WouldBlock when a live pipe is empty/full, drain before EOF, and return
  BrokenPipe after the last reader closes. Writes no larger than the ring are
  all-or-nothing. Larger writes return NoSpace. No scheduler wait queues,
  signals, cross-process inheritance or blocking behavior is claimed.

Console limits: 16 inodes including root, 256 bytes/file, two mounts, 16
descriptor slots, two 256-byte pipes, 31 bytes/name, 255 bytes/path and 16
resolved directory levels. The console's existing 80-byte line editor is an
additional input limit. The library types permit different bounded capacities.
All state is exclusively borrowed; this change introduces no unsafe Rust.

## Validation

`cargo test --locked -p vibrix-kernel --lib vfs::` runs the production modules.
The same behavior proof executes after ExitBootServices and reports four
markers only after checking data and rejection behavior. Canonical CI requires
these markers in normal QEMU boot, and the new filesystem keyboard run requires
both independent COM1 and debugcon proof markers plus exact serial lines from
write/read, device listing, pipe roundtrip and removal. It must also observe a
real reboot under `-no-reboot` before its 40-second timeout.

```bash
VIBRIX_QEMU_FILES_PROBE=1 ./tools/test-qemu.sh
```

No native USB writes, VibrixFS VFS driver, persistent root, userspace pointer
access, file syscalls, process ownership, TTY or hardware validation is included.
The syscall v1 contract is unchanged. See [ADR 0016](decisions/0016-bootstrap-vfs.md)
for research, design alternatives and compatibility requirements.
