# Partition-bounded block views

Authoring model: **GPT-6 Astra Pro**.

`BlockDevice::partition(first_lba, sectors)` creates an allocation-free,
exclusively borrowed `BlockDevice<Partition<T>>` over a nonempty subset of an
already validated device. LBAs supplied to the view are relative to that subset.
The logical sector size, read-only restriction and durability declaration are
inherited. Views can be nested without widening their parent's accessible range.

Both the view and its transport validate whole-sector, nonempty, bounded I/O.
The translation uses checked arithmetic. A request crossing either end is
rejected before the parent transport runs. Releasing the view makes the parent
available again; no shared mutable device handle or unsafe code is introduced.
An underlying I/O error remains an error and may represent partial device I/O.

Flush delegates to the parent device and remains a device-wide barrier; it does
not promise partition-local flushing, atomic writes, or stronger durability.
RAM stays volatile and reports `UnsupportedFlush`. Constructing a view does not
perform I/O, authorize physical writes, identify removable media or select root.

## Validation

The existing production `block::self_test` now calls the partition proof before
its established QEMU success marker. It writes and reads two middle sectors and
checks sentinel sectors on both sides, rejection of a crossing request, and
volatile-flush rejection. Existing canonical and block-specific CI compile the
same module and tests; no new workflow or passing-result shortcut is added.

Six added host tests cover the production proof, 4096-byte sectors and nested
views, zero/overflowing/out-of-device extents, rejected-I/O non-dispatch,
inherited read-only policy, and translated transport/flush errors.

## Roadmap boundary and research

This advances the M7 block/storage integration foundation, not GPT discovery,
USB BOT/SCSI, a mounted filesystem, persistence, or physical Target 001 support.
No roadmap checkbox changes. See [the block contract](BLOCK_DEVICE.md).

Primary references checked on 2026-09-29: the Rust core `u64` documentation
(`checked_add`, `checked_mul`, `is_multiple_of`) and The Rust Programming Language,
chapter 4.2 (exclusive mutable borrowing). The implementation is first-party safe
Rust extending Vibrix's existing transport, with no copied OS code or new crate.
A separate partition-library dependency was considered unnecessary for this
small adapter: no table format is parsed and the existing geometry contract is
reused rather than reimplemented.
