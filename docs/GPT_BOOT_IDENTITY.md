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
