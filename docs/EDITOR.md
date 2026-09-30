# Native editor core

`vibrix-editor` is the bounded editing engine for future native developer
tooling. It is `no_std`, allocation-free and compiles for
`x86_64-unknown-none`.

The editor owns one 4096-byte buffer, a byte cursor and a dirty flag. Loading,
insertion and cursor movement validate bounds before mutation. Capacity failure
does not partially change the document. Terminal rendering, Unicode grapheme
navigation, VFS open/save, undo, search, syntax highlighting and multi-file
editing are intentionally separate layers.

This advances M14 **editor/developer tooling**, but does not mark the item
complete until the core is wired to native Ring 3 TTY/VFS I/O and exercised in
the actual Vibrix userspace environment.
