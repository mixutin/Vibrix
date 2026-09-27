# Vibrix Security Roadmap

> Security is an architecture constraint, not a final milestone.

Vibrix is an experimental operating system and is **not currently production-ready or a security boundary**. This roadmap defines the security properties the project intends to build and verify as the OS gains capability. A checkbox requires implementation plus evidence on the stated target; documentation alone is not completion.

## Security principles

- **Secure by default:** deny or disable capabilities until explicitly enabled.
- **Least privilege:** kernel, drivers, processes and services receive only the access they need.
- **Memory permissions are policy:** writable memory should not be executable; userspace must not access supervisor mappings.
- **Treat inputs as hostile:** firmware tables, executables, filesystems, packages, USB descriptors, network packets and device metadata are untrusted.
- **Unsafe Rust is a boundary:** unsafe code must have explicit invariants and narrow safe interfaces.
- **No silent trust:** dependencies, updates and release artifacts need provenance and integrity controls.
- **Fail closed:** malformed state, failed verification or ambiguous device identity must not silently fall back to unsafe behavior.
- **Physical hardware is different from QEMU:** emulator evidence never implies resistance to malicious hardware or DMA.
- **Recovery matters:** updates and persistent state need rollback/recovery without weakening integrity.
- **Security claims require evidence:** do not advertise a property before it is tested.

## S0 — Repository and supply-chain baseline

- [x] RustSec advisory scanning in CI
- [x] Dependency license/source/bans policy with cargo-deny
- [x] Reject yanked crates and wildcard dependency versions
- [x] Reject unapproved registries and Git sources
- [x] Pin GitHub Actions to immutable commit SHAs
- [x] Dependabot for Cargo and GitHub Actions
- [ ] Commit and enforce Cargo.lock reproducibility
- [ ] Preserve CI failure diagnostics as artifacts
- [ ] Secret scanning and push protection enabled in repository settings
- [ ] Private vulnerability reporting enabled
- [ ] SECURITY.md with supported versions and disclosure process
- [ ] SBOM generated for release artifacts
- [ ] Release hashes and provenance/attestations

**Gate:** dependency and CI execution changes are reviewable, reproducible and auditable.

## S1 — Boot and kernel memory integrity

- [ ] Enforce W^X across kernel mappings
- [ ] Kernel text mapped RX, never writable after initialization
- [ ] Read-only data mapped R+NX
- [ ] Data, heap and stacks mapped RW+NX
- [ ] Supervisor-only kernel mappings
- [ ] CR0.WP verified enabled after transition
- [ ] NX capability detected and required or explicitly degraded
- [ ] Canonical-address and overflow validation at mapping boundaries
- [ ] Guard pages around kernel stacks
- [ ] Dedicated double-fault IST with guard protection
- [ ] Page-table mutation API centralized and invariant-checked
- [ ] No unintended permanent writable physical-memory alias
- [ ] QEMU protection probes verify expected #PF behavior
- [ ] Kernel image integrity/authenticity design documented

**Gate before general userspace:** kernel memory permissions and privilege boundaries are verified, not merely intended.

## S2 — Interrupts, concurrency and CPU state

- [ ] Interrupt handlers use explicit ownership/synchronization rules
- [ ] Unexpected/spurious vectors fail safely
- [ ] IRQ enablement audited against current single-CPU assumptions
- [ ] Per-CPU state replaces unsafe bootstrap globals where required
- [ ] Interrupt stack/IST policy documented and tested
- [ ] Timer and keyboard interrupt stress tests
- [ ] Locking primitives define interrupt/preemption behavior
- [ ] SMP bring-up has explicit memory-ordering model
- [ ] Cross-CPU TLB invalidation designed before shared VM mutation
- [ ] Panic path remains reliable during interrupt faults

**Gate before SMP:** no shared mutable kernel state relies on the old single-CPU/IF=0 bootstrap contract.

## S3 — Userspace isolation and syscall security

- [ ] Ring 3 execution verified
- [ ] User pages cannot access supervisor memory
- [ ] User mappings NX unless executable by policy
- [ ] Syscall entry validates canonical user pointers
- [ ] Copy-in/copy-out validates full ranges and overflow
- [ ] Kernel never directly trusts userspace lengths or structures
- [ ] Per-process address spaces
- [ ] Guard pages for userspace stacks
- [ ] Process/resource limits
- [ ] Handle/capability or descriptor ownership model documented
- [ ] Syscall fuzz/property tests
- [ ] Malformed executable tests
- [ ] Privilege transitions tested under QEMU fault probes

**Gate before third-party applications:** a process cannot directly read/write kernel or another process's memory through supported interfaces.

## S4 — Driver, MMIO and DMA security

- [ ] Device/driver ownership model
- [ ] BAR/MMIO mappings bounded to validated resources
- [ ] MMIO mappings NX and correctly cached
- [ ] Configuration writes require explicit driver ownership
- [ ] DMA buffers have explicit ownership/lifetime rules
- [ ] IOMMU architecture investigated and documented
- [ ] IOMMU enabled by default where supported before untrusted DMA is relied upon
- [ ] Malformed USB descriptor fuzzing
- [ ] USB transfer lengths and DMA addresses checked
- [ ] Driver timeouts prevent permanent hangs
- [ ] Device removal/reconnect does not leave stale pointers
- [ ] Internal disks remain forbidden as Vibrix system/root targets

