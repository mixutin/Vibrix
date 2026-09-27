# ADR 0010: VibrixFS v1 on-disk format

- **Status:** Proposed
- **Date:** 2026-09-27
- **Roadmap:** M8 — Vibrix filesystem, on-disk specification
- **Supersedes:** None

## Context

Vibrix needs a persistent root filesystem that lives on the same removable USB
system disk from boot through normal operation. The format must be implementable
by the Rust-native kernel and host tooling without importing another operating
system's filesystem implementation, remain understandable enough for recovery
tools, and tolerate USB media with 512-byte or 4096-byte logical sectors.

The existing GPT tooling creates an unformatted generic data partition. No
Vibrix filesystem magic, superblock, inode, directory or allocation format is
currently adopted. ADR 0005 separately defines how the boot USB and its GPT
partition identities are reacquired; this ADR defines filesystem bytes inside a
selected root partition and does not change BootInfo, GPT identity or USB
transport.

## Decision

Adopt a versioned little-endian format named **VibrixFS v1** with these
fundamental rules:

1. The filesystem uses a fixed **4096-byte filesystem block**, independent of
   the underlying 512/4096-byte logical sector size. A partition start used for
   VibrixFS must be 4096-byte aligned.
2. The first block contains the primary superblock. The final filesystem block
   contains a secondary superblock copy. Both copies include generation and
   checksum fields; selection/recovery policy is a later crash-consistency
   decision and no implementation may silently choose a corrupt copy.
3. On-disk integers are explicitly little-endian. Structures have fixed wire
   sizes, byte offsets and zero-reserved fields. Rust in-memory structs are not
   written with raw structure casts.
4. Inode number 1 is the root directory. Inode 0 is permanently invalid.
   Inodes are fixed-size records in a bounded inode-table region; file data is
   represented by sorted, non-overlapping extents.
5. Allocation metadata uses block and inode bitmaps. A set bit means allocated.
   Superblock, bitmap, inode-table and secondary-superblock blocks are always
   reserved and must never appear as file extents.
6. Directories store self-describing variable-length records containing inode,
   type and UTF-8 name bytes. Names prohibit NUL and slash and are limited to
   255 bytes. Dot and dot-dot are represented as ordinary directory records
   but are validated specially.
7. Timestamps are signed Unix seconds plus unsigned nanoseconds. v1 records
   access, modification and metadata-change timestamps. Nanoseconds must be
   less than 1,000,000,000.
8. Permissions use the low 12 traditional Unix mode bits plus an explicit file
   type field; uid/gid are unsigned 32-bit values. Interpretation by the future
   credentials subsystem is separate.
9. Every superblock and inode record has a checksum over a canonical byte range
   with the checksum field zeroed. v1 uses IEEE reflected CRC-32, matching the
   project's existing first-party GPT tooling; CRC is corruption detection, not
   authentication.
10. Unknown incompatible feature bits cause mount refusal. Unknown compatible
    feature bits may be ignored only when the bit definition explicitly says so.
    v1 writers must keep all reserved bytes zero.
11. The format reserves transaction/journal feature fields but **does not yet
    define crash-consistency ordering or a journal record format**. Until that
    later M8 decision is implemented and proven, the filesystem must not be
    mounted writable after an unclean shutdown.
12. The root filesystem partition is chosen only after ADR 0005's boot-USB/GPT
    identity checks. A mountable VibrixFS found on an internal disk is never an
    automatic substitute for the configured removable root.

The exact byte layout and validation algorithm are normative in
docs/VIBRIXFS.md.

## Alternatives considered

- **FAT as the root filesystem:** convenient for firmware but lacks the native
  Unix metadata and evolution model Vibrix needs. FAT remains appropriate for
  the EFI System Partition.
- **Adopt ext4/UFS/ZFS/Btrfs:** mature formats, but implementing or importing
  their complete semantics would add complexity and conflict with the project's
  goal of an understandable Vibrix-owned persistent-system format.
- **Write Rust structs directly to disk:** rejected because compiler layout,
  padding, endianness and future field changes are not a stable wire format.
- **Variable filesystem block sizes in v1:** rejected to keep allocation,
  mapping and recovery logic small. A future incompatible version can revisit
  this if real hardware evidence requires it.
- **Copy-on-write everything in v1:** attractive for consistency but much
  larger in scope than the first persistent USB root. Crash consistency is
  intentionally a separate reviewed M8 decision.

## Consequences and compatibility

VibrixFS v1 is an incompatible, explicitly versioned format. Future readers
must reject an unsupported major version and feature set. The primary and
secondary superblock locations make gross metadata loss detectable, but do not
by themselves make updates atomic. The fixed 4 KiB block aligns naturally with
the kernel's current page size and both supported GPT image sector sizes.

The current GPT image creator is **not** changed by this ADR and still labels
its second partition as generic unformatted data. A later partition-layout and
formatter PR must deliberately adopt a Vibrix root partition contract; it must
not make existing blank images appear formatted.

## Validation plan and evidence

This ADR alone is design evidence, **not completion of the M8 checkbox**.
Implementation must add a first-party wire encoder/decoder independent of
in-memory Rust layout, golden byte fixtures, malformed/overflow/checksum tests,
and a formatter/inspector round trip for 512- and 4096-byte logical-sector
images before the on-disk-spec item is checked.

Later writable validation must additionally exercise the adopted
crash-consistency design and recovery tooling, including interrupted metadata
updates. Persistent-root completion requires native USB block I/O and reboot
persistence on the stated QEMU/physical targets.

## References

- Vibrix USB system model
- ADR 0005 — persistent boot USB identity
- UEFI GPT and removable-media specifications already referenced by the project

No external filesystem implementation source was used.
