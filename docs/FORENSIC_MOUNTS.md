# Read-only forensic mounting mode

Vibrix can mount any filesystem backend read-only at the VFS namespace
boundary for forensic inspection. This is stronger than relying on the backend
to remember that it should not write: the VFS rejects mutation before the
backend is called.

A read-only mount permits path lookup, metadata, directory enumeration and file
reads. It rejects create, remove, write, truncate, file-flag mutation and
device-buffer mutation with `Error::ReadOnly`.

The production post-firmware VFS self-test mounts a normally writable MemFs at
`/evidence` using this mode, verifies an existing file can be read, and proves
all supported mutation paths fail closed.

This is a mount-policy primitive. It does not yet provide a user-facing mount
command, disk-image parser, write-blocker hardware, evidence hashing, chain of
custody, or persistent USB forensic workflow.
