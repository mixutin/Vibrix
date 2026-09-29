# USB mass-storage and SCSI foundation

Vibrix targets USB Mass Storage Class Bulk-Only Transport (BOT) for the first
native removable-storage path.

The production wire contracts live in:

- `shared/usb_mass_bulk.rs` for the 31-byte Command Block Wrapper (CBW) and
  exact 13-byte Command Status Wrapper (CSW);
- `shared/scsi.rs` for the bounded SCSI transparent command subset used by
  block devices.

## BOT boundary

The BOT layer encodes the required CBW signature, caller tag, transfer length,
direction, LUN and 1–16 byte command block. CSW parsing requires the BOT
signature, the exact matching tag, residue no larger than the requested
transfer length, and one of the three defined status values.

The future xHCI transport must preserve the BOT ordering:

1. CBW over Bulk-OUT;
2. optional data stage in the declared direction;
3. CSW over Bulk-IN.

Stall/reset recovery and bulk endpoint data-toggle/ring recovery remain part of
the transport implementation rather than this pure wire module.

## Initial SCSI transparent subset

The bounded command layer currently provides:

- TEST UNIT READY;
- INQUIRY;
- REQUEST SENSE;
- READ CAPACITY (10);
- READ (10);
- WRITE (10);
- SYNCHRONIZE CACHE (10).

READ CAPACITY parameter data is decoded as big-endian and accepts power-of-two
logical block sizes of at least 512 bytes. Fixed-format sense data extracts the
sense key, ASC and ASCQ.

## Completion boundary

These modules do not perform USB I/O and therefore do **not** complete the
ROADMAP USB mass-storage or SCSI items by themselves. Completion requires the
native xHCI driver to configure real bulk endpoints and execute these commands
against an attached mass-storage device with exact guest evidence.
