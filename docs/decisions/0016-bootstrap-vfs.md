# ADR 0016: bounded bootstrap VFS and streams

Status: accepted for the single-owner kernel bootstrap; 2026-09-28.

## Problem and decision

M6 needs real file operations before USB storage and process syscall dispatch
exist. Introduce a safe, allocation-free filesystem trait, a bounded mount
namespace, an in-memory root, a small device filesystem and descriptor-owned
streams. The development console owns these objects; interrupt handlers never
borrow them. Ordinary Rust exclusive borrows serialize operations. This is
volatile bootstrap storage, never a replacement for the removable USB root.

Paths are absolute UTF-8, bounded in length/depth, with checked component walks.
Repeated separators and `.` are accepted. `..` walks the resolved ancestor
stack (including across mount roots), clamps at namespace root, and never
skips validation of preceding components. NUL and overlong names fail. Mounts
cover existing directories and remain fixed once file tables borrow the VFS.
There is no symlink, rename, credentials or persistent mount support yet.

Each memory inode has a generation, bounded contents and a parent/name. Handles
to recycled slots fail rather than aliasing new files. Unlink of an open object
is rejected with Busy in this first contract. A directory must be empty before
removal. File writes check capacity and arithmetic before modifying bytes;
holes are zero-filled. Filesystem operations take node IDs, never raw pointers.

Descriptor numbers index a bounded table of shared open descriptions. Separate
opens have separate offsets; dup shares offset/access state and closes only its
own reference. Open/pipe failure must leave tables and file data unchanged.
Read/write access, seekability and closed descriptors are checked. Pipe ends
refer to bounded ring buffers; empty live pipes return WouldBlock, closing the
last writer gives EOF after draining, and a writer without readers receives
BrokenPipe. Writes up to the ring capacity are atomic: insufficient room returns
WouldBlock without a partial write. There are no blocking waits or signals.
`/dev/null` discards writes and reads EOF; `/dev/zero` fills reads with zero.
Neither endpoint accesses firmware or hardware.

## Research and alternatives

Checked 2026-09-28:

- Rust Reference, [references and pointers](https://doc.rust-lang.org/reference/types/pointer.html):
  exclusive mutable references supply ownership without global unsafe state.
- Linux man-pages project's interface documentation for
  [read](https://man7.org/linux/man-pages/man2/read.2.html),
  [pipe](https://man7.org/linux/man-pages/man2/pipe.2.html), and
  [dup](https://man7.org/linux/man-pages/man2/dup.2.html): short reads, descriptor
  versus open-description lifetime, shared offsets and pipe endpoint ownership.
  Only documented interface concepts were consulted; no OS implementation was
  copied, and Vibrix does not claim POSIX or Linux syscall compatibility.
- [embedded-io 0.7.1](https://docs.rs/embedded-io/0.7.1/embedded_io/), maintained
  by the Rust embedded HAL team, offers no_std read/write/seek traits but no
  namespace, mount or descriptor ownership. Its Read/Write contract is blocking;
  the first Vibrix pipe API explicitly returns WouldBlock. It is therefore not
  added to this bounded implementation. A later adapter can be reviewed when
  scheduler-backed blocking exists.
- Open Group Issue 8 and Issue 7 online read/pipe/dup pages returned HTTP 403.
  No claim of having verified their exact text is made.

A heap-backed tree and reference-counted trait objects would simplify arbitrary
growth but require a general allocator and introduce unbounded resource use.
Fixed capacities fit this early kernel and make exhaustion deterministic. No
external dependency, build script, runtime or new license is introduced.

## Compatibility and validation criteria

The kernel API is internal and deliberately not frozen as a userspace ABI.
Syscall ABI v1 numbers are unchanged. No user pointers are accepted. A later
dispatcher must validate/copy process buffers and associate tables with process
lifetimes before exposing these operations to Ring 3.

Production-linked host tests and post-ExitBootServices QEMU must exercise file
creation, directory lookup, byte reads/writes, mount selection, stale handles,
path errors, capacity errors without mutation, shared/independent offsets,
descriptor reuse, null/zero devices, pipe ordering/wrap/full/EOF/broken-reader
behavior and close/reuse. Console operations must use the same production VFS,
with a real keyboard write followed by a separate read proving retained state.
Checkboxes wait for observed exact-head CI. USB persistence, TTY, process
integration, shell, credentials and filesystem durability remain separate.
