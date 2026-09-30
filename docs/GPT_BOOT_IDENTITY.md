# Shared GPT boot identity validator

The loader and post-firmware kernel must agree on the same on-media identity
rules when reacquiring the removable system disk. `shared/gpt_identity.rs`
provides that allocation-free contract.

The caller supplies already-read primary/backup GPT headers and entry arrays.
The validator checks header signatures/revision/CRCs, reciprocal locations,
matching identity/layout metadata, entry-array CRC and byte equality, nonzero
disk/partition GUIDs, duplicate GUIDs, partition ranges/overlap, exactly one EFI
System Partition, and at most one Vibrix System partition. It returns only the
firmware-neutral disk/ESP/optional-system GUID tuple.

The opt-in boot-identity probe now obtains the loaded image's exact whole-disk
UEFI Block I/O parent, reads the primary and backup GPT headers and entry arrays
from that disk, validates both copies with this shared parser, and requires the
validated ESP GUID/extent to match the partition in the loaded-image device
path. Temporary firmware buffers are page-backed, alignment-checked and released
before ExitBootServices; only the firmware-neutral identity tuple survives the
read.

This remains a pre-ExitBootServices provenance proof. Passing the neutral tuple
through BootInfo and reacquiring the same removable disk through Vibrix's native
post-firmware USB mass-storage path are still separate integration steps.


## Native post-firmware reacquisition

The `usb-boot-reacquire-probe` joins the BootInfo v4 identity handoff to the
native xHCI/BOT/SCSI path. After `ExitBootServices`, the kernel reads the
candidate disk's primary and backup GPT headers and bounded entry arrays using
SCSI READ(10), validates both copies with the same shared
`gpt_identity` parser, and compares the disk GUID, ESP unique GUID and extent,
and optional Vibrix System partition GUID exactly against BootInfo.

This path is deliberately read-only. It does not use the disposable storage
probe's reversible last-block write because the last block of a GPT disk is the
backup header. The evidence workflow checks the complete image hash before and
after native reacquisition.

A second QEMU boot presents a different valid USB disk on the lower native root
port while firmware is instructed to boot the real Vibrix disk on another port.
The kernel must reject the first native disk on GPT-identity mismatch rather
than accepting enumeration order, topology, or a merely valid GPT.

The current proof is bounded to a directly attached SCSI-transparent BOT device,
512- or 4096-byte logical blocks, READ CAPACITY(10), and GPT entry arrays up to
16 KiB. Hub-routed storage, multiple-LUN search, READ CAPACITY(16), larger GPT
entry arrays, hotplug/reconnect, persistent-root mounting, physical Target 001
validation, and writable system operation remain separate work.
