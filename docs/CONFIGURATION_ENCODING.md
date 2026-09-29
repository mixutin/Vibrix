# Canonical portable configuration encoding

Authoring model: **GPT-6 Astra Pro**.

`PortableConfig::write_text(&mut output)` completes the in-memory round trip for
[ADR 0014](decisions/0014-portable-configuration.md)'s existing schema. It emits
version 1 and every supported preference in fixed order: hostname, keymap,
network and log level. Output is ASCII with LF line endings and a final newline;
the returned byte length excludes any NUL terminator. Bytes after that prefix
are untouched. Comments and formatting are deliberately not preserved.

The encoder revalidates the public hostname field before doing anything to the
output. Typed enum values are converted to the existing schema's literals.
Length and capacity are checked before copying any bytes. `Error::Value`,
`Error::TooLarge` and `Error::OutputTooSmall` leave the entire output unchanged.
Callers must use the returned length, not write the whole scratch buffer.

A saved DHCP preference still is not current-boot consent. Decoding an encoded
configuration does not alter `network_for_boot(false)`, select a disk, activate
a device or restore hardware identifiers. There are no credentials or extra
schema keys. Serialization is not persistence: a future writer still needs
validated removable-root identity, safe filesystem transactions, ordered flush
and recovery. This module performs no file or block I/O.

## Validation

Six added host tests cover the production round trip, canonical/exact-capacity
output, all twelve enum combinations, every shorter output length, invalid
manually constructed hostnames, the 63-byte maximum and canonical idempotence.
The existing configuration policy self-test now executes the encoder and parser
inside the QEMU kernel, including a rejected short buffer and retained network
consent gate, before its existing required COM1/debugcon success marker.

Existing standalone Rust tests and canonical CI include this production module.
No dependencies, unsafe Rust, wire-format changes or roadmap checkbox changes
are introduced. Exact-head build/QEMU evidence is recorded in the PR.

## Research and design

Checked 2026-09-29: ADR 0014 and the existing production parser are authoritative
for this repository-owned schema. Rust core slice copying, integer checks and
exclusive borrowing provide the implementation mechanisms. A general-purpose
serialization library was considered unnecessary for five fixed scalar fields;
that would add schema/feature/dependency surface without providing file durability.
The encoder emits only the literals already accepted by the parser, and the
independent canonical-byte fixture guards against matching encoder/decoder bugs.
