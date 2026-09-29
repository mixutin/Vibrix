# ADR 0023: optional encrypted Vibrix USB system volume

- **Status:** Accepted
- **Date:** 2026-09-29
- **Authoring AI:** GPT-5.6 Sol
- **Roadmap:** M11 — optional USB system encryption design
- **Supersedes:** None
- **Compatibility:** Design only. No current on-disk VibrixFS, boot, syscall, USB, recovery, or key-management ABI changes.

## Decision

Vibrix will treat portable-system encryption as an **optional authenticated
volume layer beneath VibrixFS**, not as ad-hoc per-file crypto and not as a
filesystem-format rewrite.

The unencrypted EFI System Partition remains readable by firmware. The Vibrix
system partition may contain an encrypted volume whose plaintext block device
is exposed to VibrixFS only after an explicit unlock succeeds.

This ADR fixes the design boundary and cryptographic construction choices. It
does **not** claim that encryption, secure random, key storage, unlock UI, USB
mass storage, writable VibrixFS, or recovery integration exists today.

## Threat model

The optional layer is intended to protect confidentiality and detect
unauthorized modification when a removable Vibrix USB is lost, stolen, copied,
or examined while powered off.

It does not claim protection against:

- a running compromised kernel;
- DMA before IOMMU isolation exists;
- evil-maid replacement of the unencrypted EFI bootloader;
- hardware keyloggers;
- forensic recovery from unrelated host memory;
- full-volume rollback by an attacker who can replay every encrypted byte and
  header together, unless a future external monotonic/trusted anchor is added.

The final limitation is fundamental for a self-contained removable device: an
offline attacker who can restore an older complete image can also restore the
older authenticated root. The format therefore detects partial tampering and
mix-and-match replay but does not promise externally anchored freshness.

## Key hierarchy

Each encrypted volume has one random 256-bit **volume master key (VMK)**.
The VMK is never derived directly from a passphrase.

A passphrase keyslot contains:

- a unique random 128-bit-or-larger Argon2 salt;
- explicit Argon2id version and tunable memory/time/parallelism parameters;
- an AES key-wrap ciphertext containing the VMK;
- an authenticated keyslot identifier and format version.

The passphrase derives a key-encryption key with **Argon2id version 1.3**.
Parameters are stored per keyslot so Vibrix can increase cost for newly created
or rewrapped slots without silently changing old slots. Creation should choose
the largest practical memory/time cost for the target machine, with RFC 9106's
Argon2id recommendations as the starting point rather than a permanently
hard-coded desktop/server value.

The VMK expands through **HKDF-SHA-256** with the volume UUID and distinct
context labels into separate keys for:

- data-block authenticated encryption;
- metadata/authentication-tree protection;
- header/keyslot authentication.

No derived key is reused for a different purpose.

Multiple independent keyslots may wrap the same VMK, allowing passphrase
rotation without re-encrypting the entire volume. Removing a keyslot revokes
that unlock method, subject to flash-media remanence limitations.

## Block encryption

The plaintext interface uses 4096-byte logical blocks.

Each allocated plaintext block is encrypted independently with
**AES-256-GCM-SIV** and a 128-bit authentication tag. The 96-bit AEAD nonce is
deterministically derived from the immutable volume identity, logical block
number and encryption-generation domain. Associated data binds at least:

- encryption format version;
- volume UUID;
- logical block number;
- block role/domain;
- generation/domain identifier.

AES-GCM-SIV was selected because removable-media crash/replay bugs make perfect
nonce-state guarantees difficult, and RFC 8452 is specifically designed to
avoid catastrophic confidentiality/integrity failure under accidental nonce
reuse. This is defense in depth, not permission to intentionally recycle
nonces without analysis.

Authentication tags are stored outside the encrypted 4096-byte payload so the
plaintext block-device geometry remains stable for VibrixFS.

## Integrity tree and metadata

Tags and encryption metadata are themselves authenticated. The format reserves
a bounded metadata region containing:

1. two independently checksummed/authenticated encryption headers;
2. keyslots;
3. per-data-block authentication tags;
4. an authenticated tree over tag/metadata pages;
5. two alternating authenticated root records with generation numbers.

The tree root authenticates every tag page and the immutable volume geometry.
A block read is released to VibrixFS only after its tag and the path to the
selected root validate.

A malicious splice of blocks/tags from another volume or logical position fails
because volume UUID and block number are associated data.

