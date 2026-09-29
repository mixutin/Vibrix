# Vibrix Boot ABI

The boot ABI is the versioned, firmware-independent contract between the Rust
UEFI loader and kernel. Both compile `shared/bootinfo.rs`.

## Current layout: BootInfo v4

The `#[repr(C)]` object is 168 bytes with alignment 8 on x86-64. The first 96 bytes preserve the complete v3 layout.

| Offset | Field | Meaning |
| --- | --- | --- |
| 0 | magic: u64 | `0x4942584952424956` (`VIBRIXBI` little endian) |
| 8 | version: u32 | 4 |
| 12 | _reserved: u32 | zero |
| 16 | framebuffer_base: u64 | physical framebuffer base |
| 24 | framebuffer_size: u64 | byte extent |
| 32–44 | width, height, stride, format: u32 | pixel geometry and format |
| 48 | rsdp: u64 | physical ACPI RSDP |
| 56 | memory_map: u64 | physical final memory-map buffer |
| 64 | memory_map_len: u64 | valid byte length |
| 72 | memory_descriptor_size: u64 | firmware descriptor stride |
| 80 | memory_descriptor_version: u32 | supported firmware descriptor version 1 |
| 84 | _reserved_v2: u32 | zero |
| 88 | kernel_window_table: u64 | physical address of retained, identity-mapped early-window PT |
| 96 | boot_identity_flags: u32 | bit 0 identity present; bit 1 system GUID present |
| 100 | _reserved_v4: u32 | zero |
| 104 | boot_disk_guid: [u8; 16] | validated whole-disk GPT GUID, or zero when absent |
| 120 | boot_esp_guid: [u8; 16] | validated boot ESP unique GUID, or zero when absent |
| 136 | boot_system_guid: [u8; 16] | optional Vibrix System partition GUID |
| 152 | boot_esp_first_lba: u64 | validated boot ESP first LBA |
| 160 | boot_esp_last_lba: u64 | validated boot ESP last LBA |

All stored addresses are physical numbers. The entry argument `*const BootInfo`
is a virtual pointer under the active page tables. Scalar validation alone
does not establish mapping, backing ownership or pointer provenance.

Version 4 preserves the complete 96-byte v3 prefix and appends only
firmware-neutral boot-media identity. The kernel reads the common-prefix version
before copying the complete 168-byte object. Versions 1–3 fail closed; an older
kernel also rejects v4. **Deploy a matching loader and kernel together.**
Reserved fields retain their zero meaning. When the identity-present flag is
clear, the entire new tail must be zero. When present, disk/ESP GUIDs and the
ESP LBA extent must be nonzero/valid; the optional system GUID is controlled by
its own flag. UEFI handles, device-path pointers, USB addresses and the firmware
map key never enter this ABI.

## Early mapping window

[ADR 0009](decisions/0009-early-mapping-window.md) reserves 512 empty 4 KiB
supervisor pages beginning at `0xffffc00000000000`. The loader allocates and
links the leaf table before the final map capture, rejects pre-existing
leaves, and identity-maps the table itself as writable and non-executable.
The table is retained EfiLoaderData. Its address must be nonzero, 4096-aligned,
below the low canonical limit and representable by the CPU physical width.
The kernel reserves this page and uses it only under exclusive boot-CPU,
interrupts-disabled ownership. It is not a recursive/whole-memory mapping.

## Handoff and ownership

1. Load and validate ELF64; allocate separate physical image backing.
2. Zero the image span, copy PT_LOAD contents and verify BSS.
3. Construct higher-half ELF mappings, the empty early window, and narrow
   transition mappings for PE code, stack, BootInfo, the full map buffer,
   RSDP, the window PT and the uncached GOP framebuffer.
4. Check NX, four-level paging and physical address constraints.
5. Refresh the final memory map in its preallocated buffer; construct BootInfo
   from that exact descriptor length/stride/version tuple plus any validated
   firmware-neutral boot USB identity.
6. Call ExitBootServices with the associated key. On stale-key failure,
   refresh only the existing map buffer and rebuild BootInfo before retrying.
7. Enable NX, switch CR3 and the dedicated stack, then pass BootInfo in RDI
   to `vibrix_kernel_entry`. No firmware Boot Services calls follow success.

Loader-owned resources remain reserved until explicitly transferred or
reclaimed. The type-7-only early frame allocator does not reclaim them.
The kernel must not treat arbitrary physical numbers as readable pointers.
Full ACPI table traversal, USB reacquisition and userspace remain separate.

## History and references

[ADR 0001](decisions/0001-bootinfo-address-spaces.md) established address
semantics; [ADR 0002](decisions/0002-kernel-load-layout.md) separates physical
backing from linked addresses; [ADR 0004](decisions/0004-memory-descriptor-version.md)
introduced the v2 descriptor-version tail. [ADR 0006](decisions/0006-transition-mappings.md)
and [ADR 0007](decisions/0007-uefi-exit-kernel-entry.md) describe the original
QEMU-verified v2 transition. Those v2 observations are historical evidence,
not evidence for later ABI behavior; the v3 mapping-window result belongs in ADR 0009. BootInfo v4 appends the boot-media identity tail without changing that v3 window contract.
