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

The inspector validates the protective MBR and **both GPT copies**. For the
primary and backup headers it checks the signature, GPT 1.0 revision, header
size/CRC, reserved field, reciprocal current/alternate header LBAs, usable
range, disk GUID, partition-entry count/size and entry-array CRC metadata. It
bounds each entry array to 16 MiB, uses checked byte-offset arithmetic, verifies
the primary array stays before the first usable LBA and the backup array stays
after the last usable LBA but before the backup header, validates both array
CRCs, and requires the primary and backup entry arrays to be byte-for-byte
identical.

After the two GPT copies agree, the inspector validates used partition GUIDs,
partition ranges and overlaps, and counts partitions plus EFI System
Partitions. Synthetic host tests cover complete GPT images with 512-byte and
4096-byte logical sectors plus corrupted headers, corrupted entry arrays,
non-reciprocal header locations, metadata disagreements, backup-array
placement errors and mismatched primary/backup entry data.

The tool makes **no writes**. A successful inspection means that the regular
disk-image file has internally consistent primary and backup GPT metadata; it
does **not** prove that a physical USB device is the intended Vibrix system
disk, that it is safe to overwrite, or that any partition contains a usable
filesystem or persistent Vibrix root.

Limitations: the inspector does not repair GPT structures, scan raw block
devices, identify removable-device topology, validate partition contents or
filesystems, provision a USB drive, or demonstrate actual persistent-root
boot. No roadmap checkbox is claimed complete by this tooling.

Primary reference: UEFI specification, GUID Partition Table (GPT) disk layout
and EFI System Partition GUID. The implementation is first-party Rust using
only the official host standard library.
