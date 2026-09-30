# Atomic VFS rename foundation

The bootstrap VFS now has an explicit same-filesystem rename contract. Backends
must either provide an indivisible namespace update or return `Unsupported`;
the VFS does not emulate rename with copy/remove.

The in-memory backend validates both parents, source/destination type
compatibility, immutable/append-only flags, non-empty directory replacement,
and directory-cycle safety before any namespace mutation. Replacing an existing
entry invalidates only the replaced inode generation; the moved inode keeps its
identity and contents. Mount roots cannot be renamed and cross-filesystem moves
fail rather than becoming non-atomic copies.

The descriptor owner additionally refuses replacement of an open destination,
avoiding stale open-description semantics in the current early file model.

This is foundation work for M21 atomic configuration updates and for a future
real rename syscall. It does not by itself check a ROADMAP item: there is not yet
a userspace rename ABI or persistent-filesystem implementation.
