# ADR 0005: Identify and reacquire the persistent boot USB

- **Status:** Proposed
- **Date:** 2026-09-27
- **Roadmap:** [M7 — Detect the boot USB device robustly](../../ROADMAP.md) and [M9 — Persistent USB operating system](../../ROADMAP.md)
- **Supersedes:** None

## Context

Vibrix boots a kernel file from an EFI System Partition (ESP), then must use
its own USB stack to mount its persistent root on the **same removable USB
disk** after `ExitBootServices`. Firmware handles, Block I/O interfaces and
device-path pointers cease to be usable for storage access at that boundary.
USB addresses and port topology may change on the same machine; controller,
port and PCI addresses also change when the drive moves to another machine.

Today `boot/src/uefi.rs` opens the filesystem on the loaded image's
`DeviceHandle` to read `kernel.elf`. It does not inspect the parent whole-disk
block device or GPT, carry boot-disk identity to the kernel, or reacquire
USB block access. The implemented `BootInfo` v2 has no boot-device identity;
the QEMU kernel now executes after `ExitBootServices`. This proposal is a future contract, not a description of working
storage code. It follows [ADR 0001](0001-bootinfo-address-spaces.md): firmware
handles are not persistent kernel identities.

## Decision

1. **Identify the actual source before exit.** While Boot Services are
   available, start from the loaded image's device handle, verify that the
   filesystem containing `kernel.elf` belongs to a partition of a whole disk
   reachable over USB, and obtain the whole-disk Block I/O view. A file path
   alone, a separately discovered disk with a familiar label, or a boot option
   name does not prove provenance. Reject a non-USB source or a source that
   cannot be associated unambiguously with one whole disk. Firmware
   `RemovableMedia` is a hint, not proof of USB transport: many USB-attached
   SSDs report fixed media. Do not fall back to an internal disk.
2. **Record on-media identity, not a firmware object.** From that disk,
   validate both GPT header and entry-array copies (bounds, CRCs, matching
   disk/layout metadata and identical entry-array bytes), and record the GPT
   disk GUID plus the **unique partition GUID**
   of the booted ESP. A separately provisioned root-partition unique GUID is
   part of the identity when a persistent root is configured. The root GUID
   must come from an explicit boot configuration on the *booted ESP* and must
   name exactly one valid partition on that same disk; it must not be inferred
   by searching all disks for a matching filesystem. Reject duplicate unique
   partition GUIDs anywhere in the selected GPT (including duplicates of the
   ESP or root GUID), and reject zero or multiple matches for the configured
   root GUID; never accept the first matching entry by enumeration order.
   The exact configuration
   format, root partition type and on-disk filesystem are separate future
   decisions. Until that contract exists, this proposal cannot identify a
   root and cannot be treated as an implemented boot path.
3. **Pass a versioned, firmware-neutral hint.** A future coordinated
   loader/kernel ABI change passes the validated GUID tuple and presence of a
   root GUID as plain bytes/value fields, plus enough size/version information
   to reject incompatible handoffs. It must not pass UEFI handles, Block I/O
   pointers, live device-path pointers, USB addresses or firmware-private
   types. BootInfo v4 implements that reviewed extension as a flagged
   firmware-neutral disk/ESP/system GUID and ESP-LBA tail while preserving the
   complete v3 prefix. This ADR originally predated that implementation; it does
   not authorize passing firmware handles across ExitBootServices. The earlier
   design did **not** change the implemented v2 layout in
   [ADR 0004](0004-memory-descriptor-version.md).
   An optional copied device path or USB descriptor/serial can aid diagnostics
   or discovery priority, but can never override an on-media mismatch.
4. **Reacquire through native USB, read-only first.** After exit, initialize
   native xHCI and USB mass-storage support, enumerate accessible USB logical
   units and read each whole-disk GPT without using firmware services. Match
   the validated disk GUID **and** boot-ESP unique GUID, then (when present)
   check that the configured root unique GUID belongs to that same disk.
   Validate partition extents and the intended root filesystem before mounting.
   If primary and backup GPT copies disagree, reject the disk even when each
   copy has an individually valid CRC; do not select whichever copy matches
   the boot hint. Apply the same single-match partition checks on reacquisition.
   A matching partition GUID on a different whole disk is not sufficient.
   Recheck identity after a disconnect/reconnect before resuming I/O; never
   reuse a stale native device address or handle as proof.
5. **Fail closed.** If no USB disk matches, more than one disk matches (for
   example a byte-for-byte clone), identity metadata is missing/corrupt,
   partition association fails, GPT copies disagree, unique partition GUIDs
   are duplicated, or the root GUID is absent/invalid or matches other than
   exactly one entry when root mounting is requested, do not select a different
   disk or mount it writable.
   Report a diagnostic and enter a bounded recovery/read-only path or stop;
   any manual selection requires an explicit future recovery design. Do not
   break ties by enumeration order, port, serial number, label or internal
   disk. GPT CRCs detect corruption, **not** malicious cloning or tampering;
   authenticated boot/root identity is a separate security decision.

