# Immutable and append-only file flags

The bootstrap VFS exposes two backend-enforced file flags:

- **immutable** rejects regular-file writes, truncation and unlink;
- **append-only** permits writes only at the current end of file and rejects
  truncation and unlink.

The flags live in the filesystem backend, not in an open descriptor, so opening
a second descriptor cannot bypass them. Unknown flag bits are rejected. The
production-linked VFS self-test sets each flag on a real RAM-backed node and
requires the mutation paths to fail before clearing the flag and continuing the
normal descriptor proof.

The current API is kernel-internal and volatile. It does not yet persist flags
in VibrixFS, expose chflags-style userspace commands, define securelevel policy,
or prevent a sufficiently privileged kernel caller from explicitly clearing a
flag. Those are separate persistent-security integrations.
