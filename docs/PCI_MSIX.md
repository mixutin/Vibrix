# MSI-X bounds and masked programming

Author: GPT-6 Astra Pro.

This M4 increment depends on conventional capability discovery and the MSI
configuration transport/message builder. It supplies checked MSI-X geometry
and a production table/configuration programming routine. It does not yet
supply native MMIO mappings or prove interrupt delivery.

## Bounds before device access

`MemoryBar` validates metadata arithmetic/alignment, not hardware ownership.
The resource owner must supply real memory BAR extents; IO BARs and high
halves of 64-bit BARs must not be represented as independent apertures.
`Layout` checks the entire advertised table and pending-bit array against
those extents. Table length is 16 bytes per vector; the pending array has
one eight-byte word per 64 vectors, rounded up. Physical overlaps are
rejected even if different BIRs alias the same address. No BAR sizing writes
or guessed aperture based solely on an assigned base are used.

## Enable-last protocol

The unsafe activation API requires exclusive device/config/table ownership,
a validated live UC mapping, quiesced source/INTx, and a permanent handler
for a physical xAPIC destination with no interrupt remapping. It checks the
capabilities again and refuses to take over already enabled MSI/MSI-X.
It function-masks first, masks every advertised entry, writes one entry's
address/data, enables the still-function-masked capability, unmasks that
entry, and clears the function mask last. Every write is read back. All
unused entries stay masked. Table operations use aligned DWORDs, and the
pending-bit array is never written.

The failure path attempts to mask and disable, but never claims hardware
rollback or drained interrupts. The caller must retain handlers and mappings
on errors. Verified disable alone does not free in-flight interrupt state.

## Verification boundary

Host tests cover 1/63/64/65/2048-vector geometry, invalid/overflowing extents,
missing BARs, physical alias overlap, exact selected-entry state, all config
and table write-failure positions, and rejection before writes for an
out-of-range entry. The same production source is compiled without `std`.
Exact-head Actions results, not these descriptions, establish test status.
Native QEMU delivery remains required before the broad M4 MSI/MSI-X item
can be checked. No dependency or on-disk/BootInfo ABI change is introduced.

Primary references: [PCI-SIG MSI-X ECN](https://pcisig.com/PCIConventional/ECN/Base)
and [QEMU ivshmem interface](https://www.qemu.org/docs/master/specs/ivshmem-spec.html).
