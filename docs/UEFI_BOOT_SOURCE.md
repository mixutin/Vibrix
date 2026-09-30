# UEFI boot-source device-path parsing

Before `ExitBootServices`, the loader can inspect the Loaded Image device
handle's Device Path and copy only firmware-neutral boot-source facts.

`shared/uefi_boot_path.rs` validates a bounded copied device path, requires a
USB messaging node, exactly one GPT hard-drive media node, a nonzero partition
number/size and a nonzero GPT partition signature. The result is the ESP
partition GUID and extent plus a boolean proving that the firmware path
contained USB transport.

This is a provenance prerequisite, not the persistent identity by itself.
Topology and USB addresses are never carried across the firmware boundary. The
loader still must associate this partition with a whole-disk Block I/O handle,
validate both GPT copies, obtain the disk GUID, and pass the resulting
firmware-neutral tuple to the kernel.
