# Package and application capability declarations

Vibrix package metadata keeps its existing `VPKGv001` manifest unchanged.
Capability declarations use a separate fixed-size sidecar record,
`VCAPv001`, so old package metadata retains its fail-closed reserved-byte
contract.

A declaration binds one canonical package name to a typed authority bitset.
The initial vocabulary is deliberately small:

- filesystem read,
- filesystem write,
- network access,
- device access,
- process control.

A zero-bit declaration is valid and explicitly requests no ambient capability.
Unknown capability bits, malformed package names, wrong magic/length and nonzero
reserved fields are rejected. This prevents a future capability from being
silently interpreted as harmless by an older decoder.

The declaration is metadata only. Installation permission review, sandbox
enforcement, filesystem/network namespaces and device mediation remain separate
M19 work. The package database does not grant authority merely because a
declaration exists.

Exact-head evidence covers host round trips and malformed records plus the
native no_std Ring-3 package probe, which encodes and decodes the same production
`CapabilityDeclaration` type.
