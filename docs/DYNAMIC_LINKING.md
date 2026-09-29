# Dynamic linking and shared-library ABI policy

This document defines the M24 design boundary for future dynamically linked
Vibrix userspace. It is a design and ABI policy, not a claim that the current
bootstrap userspace has a dynamic linker.

## File format and interpreter contract

Vibrix will use ELF64 dynamic-linking structures rather than invent a parallel
object format. Dynamically linked executables will identify the Vibrix dynamic
loader through `PT_INTERP`. Objects participating in dynamic linking use
`PT_DYNAMIC`, with dependencies expressed through `DT_NEEDED`.

The initial supported relocation set will be architecture-specific and
allow-listed. Unknown relocation kinds, malformed tables, arithmetic overflow,
text relocations, non-canonical addresses or mappings that violate W^X fail
before control reaches application code.

## Search policy

The hardened default search order is deterministic:

1. explicit object dependencies resolved against the installed package database;
2. package-declared runtime library directories;
3. the fixed base-system library directory.

The first implementation will **not** honor ambient host-style
`LD_LIBRARY_PATH`, current-working-directory lookup or implicit relative
directories for privileged programs. Any later environment-controlled search
path must be disabled for set-ID or otherwise privileged transitions.

`DT_RPATH` is not a compatibility target. If `DT_RUNPATH` is supported, it
must be validated as a bounded path list and must not escape the process's
filesystem visibility policy.

## Binding and hardening

The first implementation uses eager relocation before application entry.
Lazy PLT binding is deferred until there is a concrete performance need because
it increases mutable linker state after startup.

Loader mappings follow these phases:

1. map object segments with temporary permissions no broader than required for
   relocation;
2. apply validated relocations;
3. commit final W^X permissions;
4. make relocation metadata read-only where practical;
5. run dependency constructors in a documented order;
6. transfer control to the executable.

Text relocations are rejected. A future RELRO-like milestone may strengthen
post-relocation immutability further, but this design does not mark that
separate M23 item complete.

## Shared-library naming and ABI versioning

Vibrix-owned shared libraries use a logical SONAME carrying the ABI major
version, for example `libvibrix.so.1`.

Policy:

- incompatible ABI changes require a new SONAME major;
- backward-compatible symbol additions retain the major;
- existing symbol meaning, calling convention and object layout are not silently
  changed within one ABI major;
- package metadata records required SONAME majors;
- removal of a still-declared ABI major requires an explicit migration/rebuild
  plan;
- Rust implementation details are not automatically a stable C ABI;
- unstable/private symbols are not exported as compatibility promises.

Fine-grained ELF symbol versioning may be added later, but SONAME-major
coexistence is the first compatibility mechanism.

## Security boundary

The loader treats ELF metadata, dependency names, symbol tables, string tables
and relocations as untrusted input. All offsets, sizes, counts and address
calculations are bounded before use.

Privileged execution must ignore unsafe environment-controlled loader behavior.
The loader must not resolve libraries from writable user-controlled directories
merely because they appear in an environment variable.

Signature trust belongs to the package/repository layer: the dynamic loader
consumes already authorized installed objects rather than implementing a second
independent package-signature policy.

## Compatibility and non-claims

This design completes the M24 **dynamic linker/loader design** and
**shared-library ABI/versioning policy** deliverables only.

It does not claim:

- a dynamic loader binary exists;
- shared objects currently execute on Vibrix;
- `dlopen`/`dlsym`, TLS, auditing or lazy binding;
- libc/POSIX ABI completeness;
- RELRO implementation;
- package signatures or persistent package installation.

## References

Primary format references checked 2026-09-29:

- System V ABI ELF dynamic-linking chapter:
  https://gabi.xinuos.com/elf/09-dynamic.html
- System V ABI program loading and dynamic linking:
  https://gabi.xinuos.com/elf/07-loading-intro.html

The design is Vibrix-specific; no loader implementation source is copied.
