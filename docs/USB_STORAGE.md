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

## Native xHCI BOT/SCSI evidence

The `usb-storage-probe` kernel profile reuses the production xHCI controller,
slot, EP0 and event-ring ownership used by device enumeration. It parses the
actual configuration descriptor for exactly one class 08/subclass 06/protocol
50 interface, configures one Bulk-OUT and one Bulk-IN endpoint, and carries the
shared CBW/CSW and SCSI contracts over real xHCI Normal TRBs.

The bounded QEMU fixture executes TEST UNIT READY, INQUIRY, REQUEST SENSE,
READ CAPACITY (10), READ (10), WRITE (10) and SYNCHRONIZE CACHE (10). It reads
the final logical block, saves its original contents in a retained DMA page,
writes a deterministic pattern, flushes and reads the pattern back, then restores
the original block, flushes again and verifies the restoration. CI also compares
the entire backing-image SHA-256 before and after the guest run.

Every transfer validates the event slot, endpoint DCI, completed TRB pointer,
completion code and residue. CBW/CSW tags must match and a command must report
`Passed`; malformed/ambiguous descriptors and unsupported capacities fail
closed. The test uses one directly attached 2 MiB QEMU USB disk with 512-byte
logical blocks.

## Completion boundary

This native probe is the implementation/evidence path for the bounded M7
**USB mass-storage transport** and **SCSI transparent command subset** items
once exact-head CI is green. It does not identify the firmware boot USB, keep
the device open as a long-lived block backend, mount VibrixFS from USB, survive
hotplug/reset recovery, support multiple LUNs/devices, provide DMA isolation, or
prove physical Target 001 behavior. Those remain separate roadmap work.
