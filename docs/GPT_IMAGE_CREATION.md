# Blank GPT image creation (development only)

`tools/create-usb-image.rs` creates **only a new regular host file** for a
future persistent Vibrix USB installation. It does **not** format FAT, install
`BOOTX64.EFI`, put `kernel.elf` on media, define the Vibrix on-disk
filesystem, install an operating system, or write to any physical USB drive.

Using pinned Rust:

```sh
rustc --edition=2024 tools/create-usb-image.rs -o /tmp/vibrix-create-usb-image
rustc --edition=2024 tools/inspect-gpt.rs -o /tmp/vibrix-inspect-gpt
/tmp/vibrix-create-usb-image ./vibrix-blank.img 64 512
/tmp/vibrix-inspect-gpt ./vibrix-blank.img 512
# Alternatively choose 4096-byte logical blocks for image creation AND inspection.
```

CLI: `create-usb-image <NEW-FILE.img> <SIZE-MiB> [512|4096]`.
Sizes 64 MiB–1 TiB are supported; 64 MiB is a fast metadata-test example,
not a recommended finished USB OS capacity. The target file **must not
exist** (an existing image, symlink or block-device path is refused);
pseudo/device filesystem directories `/dev`, `/proc` and `/sys` are
disallowed. It uses `create_new`, creates a sparse regular file, and writes
only GPT metadata. A failure removes the incomplete newly created file.
A Linux host with `/dev/urandom` is required to generate **fresh**, distinct
disk, ESP and data-partition unique GUIDs. This is not an entropy source for
the eventual Vibrix kernel.

## Populating the regular-file EFI System Partition

The blank-image creator remains intentionally separate from boot-file
installation. For a **regular-file** image created by the command above,
`tools/populate-esp.rs` can format only the still-blank 32 MiB ESP as FAT16
and install the adopted removable-media paths:

```text
/EFI/BOOT/BOOTX64.EFI
/VIBRIX/KERNEL.ELF
```

Compile the population and independent read-only inspection tools with the same
pinned Rust toolchain:

```sh
rustc --edition=2024 tools/populate-esp.rs -o /tmp/vibrix-populate-esp
rustc --edition=2024 tools/inspect-esp.rs -o /tmp/vibrix-inspect-esp
/tmp/vibrix-populate-esp ./vibrix-blank.img ./BOOTX64.EFI ./kernel.elf 512
/tmp/vibrix-inspect-esp ./vibrix-blank.img 512
```

Use `4096` consistently instead when the GPT image was created with
4096-byte logical sectors. The population tool rejects symlink/device/pseudo
filesystem targets, unexpected GPT geometry, non-blank ESP contents, a
non-PE/COFF loader, or a non-ELF kernel. It writes no bytes outside the
GPT-declared ESP and has no block-device mode. The inspector independently
checks the FAT16 BPB, both FAT copies, exact 8.3 directory tree, bounded file
chains and executable signatures.

CI additionally populates a GPT image with the **production** Vibrix UEFI
loader and kernel, attaches that image read-only to QEMU q35/OVMF as USB mass
storage, and requires firmware fallback boot through `BOOTX64.EFI`, kernel
file discovery, `ExitBootServices`, and standalone kernel entry. The image
SHA-256 must be unchanged after the boot. See
[ADR 0013](decisions/0013-efi-system-partition-layout.md).

## Experimental partition geometry

- Protective MBR in logical block 0.
- Primary GPT header at LBA 1 and a 128-entry × 128-byte array at LBA 2.
- Partition 1: an **unformatted** 32 MiB EFI System Partition at a
  1 MiB-aligned offset, with the standard ESP GPT type GUID.
- Partition 2: an **unformatted** Vibrix System partition named
  `Vibrix System`, starting at the next 1 MiB boundary and ending on a
  1 MiB boundary before backup metadata. Its project-owned GPT type GUID is
  `2e4a0f3b-6a3d-4e96-b99a-553d7c0b1201`. The type identifies the intended
  removable Vibrix system/root partition only; it does not imply that the
  partition is formatted, mountable, writable, or safe to select without the
  disk/ESP/root unique-GUID identity checks in ADR 0005.
- Identical backup entry array immediately before the reciprocal backup
  GPT header in the last block. Both headers and the 16 KiB entry arrays
  have separately calculated UEFI/GPT CRC-32 checksums.

Both 512- and 4096-byte-sector layouts are tested by compiling and running
the production writer and the existing **independent, read-only**
`tools/inspect-gpt.rs`. The latter rejects invalid primary or backup CRCs,
layout disagreements, duplicate unique partition GUIDs and overlaps.
The smoke path also checks refusal to overwrite an existing file. Formatting
and standalone writer unit tests exercise partition geometry, GUID version,
CRC-32, reciprocal header pointers and the no-overwrite policy.

**No block-device writing capability exists here.** This is deliberate:
a separately reviewed, explicitly opt-in USB provisioner must validate
removability, target identity and confirmation before writing a device.
The creator's raw blank GPT output is not bootable until a separate ESP
population step succeeds. The regular-file population path does **not** satisfy
M9 safe physical USB provisioning, persistent root, native post-firmware USB
storage access or Target 001 boot.

The tool does not make firmware handles into stable USB identities. Follow
[ADR 0005](decisions/0005-boot-usb-identity.md) for the proposed identity
and fail-closed reacquisition policy. See [GPT inspection](GPT_INSPECTION.md)
and the [USB system model](USB_MODEL.md).

Primary reference: UEFI GPT disk-layout specification and the standard
ESP and Basic Data type GUIDs. Implementation is first-party Rust with only
the host standard library; no new Cargo dependency is needed.
