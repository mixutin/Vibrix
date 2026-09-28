# ADR 0011: VibrixFS v1 crash consistency

- **Status:** Accepted
- **Date:** 2026-09-28
- **Roadmap:** M8 — Vibrix filesystem, crash-consistency design
- **Supersedes:** None

## Context

VibrixFS v1 already defines checksummed superblocks, allocation bitmaps, fixed
inodes and directory records, but the base format intentionally forbids a
writable mount after an unclean shutdown. A removable USB root must survive
power loss without turning partially written metadata into trusted allocation
ownership or directory/inode pointers.

USB flash and USB-attached SSDs do not guarantee that a 4096-byte filesystem
block is power-fail atomic. A design therefore cannot treat one block write,
cache flush, or checksum as an atomic transaction. The future USB mass-storage
path must also expose a real durability barrier (for example SCSI SYNCHRONIZE
CACHE or a transport-equivalent guarantee); acknowledging a write in RAM is not
enough.

## Decision

Adopt a **bounded full-block redo journal with ordered data writes** for the
first writable VibrixFS implementation.

### Transaction model

1. Only one metadata transaction is active at a time in v1. This deliberately
   avoids concurrent journal ownership before the VFS/locking model exists.
2. A transaction has monotonically increasing `previous_generation` and
   `new_generation`, plus a bounded list of destination filesystem block
   numbers.
3. Each journal entry stores one complete 4096-byte **after-image** of a
   metadata block and identifies its eventual home block. Entries carry their
   own checksum; the transaction manifest binds the ordered target-block list
   and entry checksums.
4. A final commit block records the transaction id/generations, entry count and
   manifest checksum. A transaction is committed only when every entry and the
   commit block validate.
5. v1 initially uses a fixed journal region and a fixed maximum transaction
   size. Writers must split a larger operation at an explicitly safe semantic
   boundary or refuse it; they must never silently exceed the journal.

### Write ordering

For a metadata-changing operation:

1. Allocate any new data blocks in memory without making them reachable from
   committed metadata.
2. Write new file-data blocks first.
3. Issue a durability barrier.
4. Write all redo entry blocks and their manifest.
5. Issue a durability barrier.
6. Write the commit block.
7. Issue a durability barrier. Only after this barrier is the transaction
   durable.
8. Copy the committed after-images to their home metadata blocks.
9. Issue a durability barrier.
10. Checkpoint the secondary superblock at `new_generation`, flush it, then
    checkpoint the primary superblock and flush it.
11. Only after both valid superblocks agree on the checkpoint generation may
    the journal slot be retired/reused and blocks freed by the transaction
    become available for unrelated allocation.

This ordering ensures a crash before the commit leaves no transaction to
replay, while a crash after the commit leaves a durable source from which any
torn or missing home metadata block can be restored.

### Data and allocation rules

- Newly allocated blocks are not referenced by committed inodes/directories
  until the journal commit is durable.
- Blocks removed from a file or metadata object are not reused until the
  transaction is checkpointed and the journal copy is no longer required.
- Size/extent replacement uses newly written blocks before the metadata switch.
  Ordinary in-place file-data writes may still lose or tear the affected file
  contents on power failure; they must not corrupt filesystem metadata.
- Metadata blocks are never trusted solely because their CRC is valid. Their
  bounds, type, allocation ownership and transaction generation are still
  validated through the normal VibrixFS rules.

### Superblocks and mount recovery

The two superblocks remain independent checkpoints. During recovery:

1. Validate each copy independently before using any internal offset.
2. If both are valid and immutable geometry/identity fields disagree, refuse
   automatic recovery.
3. Use the highest valid checkpoint generation as the starting point; a lower
   valid copy is stale, not automatically corrupt.
4. Scan only the externally bounded journal region. Ignore incomplete
   transactions with no valid commit block.
5. For a valid committed transaction whose
   `previous_generation <= checkpoint_generation < new_generation`, validate
   every redo entry and replay all after-images idempotently.
6. Flush the replayed home metadata, then checkpoint secondary and primary
   superblocks in that order.
7. If a commit is valid but any required journal entry is torn, out of bounds,
   duplicated, targets the journal itself, or fails checksum/manifest
   validation, **fail closed** and require the recovery tool. Never partially
   replay the remaining entries.

A clean shutdown first checkpoints every committed transaction and retires the
journal, then writes the clean-state superblocks using the same secondary-then-
primary ordering. A dirty flag is diagnostic/recovery input; it never overrides
checksums or generation rules.

