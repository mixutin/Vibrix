# VibrixFS v1 wire specification

This document is the normative byte-level proposal for the first Vibrix-owned
persistent filesystem. **No current image is formatted as VibrixFS yet.** A
reader must validate all bounds and checksums before treating on-media numbers
as offsets, allocation ownership or Rust references.

## 1. Common rules

- byte order: little-endian
- filesystem block size: exactly 4096 bytes
- minimum filesystem size: 4096 blocks (16 MiB)
- maximum v1 block number: 2^48 - 1
- inode numbers: unsigned 64-bit; 0 invalid, 1 root
- inode number `n` is stored in inode-table slot `n - 1`; slot 0 stores root inode 1
- names: valid UTF-8, 1–255 bytes, no NUL, no slash
- all reserved bytes/bits written as zero and checked as zero unless a future
  compatible feature explicitly redefines them
- all block ranges are half-open [start, start + length) and arithmetic is
  checked before multiplication/addition

The 8-byte filesystem magic is ASCII VIBRIXFS. Major version is 1 and minor
version begins at 0. Unsupported major versions always fail closed.

## 2. Superblock — filesystem block 0 and final block

Only the first 256 bytes are currently defined; the remainder of the 4096-byte
block is zero. The checksum covers all 4096 bytes with bytes 120–123 zero.

| Offset | Bytes | Field |
| ---: | ---: | --- |
| 0 | 8 | magic = VIBRIXFS |
| 8 | 2 | major = 1 |
| 10 | 2 | minor |
| 12 | 4 | header_bytes = 256 |
| 16 | 4 | block_bytes = 4096 |
| 20 | 4 | flags; bit 0 = clean shutdown |
| 24 | 8 | generation |
| 32 | 8 | total_blocks |
| 40 | 8 | total_inodes |
| 48 | 8 | block_bitmap_start |
| 56 | 8 | block_bitmap_blocks |
| 64 | 8 | inode_bitmap_start |
| 72 | 8 | inode_bitmap_blocks |
| 80 | 8 | inode_table_start |
| 88 | 8 | inode_table_blocks |
| 96 | 8 | root_inode = 1 |
| 104 | 8 | journal_start, zero in base v1 |
| 112 | 8 | journal_blocks, zero in base v1 |
| 120 | 4 | IEEE CRC-32 of full superblock block |
| 124 | 4 | compatible_features |
| 128 | 4 | incompatible_features |
| 132 | 4 | required_readonly_features |
| 136 | 16 | filesystem UUID, nonzero |
| 152 | 16 | root-partition unique GUID copy, GPT mixed-endian bytes |
| 168 | 88 | reserved zero |
| 256 | 3840 | reserved zero |

The secondary superblock is byte-identical. A valid filesystem requires
geometry to place block 0 and the final block outside every allocatable range.

### Geometry invariants

- total_blocks >= 4096
- bitmap/table starts and lengths are nonzero except the base-v1 journal fields
- metadata ranges do not overlap one another or either superblock
- every metadata range lies below total_blocks - 1
- total_inodes >= 1; inode table capacity is at least total_inodes
- bitmap capacity covers total_blocks and total_inodes + 1
- the root-partition GUID copy must equal the already selected GPT partition's
  unique GUID; it is a consistency binding, not a disk-selection substitute
- both superblock UUID/GUID fields are nonzero

## 3. Allocation bitmaps

Bitmap bit numbering is least-significant bit first within each byte.

- block bitmap bit n describes filesystem block n
- inode bitmap bit n describes inode number n
- bit 0 of the inode bitmap is permanently set/reserved
- block 0, final block, all bitmap blocks, all inode-table blocks and any
  future journal region must be marked allocated
- padding bits beyond the declared object count are set to 1 so corrupt
  allocators cannot consume them

A bitmap bit establishes allocation bookkeeping only. Readers must still reject
duplicate/overlapping file extents and metadata references.

## 4. Inode record — 256 bytes

Each inode record has fixed width. The checksum covers all 256 bytes with
bytes 184–187 zero.

| Offset | Bytes | Field |
| ---: | ---: | --- |
| 0 | 8 | inode number |
| 8 | 1 | file type: 1 regular, 2 directory, 3 symlink, 4 char, 5 block |
| 9 | 1 | extent_count, 0–6 |
| 10 | 2 | mode low 12 bits; upper bits zero |
| 12 | 4 | uid |
| 16 | 4 | gid |
| 20 | 4 | link_count |
| 24 | 8 | logical size_bytes |
| 32 | 8 | allocated_blocks |
| 40 | 8 | atime seconds |
| 48 | 4 | atime nanoseconds |
| 52 | 4 | reserved zero |
| 56 | 8 | mtime seconds |
| 64 | 4 | mtime nanoseconds |
| 68 | 4 | reserved zero |
| 72 | 8 | ctime seconds |
| 80 | 4 | ctime nanoseconds |
| 84 | 4 | reserved zero |
| 88 | 16 | object generation/nonce; nonzero |
| 104 | 72 | six 12-byte extent slots |
| 176 | 8 | device number or zero |
| 184 | 4 | inode CRC-32 |
| 188 | 4 | inode flags |
| 192 | 64 | reserved zero |

Each extent slot contains a 6-byte little-endian physical start block, a
4-byte block count and 2-byte flags (zero in base v1). Unused slots are all
zero. Used extents are sorted by physical block, nonzero length, inside
allocatable data blocks and pairwise non-overlapping.

v1 does not support indirect extent trees. A base-v1 writer must refuse a file
requiring more than six extents rather than inventing an incompatible format.

For regular files, allocated_blocks times 4096 must be at least size_bytes. For
device nodes, device_number may be nonzero and data extents must be absent.
Base-v1 symlinks use file extents containing UTF-8 target bytes and prohibit
NUL.

