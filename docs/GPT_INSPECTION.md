# Read-only GPT image inspection

`tools/inspect-gpt.rs` is an **offline host-side diagnostic**, not a Vibrix
installer, formatter, recovery tool, runtime storage driver or USB provisioning
utility. It takes one existing **regular file** representing a whole-disk image,
opens it read-only, and rejects block devices and other non-regular files.

Using the pinned Rust toolchain:

```sh
rustc --edition=2024 tools/inspect-gpt.rs -o /tmp/vibrix-inspect-gpt
/tmp/vibrix-inspect-gpt path/to/usb-image.img 512
# Specify 4096 for an image with 4096-byte logical sectors.
rustc --edition=2024 --test tools/inspect-gpt.rs -o /tmp/vibrix-gpt-tests
/tmp/vibrix-gpt-tests
```

The inspector validates the protective MBR, primary and backup GPT signatures,
version, header size/CRC, header LBAs, usable range, partition-entry
bounds/CRCs, reciprocal header locations, matching disk/layout metadata,
byte-identical primary/backup entry arrays, nonempty unique GUIDs, partition
ranges and overlaps. The inspector also **rejects duplicate nonzero partition
unique GUIDs** across all used GPT entries, including non-ESP entries. It
reports the GPT **disk GUID** and each used partition's **unique GUID** with
its 1-based GPT entry index for read-only identity diagnostics. GUID text
follows the standard mixed-endian GPT/UEFI representation rather than the raw
on-disk byte sequence. Duplicate-GUID rejection occurs only after both GPT
copies validate; failure produces no partially accepted identity report.

Synthetic corruption and identity tests cover both 512-byte and 4096-byte
logical sectors, including individually CRC-valid matching copies with
duplicate partition GUIDs. The tool counts partitions and EFI System
Partitions by GPT type GUID. The entry table is capped at 16 MiB and the
tool makes **no disk-image writes**.

Limitations: it does not repair either GPT, or validate partition
contents, filesystems, removable-device identity, USB topology or actual
persistent-root boot. Reported GUIDs are diagnostic on-media locators,
**not** proof that the image came from the booted USB, a valid root configuration,
a unique disk among connected clones, or cryptographic authentication.
A valid disk image does not prove that the target USB drive is safe to write.
It does not check whether a partition contains Vibrix root data. This
inspection is groundwork aligned with proposed ADR 0005, not implementation of
USB discovery, boot-source provenance, persistent-root reacquisition, or an
M7/M9 roadmap checkbox.

Primary reference: UEFI specification, GUID Partition Table (GPT) disk
layout and EFI System Partition GUID. The implementation is first-party
Rust with the official host standard library only.
