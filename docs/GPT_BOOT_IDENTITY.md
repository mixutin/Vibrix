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

This does not yet obtain the firmware boot disk or perform native USB reads.
Those are the next integration steps; keeping the parser shared prevents the
loader and kernel from making different trust decisions about the same media.
