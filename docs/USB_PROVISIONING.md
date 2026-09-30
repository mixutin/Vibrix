# Safe USB provisioning tool

`tools/provision-usb.py` is the destructive host-side boundary for writing a
prepared Vibrix image to physical removable media on Linux.

It intentionally refuses generic device paths. The operator must select a
`/dev/disk/by-id/usb-*` symlink that resolves to a whole block device which the
kernel reports as removable. The tool refuses partitions, mounted targets or
mounted child partitions, empty/non-regular source images, and devices smaller
than the image.

A normal write additionally requires `--confirm` with the exact resolved
canonical device (for example `/dev/sdb`). Writes are synchronous, followed by
a read-back SHA-256 over exactly the image length. A mismatch is an error.

Use `--dry-run` to perform the safety checks without writing.

This tool does not decide which physical drive should contain Vibrix, format the
Vibrix System partition, create persistent root, resize partitions, or bypass
Linux device permissions. It is deliberately Linux-specific and deliberately
rejects USB devices that the kernel does not mark removable.
