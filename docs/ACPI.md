# ACPI table parsing foundation

The x86-64 kernel builds `kernel/src/arch/x86_64/acpi.rs` under `no_std`.
The module is a **pure parser of already readable byte slices**:
`parse_rsdp` checks the 20-byte ACPI 1.0 checksum and, on revision 2+,
the reported extended length and independent full-table checksum;
`Sdt::parse` validates the 36-byte common header, a bounded total length
(up to 1 MiB) and checksum. Only then may `root_entries` decode RSDT
4-byte or XSDT 8-byte physical-address entries.

For MCFG (PCI ECAM discovery), the same parser checks the SDT checksum,
44-byte header with zero reserved bytes, exact 16-byte entry stride,
nonzero 1 MiB-aligned ECAM bases, valid bus bounds, checked address ranges
and overlapping bus ranges within the same PCI segment. A pure
`config_physical` helper bounds bus/device/function/register values and
computes a **physical numeric address**, never a Rust pointer or a mapped
memory reference.

`rustc --edition=2024 --test kernel/src/arch/x86_64/acpi.rs`
runs production-linked host tests for normal ACPI 1.0/2.0+ tables, malformed
signatures/lengths, independently corrupted RSDP and SDT checksums,
RSDT/XSDT entry lengths, overlapping MCFG bus ranges, reserved bytes,
ECAM overflow and PCI bounds. CI also compiles this module as part of the
bare-metal kernel.

The production kernel now invokes `parse_rsdp` **after ExitBootServices**
on the loader-validated RSDP bytes mapped under ADR 0006. The QEMU
smoke test requires the **kernel-only** `VIBRIX: kernel ACPI RSDP parsed`
marker in both normal and panic-probe boots. Firmware-specific RSDP
extensions beyond the 36-byte currently mapped/validated window fail
closed rather than being read from an unknown extent.

**Limits:** Passing the kernel RSDP marker and host SDT/MCFG fixtures do
not prove that the kernel has mapped or parsed the real firmware XSDT/MCFG
pages, discovered actual PCI devices, or accessed native MMIO on QEMU
or Target 001. Kernel
call sites must first establish mapped, readable memory covering the
advertised length, copy it into owned bounded storage if lifetime requires,
and then call these slice parsers. Do not create an unchecked
`slice::from_raw_parts` from an RSDP/XSDT/MCFG physical number. MCFG
MMIO must be deliberately mapped and the relevant memory types chosen
before accessing the returned config-space address. M4 ACPI/MCFG/PCI
roadmap checkboxes remain unchecked until real runtime behavior is
demonstrated.

Primary specifications: [ACPI 6.5, §5.2 tables](https://uefi.org/specs/ACPI/6.5/05_ACPI_Software_Programming_Model.html)
and [PCI Firmware Specification](https://pcisig.com/specification-overview/pci-firmware),
section 4, MCFG. No third-party OS implementation source was used; no
community crate was needed for this bounded firmware metadata decoder.

## Mapped real-table checkpoint (QEMU validation pending)

The M4 integration branch now adds a bounded, one-table-at-a-time reader
using BootInfo v3's reserved 2 MiB leaf window. The kernel checks every
mapped page against its validated final UEFI memory map, accepting only
WB-capable EfiACPIReclaimMemory/EfiACPIMemoryNVS, never runtime or
conventional allocations. It reads the SDT header first, limits the full
length to 1 MiB, then maps the complete table read-only and NX; it
validates root entries and reads actual MCFG allocation descriptors.
Every temporary leaf is unmapped even on error, and untrusted physical
addresses never become direct identity pointers. The early mapper is
boot-CPU/IRQs-off only, with no concurrent reclamation of ACPI pages.

The QEMU smoke suite now demands a separate **kernel-origin** real-XSDT/
MCFG success marker after the existing RSDP and virtual-mapping markers.
This is still not ECAM MMIO access, MSI setup, hardware interrupt routing,
full general ACPI namespace interpretation, or Target 001 confirmation.
Do not check MCFG/ECAM or other driver checkboxes from table parsing alone.
The earlier paragraph's limitation on physical table access describes the
pre-integration state; update this section with exact-run evidence once CI
passes, and leave real-table validation unclaimed if CI rejects it.
