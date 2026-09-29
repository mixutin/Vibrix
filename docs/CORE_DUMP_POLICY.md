# Core-dump secret-exclusion policy

Vibrix's hardened profile treats process memory as potentially secret-bearing.
The current system has **no userspace core-dump facility**, which is the
fail-closed default: a crash cannot silently serialize process memory to disk.

This document defines the policy required before any future core-dump
implementation may be enabled.

## Default policy

- Core dumps are disabled unless the operator explicitly enables a supported
  diagnostic mode.
- Privileged, set-ID, authentication, key-management and security-service
  processes are never dumpable by default.
- A crash path must not write to internal disks; any persistent dump target must
  be an explicitly selected Vibrix-owned removable-system location.
- Failure to classify or redact a region fails closed by omitting that region or
  the entire dump.

## Material that must be excluded

A future dump writer must exclude at minimum:

- credential/authentication secrets and password-derived material;
- cryptographic keys, entropy-pool/internal RNG state and volume unlock
  material;
- environment entries or argument regions classified as secret-bearing;
- kernel memory and kernel pointers;
- device DMA buffers containing credentials or encrypted-volume key material;
- memory explicitly marked non-dumpable by the process or runtime;
- other processes' pages;
- stale/free pages not owned by the crashing process.

The exclusion decision is based on kernel-owned mapping metadata, not pattern
matching after a raw memory dump has already been produced.

## Metadata and diagnostics

Permitted metadata may include bounded process identity, exit/fault reason,
architecture, build identifier and sanitized mapping ranges. Normal hardened
reports must not expose raw kernel addresses.

Core-dump creation is a security-sensitive event and, once the audit facility
exists, must generate an audit record without embedding the excluded secret
content itself.

## Encryption and access

If persistent core dumps are later supported, they must inherit the system's
encrypted-storage policy when encryption is enabled. File permissions alone are
not a substitute for excluding secrets.

The operator must be able to delete dumps without affecting unrelated system
state, and retention must be bounded to avoid flash exhaustion.

## Validation gate for implementation

Core dumps remain disabled until tests prove:

1. secret/non-dumpable mappings never appear in the output;
2. cross-process and kernel pages cannot be selected;
3. malformed mapping metadata fails closed;
4. interrupted writes do not expose a partially published dump as valid;
5. privileged-process default denial works;
6. normal support bundles do not silently attach core dumps.

This policy completes only the M23 **core-dump policy that excludes secret
material** item. It does not claim a core-dump writer, debugger integration,
persistent journal, encryption implementation or audit log.