## 5. Directory record

Directories are byte streams packed within their extents. Every record starts
on an 8-byte boundary and record_bytes is a multiple of 8.

| Offset | Bytes | Field |
| ---: | ---: | --- |
| 0 | 8 | inode |
| 8 | 2 | record_bytes |
| 10 | 1 | name_bytes |
| 11 | 1 | file type mirror |
| 12 | 4 | reserved zero |
| 16 | N | UTF-8 name |
| 16+N | pad | zero to record_bytes |

A record is invalid if its inode is zero/out of range/unallocated, its name is
invalid, the record crosses the directory's declared size_bytes, or its type
mirror disagrees with the referenced inode. Each directory contains exactly one
dot entry referencing itself and one dot-dot entry; the root's dot-dot points
to inode 1.

Duplicate names in a directory are invalid. Name comparison in v1 is exact
UTF-8 byte comparison; there is no Unicode normalization or case folding.

## 6. Feature bits and compatibility

All feature words are zero for base v1.

- **compatible:** a reader may ignore a defined unknown bit only when its future
  definition explicitly guarantees layout readability
- **incompatible:** any unknown set bit requires mount refusal
- **required-readonly:** an unknown set bit prohibits writable mount

A future format change that alters existing field meaning, block size, inode
width, directory framing or extent encoding requires a new major version.

## 7. Validation order

A reader should fail closed in this order:

1. know the selected partition byte extent and require 4096-byte alignment;
2. read block 0 without trusting any internal offsets;
3. validate magic/version/header/block size/reserved bytes/checksum;
4. validate total_blocks against the partition byte extent with checked math;
5. validate metadata ranges/capacities and feature bits;
6. read the final-block superblock by the externally bounded block count and
   independently validate it;
7. require immutable identity/geometry fields of the two copies to agree;
8. validate bitmaps before trusting inode allocation;
9. validate every referenced inode and extent before reading file data;
10. validate directory records and parent/name relationships.

CRC success never overrides a bounds, feature, allocation or identity failure.

## 8. Deliberately unspecified in this document

These remain separate M8/M9 work:

- metadata transaction/journal record layout and crash-recovery ordering
- allocation policy, free-space search and fragmentation policy
- formatter/recovery command-line interface
- VFS cache/writeback behavior
- authenticated metadata or encryption
- package-database layout
- persistent root mount procedure and USB reconnect behavior

Until crash consistency and recovery are implemented, a base-v1 writable
implementation must fail closed after detecting an unclean shutdown.

## 9. Host regular-file conformance images

`tools/vibrixfs-image.rs` is a bounded host-only formatter/inspector for the
base-v1 metadata contract. It creates a **new regular file only** and refuses
paths shaped like raw devices. The initial image contains two identical
superblocks, block/inode bitmaps, the fixed inode table, root inode 1, and one
root-directory data block containing exactly `.` and `..`.

The formatter accepts an explicit 512- or 4096-byte logical-sector model; the
VibrixFS block remains 4096 bytes in either case. The inspector independently
re-reads both superblocks, validates the externally supplied root-partition GUID,
checks mandatory and padding bitmap bits, decodes root inode 1 using the wire
codec, and validates root directory record framing/padding.

This tool is deliberately limited to regular-file development images up to
1 GiB. It is **not** the M9 USB provisioning utility, does not open block
devices, does not implement crash recovery, and does not make a filesystem
persistent under the kernel. Exact CI evidence is required before any M8
roadmap checkbox changes.

## 10. Named-file conformance checkpoint (candidate)

The next host conformance increment extends the base-v1 image beyond an empty
root directory. The formatter allocates inode 2 as a regular file,
`welcome.txt`, gives it a separate allocated data block, and adds a third
root-directory record after `.` and `..`. Directory record framing is now
encoded/decoded by the shared wire codec rather than an image-tool-only helper.

The inspector independently validates the inode-bitmap allocation, parses the
directory record through the shared codec, requires its inode/type/name to match
inode 2, verifies the file extent is separately allocated and non-aliasing, and
checks the bounded file payload plus zero-filled block tail. Directory names
must be valid UTF-8, 1–255 bytes, contain no NUL or slash, and all framing,
padding, inode-number and file-type bounds fail closed.

This is still host regular-file format validation. It is not a kernel VFS,
mutable file creation API, crash-safe update protocol, USB persistence or
recovery tool. The M8 files/directories checkbox remains pending exact-head CI
for this branch.

## 11. Permissions and timestamp conformance checkpoint (QEMU-regression CI verified)

The host formatter now emits deterministic nontrivial inode metadata for the
named regular-file fixture: mode `0640`, uid/gid `1000:1000`, and distinct
atime/mtime/ctime values including sub-second nanoseconds. The root directory
retains mode `0755` and root ownership with deterministic whole-second
timestamps. The inspector does not merely print these fields: it parses the
checksummed inode record through the shared wire decoder and requires every
adopted permission/owner/time value to match before accepting the image.

The production-linked host test also asserts the exact little-endian wire
offsets for mode, uid, gid, mtime seconds and nanoseconds, followed by an
encode/decode round trip. The shared inode validator continues to reject mode
bits outside the low 12 bits and nanoseconds at or above 1,000,000,000.

This is **on-disk metadata conformance**, not credentials enforcement, access
control, wall-clock acquisition, mutation syscalls, VFS semantics or a
multi-user security boundary. [Actions run 36379311357](https://github.com/mixutin/Vibrix/actions/runs/36379311357)
proved both 512- and 4096-byte logical-sector formatter/inspector round trips
with these values, along with the full repository regression suite. The M8
permissions/timestamps checkbox therefore records the bounded on-disk metadata
contract only.