## Alternatives considered

- **Keep the EFI handle or Block I/O pointer:** unusable after Boot Services
  end and leaks firmware-specific lifetime rules into the kernel ABI.
- **Use a USB serial number, VID/PID, port path or PCI address alone:** serials
  may be unavailable, cloned or attached to a bridge rather than a logical
  unit; topology is not portable across machines. These are hints only.
- **Use only a GPT disk GUID or filesystem label:** copied media can duplicate
  either; a label is not unique. Disk and partition GUIDs together narrow
  accidental collisions, but clones still require the explicit ambiguity rule.
- **Scan internal disks or choose the first mountable root:** violates the
  USB-only system target rule and can mount or write the wrong device.

## Consequences and compatibility

Provisioning must create distinct GPT disk/partition GUIDs and bind the root
partition to the booted ESP by a reviewed configuration format. Cloning an
installation without regenerating identity is intentionally ambiguous when
both copies are present. Moving the single USB disk to another compatible
machine does not require preserving its original USB port or controller.
GPT corruption and an unavailable native USB driver prevent automatic root
mount rather than silently choosing a substitute. The identity tuple is a
locator with consistency checks, not a cryptographic authentication scheme.

Before `ExitBootServices`, firmware may be used to establish provenance and
copy validated value fields into a loader-owned handoff buffer. After exit,
only native drivers can reacquire storage. The future ABI change must respect
the buffer lifetime and mapped-pointer rules of ADR 0001 and versioning of
ADR 0004; no firmware representation is embedded in the stable kernel
interface. This document changes neither Rust ABI nor disk format today.

## Validation plan and evidence

**Existing evidence:** current CI's QEMU smoke reaches the standalone kernel
after a verified `ExitBootServices` handoff and proves a limited physical
frame allocator. Offline GPT tooling creates and validates blank host image
files. Neither demonstrates GPT **boot-device provenance**, native USB
access, a persistent root, or QEMU boot from the new unformatted GPT image. This ADR is a
design proposal, not an M7/M9 checkbox completion.

For implementation, test the loader's source-partition-to-whole-disk
association and malformed GPT inputs on synthetic 512- and 4096-byte-sector
images. In QEMU, boot from an explicitly USB-attached GPT image, exit Boot
Services, rediscover it through native xHCI/mass storage, read the selected
root, and verify persistence after reboot. Repeat with a second internal disk,
USB disk on another port, cloned USB disk, missing USB disk, changed GPT GUID,
corrupt GPT, duplicate ESP/root or unrelated unique partition GUIDs on one
disk, zero/multiple root GUID matches, individually CRC-valid but inconsistent
primary/backup GPT headers or entry arrays, mismatched root GUID and USB
reconnect. Require clear failure and
no internal-disk fallback or accidental writes. Validate portability on
Target 001 and a second compatible machine separately; QEMU cannot establish
bare-metal support.

## References

- [UEFI Specification 2.10: Loaded Image Protocol](https://uefi.org/specs/UEFI/2.10/09_Protocols_EFI_Loaded_Image.html) — `DeviceHandle` and image origin.
- [UEFI Specification 2.10: Device Path Protocol](https://uefi.org/specs/UEFI/2.10/10_Protocols_Device_Path_Protocol.html) — USB and hard-drive media nodes.
- [UEFI Specification 2.10: Media Access](https://uefi.org/specs/UEFI/2.10/13_Protocols_Media_Access.html) — Block I/O and partition discovery.
- [UEFI Specification 2.10: GUID Partition Table](https://uefi.org/specs/UEFI/2.10/05_GUID_Partition_Table_Format.html) — disk and partition GUIDs, GPT validation.
- [USB Mass Storage Class Bulk-Only Transport 1.0](https://www.usb.org/sites/default/files/usbmassbulk_10.pdf) — transport and device serial-number limitations.
- [Vibrix USB system model](../USB_MODEL.md) and [Vibrix Boot ABI](../BOOT_ABI.md).

No external implementation source was used.


## BootInfo v4 implementation note

The current implementation carries only the validated GPT disk GUID, boot ESP
unique GUID and extent, and optional Vibrix System partition GUID. The loader
fills this tail only after exact loaded-image USB-path matching and primary plus
backup GPT validation through the matched whole-disk Block I/O handle. The
kernel can therefore retain stable media identity after firmware services end
without retaining topology or firmware pointers.

Native post-ExitBootServices USB/SCSI reacquisition still has to match this tuple
before persistent-root selection is complete.
