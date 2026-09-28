# ADR 0012: Vibrix removable system GPT partition type

- **Status:** Accepted
- **Date:** 2026-09-28
- **Roadmap:** M9 — Vibrix USB system partition layout
- **Supersedes:** None

## Context

The host-side GPT image tool previously created an EFI System Partition plus a
second partition using the generic Microsoft Basic Data type. That was safe for
early tooling, but it did not express that the second partition is specifically
reserved for Vibrix's removable system/root state. A portable USB OS needs an
unambiguous on-media partition role that is independent of filesystem labels,
USB port topology and firmware handles.

This decision defines only the **partition type and outer GPT geometry**. It
does not make the partition formatted, mountable, persistent or writable by
the kernel. VibrixFS, root identity, native USB reacquisition and provisioning
remain separate contracts.

## Decision

Adopt the project-owned GPT partition type GUID:

`2e4a0f3b-6a3d-4e96-b99a-553d7c0b1201`

for the removable **Vibrix System** partition.

GPT stores GUID fields 1–3 little-endian, so the exact 16 on-media bytes are:

`3b 0f 4a 2e 3d 6a 96 4e b9 9a 55 3d 7c 0b 12 01`.

The development image layout remains:

1. protective MBR and reciprocal primary/backup GPT metadata;
2. one 32 MiB EFI System Partition beginning at the 1 MiB alignment boundary;
3. one Vibrix System partition beginning immediately after the ESP at the next
   1 MiB boundary and extending through the final aligned usable range.

The second partition's GPT name is `Vibrix System`. Its unique partition GUID
is freshly generated for each image and is distinct from the GPT disk GUID and
ESP unique GUID.

The partition-type GUID identifies **role, not identity**. Runtime root
reacquisition must still follow ADR 0005: match the validated disk GUID, ESP
unique GUID and explicitly configured root unique GUID on the same native USB
disk. A Vibrix System type on an internal disk, cloned disk or unrelated USB
disk is never enough to select it.

## Compatibility and safety

Existing development images using generic Basic Data are not silently
reinterpreted. New images created after this decision use the Vibrix System
type; migration of old images requires an explicit future tool.

The host image creator remains regular-file-only and uses exclusive
`create_new`; it has no raw-device write path. This ADR adds no installer,
formatter or destructive provisioning behavior.

The partition may later contain VibrixFS v1, but the GPT type is independent
of filesystem major version. A future filesystem format can retain this outer
partition role unless a later ADR deliberately changes it.

## Validation

The independent read-only GPT inspector recognizes and counts the exact type
GUID while retaining all existing primary/backup CRC, reciprocal geometry,
unique-GUID, overlap and Protective MBR checks. CI creates fresh 512-byte and
4096-byte-sector images and requires exactly two partitions: one ESP and one
Vibrix System partition.

This evidence establishes the host-side **Vibrix USB system partition layout**.
It does not establish an EFI filesystem layout, a formatted root, persistent
USB I/O, a safe physical-device provisioner, Target 001 boot or portability
between machines.

## References

- UEFI 2.10 GPT disk layout and partition entry format
- ADR 0005 — Identify and reacquire the persistent boot USB
- ADR 0010 — VibrixFS v1 on-disk format
- docs/GPT_IMAGE_CREATION.md
