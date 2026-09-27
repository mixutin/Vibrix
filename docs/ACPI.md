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

**Pre-integration limit (PR #51):** The original RSDP-only kernel
marker and host SDT/MCFG fixtures did not establish real firmware XSDT/MCFG
access. That gap is now addressed for a bounded x86-64 SDT reader below.
ACPI byte-slice parsers still require mapped, readable backing covering
their validated lengths. Never construct a slice from an arbitrary firmware
physical number. MCFG ECAM MMIO requires separate mapping and memory-type
verification before access; this work has not enabled PCIe extended
configuration space, APIC interrupts or userspace.

Primary specifications: [ACPI 6.5, §5.2 tables](https://uefi.org/specs/ACPI/6.5/05_ACPI_Software_Programming_Model.html)
and [PCI Firmware Specification](https://pcisig.com/specification-overview/pci-firmware),
section 4, MCFG. No third-party OS implementation source was used; no
community crate was needed for this bounded firmware metadata decoder.

## Mapped real-table checkpoint (QEMU verified — PR #64)

The M4 integration adds a bounded, one-table-at-a-time reader
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
[Actions run 36344773356](https://github.com/mixutin/Vibrix/actions/runs/36344773356)
passed formatting, host tests including ACPI region ownership and both
target builds/Clippy plus seven QEMU boots. In each configuration the
real kernel's independent debugcon reported
`VIBRIX: kernel ACPI XSDT and MCFG mapped and parsed`; COM1 reported
`kernel ACPI: 1 validated MCFG allocations`. This verifies actual
post-firmware SDT parsing on QEMU q35, not Target 001 or any ECAM MMIO
read. The M4 ACPI parser checkbox records this limited table foundation;
MCFG/ECAM, interrupt routing, full AML and hardware drivers remain open.

## Native read-only PCIe ECAM bootstrap (PR under validation)

The next M4 increment extends the existing MCFG parser to select only its
segment-zero allocation containing bus zero, verifies PAT index 3 is UC,
and checks every prospective function-zero 4 KiB ECAM page against the
retained final UEFI map. Only non-runtime, UC-capable
`EfiReservedMemoryType` or `EfiMemoryMappedIO` pages are eligible;
QEMU/OVMF's actual ECAM region was observed as reserved type 0, attribute
`EFI_MEMORY_UC`, so accepting only type 11 would incorrectly reject it.
The validated MCFG allocation additionally supplies the PCIe aperture
identity and bounds; normal RAM types remain forbidden. A one-page read-only/NX
supervisor mapping with PCD+PWT is installed only for each volatile
32-bit vendor/class register read, then unmapped. These reads are not
PCI device register writes, BAR MMIO accesses, extended configuration
space discovery on all buses, IOMMU setup or driver initialization.

On a successful QEMU head, the kernel should report
`VIBRIX: kernel PCI ECAM bus0 read` on native debugcon and
`Vibrix ECAM segment0 bus0: <N> devices, <M> xHCI` on COM1;
the virtual-xHCI configuration must report an xHCI controller using
this ECAM path independently from legacy CF8/CFC scanning.
A failed validation is a real failure, not a reason to remove the
memory-type safety checks. Leave the M4 ECAM checkbox open until the
actual exact-head QEMU tests pass.