**Gate before native persistent USB:** storage drivers must not permit unchecked DMA or ambiguous boot-device identity.

## S5 — Filesystem and persistent-state integrity

- [ ] On-disk format validates all lengths, offsets and arithmetic
- [ ] Metadata checksums/integrity scheme
- [ ] Crash-consistent update strategy
- [ ] Atomic critical metadata updates
- [ ] Corrupt filesystem fails safely/read-only where possible
- [ ] Path traversal and name validation
- [ ] Symlink/link semantics cannot escape intended namespaces
- [ ] Permission/ownership model
- [ ] Mount options default to least privilege
- [ ] Fuzz filesystem parser and recovery logic
- [ ] Power-loss simulation tests
- [ ] Recovery tooling does not silently destroy data

**Gate before calling Vibrix persistent:** unexpected power loss or malformed media must not routinely turn into arbitrary memory access or silent destructive repair.

## S6 — Networking security

- [ ] Packet parsers fuzzed for malformed lengths/options
- [ ] Socket ownership and process isolation
- [ ] Resource limits against trivial packet/connection exhaustion
- [ ] TCP state machine hostile-input tests
- [ ] DNS response validation
- [ ] DHCP input validation
- [ ] IPv4/IPv6 fragment/reassembly limits before support is exposed
- [ ] No network-facing service enabled by default without explicit purpose
- [ ] Cryptographic/TLS strategy documented before sensitive network services
- [ ] CSPRNG available before protocols depend on unpredictable values

**Gate before network services:** malformed unauthenticated packets must not cross unchecked into kernel memory operations.

## S7 — Entropy and cryptography

- [ ] Entropy-source architecture
- [ ] CSPRNG with explicit seeding/reseeding rules
- [ ] Hardware RNG treated as an input, not unquestioned sole trust anchor
- [ ] Boot entropy quality reported without leaking secret state
- [ ] Constant-time libraries preferred for secret-dependent crypto
- [ ] Cryptographic primitives come from reviewed implementations, not custom crypto
- [ ] Key material zeroization policy
- [ ] Secret material excluded from logs/crash reports

## S8 — Packages and updates

- [ ] Signed package metadata
- [ ] Signed repository metadata
- [ ] Package hashes verified before installation
- [ ] Update metadata protects against rollback/freeze attacks
- [ ] Transactional install/update behavior
- [ ] Recovery/rollback path
- [ ] Package scripts/hooks minimized or sandboxed
- [ ] Package permissions/capabilities model
- [ ] Repository key rotation/revocation design
- [ ] Offline verification supported
- [ ] Dependency provenance recorded in package metadata

**Gate before vpm is considered usable:** an attacker controlling a mirror must not be able to silently install arbitrary unsigned packages.

## S9 — Verified boot and release integrity

- [ ] Threat model for stolen/modified USB media
- [ ] UEFI Secure Boot signing path
- [ ] Loader verifies kernel authenticity
- [ ] Kernel verifies security-critical system artifacts or trusted root metadata
- [ ] Measured-boot/TPM option investigated
- [ ] Reproducible release builds
- [ ] Published SHA-256 hashes
- [ ] SBOM attached to releases
- [ ] Build provenance/attestation
- [ ] Signing-key storage and rotation procedure
- [ ] Recovery image/update trust path documented

## S10 — Security testing and response

- [ ] Parser fuzz targets: ELF, BootInfo, ACPI, MCFG, GPT
- [ ] Later fuzz targets: USB, filesystem, network, executable loader, package metadata
- [ ] Nightly QEMU resource/CPU matrix
- [ ] Low-memory boot tests
- [ ] Device-absence tests
- [ ] Fault-injection tests
- [ ] Unsafe-boundary CI inventory
- [ ] Security regression tests for every fixed vulnerability
- [ ] Threat model maintained per major subsystem
- [ ] Private vulnerability disclosure process
- [ ] Security advisory/release procedure
- [ ] Periodic manual unsafe-code review

## Security maturity labels

Vibrix documentation and releases should use conservative language:

- **Experimental:** boots/tests exist; no security guarantee.
- **Isolated prototype:** basic memory/privilege boundaries verified in QEMU.
- **Security-testing preview:** fuzzing, update integrity and persistent-state protections are active.
- **Hardened preview:** verified boot/release integrity and major subsystem threat models are in place.

Do not call Vibrix “secure,” “hardened,” or suitable for protecting sensitive data solely because Rust is used or individual roadmap items are checked.

## Current priority

The immediate security sequence is:

1. finish the S0 reproducibility/CI baseline;
2. implement and verify kernel W^X and supervisor/NX page policy;
3. add stack guard pages and fault probes;
4. audit unsafe/concurrency assumptions before enabling hardware IRQs;
5. fuzz existing hostile-input parsers;
6. require userspace isolation gates before third-party applications.

Security roadmap changes should stay synchronized with [ROADMAP.md](ROADMAP.md), [docs/UNSAFE_BOUNDARIES.md](docs/UNSAFE_BOUNDARIES.md), [docs/DEPENDENCIES.md](docs/DEPENDENCIES.md) and architecture decisions.
