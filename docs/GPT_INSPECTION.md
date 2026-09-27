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

The inspector validates the protective MBR, **primary** GPT signature,
version, header size/CRC, header LBAs, usable range, partition-entry
bounds/CRC, nonempty unique GUIDs, partition ranges and overlaps. It counts
partitions and EFI System Partitions by GPT type GUID. The entry table is
capped at 16 MiB and the tool makes **no** writes.

Limitations: it does not verify or repair the **backup** GPT, partition
contents, filesystems, removable-device identity, USB topology or actual
persistent-root boot. A valid disk image does not prove that the target USB
drive is safe to write. It does not check whether a partition contains
Vibrix root data. No roadmap checkbox is claimed complete by this tooling.

Primary reference: UEFI specification, GUID Partition Table (GPT) disk
layout and EFI System Partition GUID. The implementation is first-party
Rust with the official host standard library only.
