# ADR 0013: Removable-media EFI System Partition layout

- Status: Accepted
- Scope: M9 EFI System Partition layout

## Context

Vibrix already stages the UEFI loader at `/EFI/BOOT/BOOTX64.EFI` and the
kernel at `/vibrix/kernel.elf` for QEMU's synthetic host-directory FAT
device. M9 needs those names to become an explicit on-media contract inside
the GPT EFI System Partition rather than relying on QEMU's directory bridge.

The current regular-file GPT creator reserves a 32 MiB ESP beginning at the
1 MiB boundary and deliberately leaves it blank. The loader opens the kernel
through UEFI Simple File System using `\\vibrix\\kernel.elf`; removable
x86-64 firmware discovers the loader through the standard fallback path
`\\EFI\\BOOT\\BOOTX64.EFI`.

## Decision

The adopted ESP contains exactly the required boot tree:

```text
/
├── EFI/
│   └── BOOT/
│       └── BOOTX64.EFI
└── VIBRIX/
    └── KERNEL.ELF
```

Names are encoded as ordinary FAT 8.3 short names, so the on-media names are
case-insensitive and need no long-filename records. The 32 MiB partition is
formatted as FAT16 with one logical sector per cluster, two identical FAT
copies, a fixed 512-entry root directory, media byte `0xf8`, volume label
`VIBRIX ESP`, and a FAT boot signature. Both 512- and 4096-byte logical
sector sizes are supported by the host tooling.

FAT16 is intentional for this 32 MiB geometry. With metadata subtracted, the
cluster count is below the FAT32 threshold while remaining safely in FAT16.
The ESP GPT type remains the standard EFI System Partition GUID; this ADR does
not change partition geometry or the project-owned Vibrix System type from
ADR 0012.

`tools/populate-esp.rs` writes only an existing, blank ESP in an existing
regular `.img` file. It rejects symlink/device/pseudo-filesystem targets,
unexpected GPT geometry, multiple ESPs, non-blank ESP contents, zero/oversized
boot artifacts, a non-PE loader and a non-ELF kernel. It never accepts a raw
block device and never writes outside the GPT-declared 32 MiB ESP.

`tools/inspect-esp.rs` is independently read-only. It verifies the BPB,
FAT16 cluster count, identical FAT copies, exact short-name directory tree,
bounded file chains, and PE/ELF signatures. The existing GPT inspector remains
the authority for GPT CRCs and reciprocal primary/backup metadata.

## Verification boundary

Host tests cover both supported logical-sector sizes, format/inspect round
trips and refusal to overwrite an already populated ESP. The runtime proof
boots a regular-file GPT image through QEMU q35/OVMF as a virtual USB mass
storage device, requiring the real UEFI loader to find `BOOTX64.EFI`, open
`kernel.elf`, exit Boot Services and reach the kernel. The image is attached
read-only and its SHA-256 is checked before/after the boot.

This proves the M9 **EFI System Partition layout** only. It does not prove a
post-EBS xHCI/USB-storage driver, persistent root, VibrixFS mounting, stable
USB reacquisition, Target 001 firmware compatibility, installer/provisioner
safety, updates or recovery.

## Compatibility

The loader's existing `\\vibrix\\kernel.elf` path is unchanged. Existing
QEMU host-directory boots remain valid. Future ESP additions must preserve the
two required paths or introduce an explicit boot-layout version/migration.

Primary references: UEFI removable-media boot behavior and FAT on-media
format. The implementation is first-party Rust using only the host standard
library; no new Cargo dependency is introduced.
