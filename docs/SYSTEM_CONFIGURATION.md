# Coherent /etc-style system configuration

Vibrix's native userspace bootstrap now exposes a bounded system configuration
file at `/etc/vibrix.conf` and a `config` shell command.

`config apply KEY=VALUE...` validates the entire requested assignment set,
writes a complete replacement to `/etc/.vibrix.conf.new`, closes it
successfully, then invokes the additive rename syscall. The kernel validates
both user paths and performs a same-filesystem atomic namespace replacement
through the VFS. If validation, writing, closing, or rename fails, the existing
`/etc/vibrix.conf` remains the committed configuration.

`config show` reads the committed file through the normal userspace VFS path.
Keys are restricted to ASCII letters, digits, underscore, dot and dash; values
are printable ASCII. This keeps the bootstrap parser deterministic and prevents
newline injection into the line-oriented file.

The same rename ABI upgrades `mv` from copy/remove to an atomic
same-filesystem namespace operation. Cross-filesystem rename is rejected rather
than silently losing atomicity.

## Evidence boundary

Host shell tests exercise successful commit, invalid-input preservation and
temporary-file cleanup. The normal native userspace CLI/RFB evidence types
`config apply`, `config show`, an invalid update, and an atomic `mv` through
the real Ring-3 shell and syscall path.

This satisfies the coherent atomic-update mechanism for the current `/etc`
configuration namespace. The bootstrap root remains volatile RAM until the
persistent USB-root milestone supplies the same VFS contract on durable
storage; this change does not claim power-loss durability or persistent
configuration across reboot.