As noted in the threat model, replaying a complete older header/root/data image
cannot be distinguished without a freshness anchor outside the removable
device.

## Crash consistency

Encryption metadata participates in the same fail-closed philosophy as
VibrixFS journaling:

1. write new ciphertext and tag/tree pages;
2. flush them through the block layer;
3. write the inactive authenticated root record with the next generation;
4. flush;
5. make that root the newest valid generation.

On mount, Vibrix accepts the highest valid self-consistent root generation.
A newer root that authenticates missing/corrupt child state fails closed rather
than silently selecting unauthenticated plaintext.

Exact persistence ordering cannot be considered implemented until the native
USB mass-storage/block path has real flush/barrier semantics.

## Header and unlock policy

The encryption header is versioned independently of VibrixFS. It records only
non-secret geometry, algorithms, KDF parameters, salts, wrapped VMKs and
authenticated format metadata.

Unlock is explicit. The boot/runtime flow must never fall back from a failed
encrypted-volume authentication attempt to treating those bytes as plaintext
VibrixFS.

Passphrases are handled in mutable memory that is deliberately wiped after KDF
and unwrap processing. The eventual implementation must audit compiler/runtime
behavior so "wipe" is not merely optimized away.

Recovery tooling must be able to:

- inspect header/keyslot metadata without a passphrase;
- validate redundant header structure;
- unlock only when explicitly supplied credentials;
- verify the authentication tree;
- never auto-select an internal disk as a repair/write target.

## Randomness dependencies

Creation requires a cryptographically secure random source for:

- VMK generation;
- volume UUID;
- Argon2 salts;
- any randomized keyslot identifiers/domains.

Therefore encryption **cannot ship enabled** before the separate M11
`secure random` item is implemented and tested. Timing, MAC addresses, disk
serials, UEFI memory-map values, RDRAND alone without a health/mixing policy, or
similar identifiers are not substitutes for the secure-random subsystem.

## Algorithm agility

The on-disk header stores numeric algorithm/KDF identifiers and rejects unknown
required algorithms. Vibrix does not silently reinterpret an existing
identifier.

Changing the data AEAD requires a new format/algorithm identifier and migration
tooling. Algorithm agility is for controlled upgrades, not unauthenticated
runtime negotiation.

## Primary sources reviewed

Reviewed 2026-09-29:

- RFC 9106, Argon2 Memory-Hard Function for Password Hashing and Proof-of-Work:
  https://www.rfc-editor.org/rfc/rfc9106
- RFC 5869, HMAC-based Extract-and-Expand Key Derivation Function (HKDF):
  https://www.rfc-editor.org/rfc/rfc5869
- RFC 8452, AES-GCM-SIV: Nonce Misuse-Resistant Authenticated Encryption:
  https://www.rfc-editor.org/rfc/rfc8452
- NIST SP 800-38F, Methods for Key Wrapping:
  https://csrc.nist.gov/pubs/sp/800/38/f/final

RFC 9106 recommends Argon2id and gives parameter-selection guidance, including
a memory-constrained recommended profile. RFC 8452 defines AES-GCM-SIV as a
nonce-misuse-resistant AEAD. RFC 5869 defines extract-and-expand key
separation. SP 800-38F defines AES Key Wrap/AES Key Wrap with Padding for
protecting cryptographic keys.

## Consequences

### Positive

- VibrixFS remains unaware of cryptographic sectors.
- Passphrase changes can rewrap a VMK without rewriting all file data.
- Confidentiality and integrity are coupled rather than using unauthenticated
  disk encryption.
- Per-block authentication allows bounded verification and localized I/O.
- Crash ordering and replay limits are explicit before implementation begins.

### Costs and open implementation work

- The tag/tree region consumes additional capacity and causes write
  amplification.
- Unlock requires Argon2 memory before mounting the encrypted filesystem.
- AES-GCM-SIV, HKDF, Argon2id and AES key wrap need audited implementations or
  carefully reviewed dependencies and test vectors.
- Native USB storage flush semantics, secure random and secret-memory handling
  are prerequisites.
- Boot-chain authentication is separate; leaving the ESP plaintext preserves an
  evil-maid attack unless secure boot/measured boot is added later.

## Roadmap boundary

This ADR completes **optional USB system encryption design** only.

It does **not** complete secure random, IOMMU, secure updates, persistent USB
root, USB mass storage, writable encrypted blocks, key management UX, recovery
unlock support, boot-chain authentication, or any claim that Target 001 is
currently encrypted.