### Durability boundary

A writable kernel mount is forbidden until the native block layer and USB
mass-storage implementation can issue and verify an ordering/durability barrier
appropriate to the device. If a device reports no usable cache-flush semantics,
VibrixFS must stay read-only rather than pretending the journal is durable.

## Format compatibility

The journal activation will require an explicit VibrixFS feature bit and
nonzero `journal_start`/`journal_blocks`. Older base-v1 readers already
reject unknown feature words and therefore fail closed. The implementation PR
must define the exact record bytes/checksums and choose the feature-word class
before it writes such an image; this ADR does **not** silently reinterpret the
currently reserved bytes.

The existing host formatter remains base-v1 and journal-free until that record
codec and recovery implementation are tested.

## Alternatives considered

- **Rely on dual superblocks only:** rejected; they cannot repair torn inode,
  bitmap or directory home blocks.
- **Assume 4 KiB sector atomicity:** rejected; removable media and USB bridges
  do not provide a universal power-fail guarantee at the filesystem-block size.
- **Metadata copy-on-write tree:** attractive long term but much larger than
  the first bounded filesystem and recovery tool.
- **Journal only changed byte ranges:** rejected for v1 because replay and torn
  write validation become more complex than full-block after-images.
- **Write-back without explicit flushes:** rejected because completion at the
  host/controller boundary is not evidence of durable ordering.

## Safety and failure model

All journal block numbers, counts, generations, byte arithmetic and target
ranges are untrusted on mount and use checked arithmetic before I/O. Journal
targets may never include the journal region or blocks outside the selected
VibrixFS partition. Replay is idempotent and must not allocate from metadata it
has not yet validated.

This design targets filesystem structural consistency after interruption; it
does not promise preservation of every recently written file byte. It also does
not defend against malicious media, DMA attacks or a lying storage device.

## Validation required before writable use

Implementation remains incomplete until first-party tooling and then the kernel
exercise at least:

- power loss before entry flush, before commit flush, during home replay and
  between the two superblock checkpoints;
- torn/corrupt entry, manifest and commit blocks;
- duplicate/out-of-range/home-to-journal target rejection;
- replay from either stale superblock copy;
- allocator non-reuse of blocks still protected by a committed transaction;
- real block-layer flush/error propagation.

The M8 **crash-consistency design** checkbox records adoption of this ordering
and recovery contract only. Formatter + recovery tool, VFS driver, persistent
root and real USB power-loss behavior remain separate unchecked milestones.

## References

- [VibrixFS v1 wire specification](../VIBRIXFS.md)
- [ADR 0010 — VibrixFS v1 on-disk format](0010-vibrixfs-v1-ondisk.md)
- [Vibrix USB system model](../USB_MODEL.md)

No external operating-system filesystem implementation source was used.

## Initial journal wire encoding checkpoint (implementation candidate)

The first-party host conformance codec proposes a concrete fixed-block encoding
for the bounded redo transaction described above. This section records the
implementation under test; it does **not** enable the superblock journal feature
or writable mounts yet.

The journal region begins with one 4096-byte manifest block. Its header contains
an 8-byte `VJMANF01` magic, wire version 1, 64-byte header size, entry count,
nonzero transaction id, previous/new generations, full-block CRC-32, and the
journal start/length. Starting at byte 64, at most 64 descriptors each contain
the destination home block, CRC-32 of the corresponding complete 4096-byte
after-image, and zero reserved bytes. Unused manifest bytes are zero.

After the manifest, slots 1 through entry_count contain the **exact 4096-byte
metadata after-images**. The next slot is a 4096-byte commit block with
`VJCOMT01` magic, the same transaction/generation/count tuple, the manifest
CRC, and its own whole-block CRC. Remaining journal slots are outside that
transaction.

Before replay is permitted, the validator independently checks both checksums,
exact generation increment, journal geometry, payload count and CRCs, commit
binding, nonzero/in-range home blocks, rejection of superblock targets, no
home target inside the journal, and no duplicate targets. Any failure rejects
the transaction as a unit.

This encoding is still **host-only conformance groundwork**. The superblock
feature bit/journal geometry activation, actual image journal writes,
durability barriers, interrupted-write simulation and recovery replay remain
separate implementation work. Exact-head CI is required before this proposed
encoding can be treated as tested groundwork.
