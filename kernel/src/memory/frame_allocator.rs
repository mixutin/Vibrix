//! Early, allocation-free physical frame discovery from a copied UEFI v1 map.
//!
//! This is a monotonic EfiConventionalMemory allocator: it neither frees nor
//! maps/zeroes returned frames. Map bytes must be loader-owned and already
//! identity-mapped; this pure module never dereferences a physical address.
//! All other firmware memory types, including LoaderData, remain reserved.

const PAGE_SIZE: u64 = 4096;
const EFI_RESERVED_MEMORY: u32 = 0;
const EFI_CONVENTIONAL_MEMORY: u32 = 7;
const EFI_ACPI_RECLAIM_MEMORY: u32 = 9;
const EFI_ACPI_MEMORY_NVS: u32 = 10;
const EFI_MEMORY_MAPPED_IO: u32 = 11;
const EFI_MEMORY_UC: u64 = 1;
const EFI_MEMORY_WB: u64 = 1 << 3;
const EFI_MEMORY_RUNTIME: u64 = 1 << 63;
const DESCRIPTOR_PREFIX: usize = 40;
const MAX_MAP_BYTES: usize = 16 * 1024 * 1024;
// Pairwise overlap validation is intentionally heapless: bound its worst case.
const MAX_DESCRIPTORS: usize = 4096;
const MAX_PHYSICAL_EXCLUSIVE: u64 = 1 << 52;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameError {
    UnsupportedVersion,
    InvalidMap,
    InvalidDescriptor,
    Overlap,
    InvalidExclusion,
}

fn read_u32(bytes: &[u8], at: usize) -> u32 {
    let array: [u8; 4] = bytes[at..at + 4].try_into().expect("validated descriptor");
    u32::from_le_bytes(array)
}

fn read_u64(bytes: &[u8], at: usize) -> u64 {
    let array: [u8; 8] = bytes[at..at + 8].try_into().expect("validated descriptor");
    u64::from_le_bytes(array)
}

#[derive(Clone, Copy, Debug)]
struct Descriptor {
    kind: u32,
    start: u64,
    end: u64,
    attr: u64,
}

fn descriptor(bytes: &[u8]) -> Result<Descriptor, FrameError> {
    let kind = read_u32(bytes, 0);
    let start = read_u64(bytes, 8);
    let pages = read_u64(bytes, 24);
    let attr = read_u64(bytes, 32);
    if pages == 0 || !start.is_multiple_of(PAGE_SIZE) {
        return Err(FrameError::InvalidDescriptor);
    }
    let end = pages
        .checked_mul(PAGE_SIZE)
        .and_then(|bytes| start.checked_add(bytes))
        .filter(|&end| end <= MAX_PHYSICAL_EXCLUSIVE)
        .ok_or(FrameError::InvalidDescriptor)?;
    if kind == EFI_CONVENTIONAL_MEMORY && attr & EFI_MEMORY_RUNTIME != 0 {
        return Err(FrameError::InvalidDescriptor);
    }
    Ok(Descriptor {
        kind,
        start,
        end,
        attr,
    })
}

/// A half-open physical page range to protect from reuse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReservedFrames {
    start: u64,
    end: u64,
}

impl ReservedFrames {
    pub fn new(start: u64, pages: u64) -> Result<Self, FrameError> {
        let end = pages
            .checked_mul(PAGE_SIZE)
            .and_then(|bytes| start.checked_add(bytes))
            .ok_or(FrameError::InvalidExclusion)?;
        if pages == 0 || !start.is_multiple_of(PAGE_SIZE) || end > MAX_PHYSICAL_EXCLUSIVE {
            return Err(FrameError::InvalidExclusion);
        }
        Ok(Self { start, end })
    }

    fn contains(&self, frame: u64) -> bool {
        self.start <= frame && frame < self.end
    }
}

/// Keeps a borrow of the mapped descriptor *copy*, not of an EFI allocation
/// protocol or of raw physical memory. This object may live after firmware exit.
pub struct FrameAllocator<'a> {
    map: &'a [u8],
    stride: usize,
    next_descriptor: usize,
    next_frame: u64,
    end_frame: u64,
}

impl<'a> FrameAllocator<'a> {
    /// Validate *all* descriptors before any frame may be yielded.
    /// Only firmware descriptor format v1 is currently supported.
    pub fn from_memory_map(
        map: &'a [u8],
        descriptor_size: u64,
        descriptor_version: u32,
    ) -> Result<Self, FrameError> {
        if descriptor_version != 1 {
            return Err(FrameError::UnsupportedVersion);
        }
        let stride = usize::try_from(descriptor_size).map_err(|_| FrameError::InvalidMap)?;
        if map.is_empty()
            || map.len() > MAX_MAP_BYTES
            || stride < DESCRIPTOR_PREFIX
            || !stride.is_multiple_of(8)
            || !map.len().is_multiple_of(stride)
            || map.len() / stride > MAX_DESCRIPTORS
        {
            return Err(FrameError::InvalidMap);
        }
        // Firmware map physical allocations must be disjoint. O(n^2) uses no
        // heap and runs once, before the allocator returns a first frame.
        for (i, raw) in map.chunks_exact(stride).enumerate() {
            let current = descriptor(raw)?;
            for prior in map.chunks_exact(stride).take(i) {
                let previous = descriptor(prior)?;
                if current.start < previous.end && previous.start < current.end {
                    return Err(FrameError::Overlap);
                }
            }
        }
        Ok(Self {
            map,
            stride,
            next_descriptor: 0,
            next_frame: 0,
            end_frame: 0,
        })
    }

    /// Confirm that a physical byte span is entirely in retained firmware
    /// ACPI RAM, with WB-capable attributes and no runtime-services overlap.
    ///
    /// No physical memory is dereferenced here. The caller still needs a
    /// separate, temporary read-only mapping and must not reclaim these pages.
    /// Padding pages at either end are also required to have ACPI ownership.
    fn covers_pages(&self, start: u64, len: u64, allowed: impl Fn(Descriptor) -> bool) -> bool {
        let Some(end) = start.checked_add(len) else {
            return false;
        };
        let Some(last) = end.checked_add(PAGE_SIZE - 1) else {
            return false;
        };
        let mut cursor = start & !(PAGE_SIZE - 1);
        let aligned_end = last & !(PAGE_SIZE - 1);
        if start == 0 || len == 0 || aligned_end > MAX_PHYSICAL_EXCLUSIVE {
            return false;
        }
        while cursor < aligned_end {
            let Some(region) = self
                .map
                .chunks_exact(self.stride)
                .filter_map(|bytes| descriptor(bytes).ok())
                .find(|region| region.start <= cursor && cursor < region.end)
            else {
                return false;
            };
            if !allowed(region) {
                return false;
            }
            cursor = region.end.min(aligned_end);
        }
        true
    }

    pub fn covers_acpi_bytes(&self, start: u64, len: u64) -> bool {
        self.covers_pages(start, len, |region| {
            matches!(region.kind, EFI_ACPI_RECLAIM_MEMORY | EFI_ACPI_MEMORY_NVS)
                && region.attr & EFI_MEMORY_WB != 0
                && region.attr & EFI_MEMORY_RUNTIME == 0
        })
    }

    /// Confirm that *all page padding*, not only the bytes used by a PCI
    /// register, belongs to UC-capable reserved or MMIO memory in the final
    /// firmware map. QEMU OVMF marks its MCFG aperture type 0 (reserved) with
    /// UC capability, not type 11 (MMIO). MCFG separately proves the ECAM
    /// address and bus bounds; this rejects any conventional/loader/ACPI RAM.
    /// Numeric only: never interprets a generic reserved span as PCI MMIO.
    pub fn covers_mmio_bytes(&self, start: u64, len: u64) -> bool {
        self.covers_pages(start, len, |region| {
            matches!(region.kind, EFI_RESERVED_MEMORY | EFI_MEMORY_MAPPED_IO)
                && region.attr & EFI_MEMORY_UC != 0
                && region.attr & EFI_MEMORY_RUNTIME == 0
        })
    }

    /// Safety predicate for one page whose device identity comes from an
    /// independent hardware table/register rather than the UEFI memory map.
    ///
    /// Some firmware (including QEMU OVMF) omits architectural APIC MMIO
    /// pages from GetMemoryMap. Absence is therefore allowed, but a present
    /// descriptor must positively be non-runtime UC reserved/MMIO memory.
    /// This is only a *non-RAM/conflicting-type* check: it never proves that
    /// an arbitrary absent physical page belongs to a device.
    pub fn permits_external_mmio_page(&self, physical: u64) -> bool {
        let Some(end) = physical.checked_add(PAGE_SIZE) else {
            return false;
        };
        if physical == 0 || !physical.is_multiple_of(PAGE_SIZE) || end > MAX_PHYSICAL_EXCLUSIVE {
            return false;
        }
        let region = self
            .map
            .chunks_exact(self.stride)
            .filter_map(|bytes| descriptor(bytes).ok())
            .find(|region| region.start <= physical && physical < region.end);
        match region {
            None => true,
            Some(region) => {
                region.end >= end
                    && matches!(region.kind, EFI_RESERVED_MEMORY | EFI_MEMORY_MAPPED_IO)
                    && region.attr & EFI_MEMORY_UC != 0
                    && region.attr & EFI_MEMORY_RUNTIME == 0
            }
        }
    }

    /// Diagnostic value only: never dereferences a physical address.
    pub fn descriptor_at(&self, physical: u64) -> Option<(u32, u64)> {
        self.map
            .chunks_exact(self.stride)
            .filter_map(|bytes| descriptor(bytes).ok())
            .find(|region| region.start <= physical && physical < region.end)
            .map(|region| (region.kind, region.attr))
    }

    /// Return a newly claimed 4 KiB physical frame or None on exhaustion.
    /// The caller must map/zero it before accessing it, and must keep it
    /// reserved; this early monotonic allocator does NOT support free().
    pub fn allocate_frame(&mut self, exclusions: &[ReservedFrames]) -> Option<u64> {
        loop {
            while self.next_frame >= self.end_frame {
                let raw = self
                    .map
                    .chunks_exact(self.stride)
                    .nth(self.next_descriptor)?;
                self.next_descriptor += 1;
                // Validated at initialization; none of the map is mutable.
                let entry = descriptor(raw).ok()?;
                if entry.kind != EFI_CONVENTIONAL_MEMORY {
                    continue;
                }
                let _firmware_attributes = entry.attr;
                self.next_frame = entry.start.max(PAGE_SIZE); // never page zero
                self.end_frame = entry.end;
            }
            let candidate = self.next_frame;
            self.next_frame += PAGE_SIZE; // validated <= 2^52, no overflow
            if exclusions
                .iter()
                .all(|reserved| !reserved.contains(candidate))
            {
                return Some(candidate);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STRIDE: usize = 48;
    fn raw(kind: u32, start: u64, pages: u64, attr: u64) -> [u8; STRIDE] {
        let mut result = [0x5a; STRIDE]; // test padded firmware descriptor
        result[0..4].copy_from_slice(&kind.to_le_bytes());
        result[8..16].copy_from_slice(&start.to_le_bytes());
        result[24..32].copy_from_slice(&pages.to_le_bytes());
        result[32..40].copy_from_slice(&attr.to_le_bytes());
        result
    }

    fn sample() -> Vec<u8> {
        [
            raw(2, 0x1000, 2, 0),
            raw(7, 0x3000, 3, 0),
            raw(7, 0x8000, 2, 0),
        ]
        .concat()
    }

    #[test]
    fn only_conventional_frames_are_allocated_without_repeats() {
        let bytes = sample();
        let mut allocator = FrameAllocator::from_memory_map(&bytes, 48, 1).unwrap();
        let mut results = Vec::new();
        while let Some(frame) = allocator.allocate_frame(&[]) {
            results.push(frame);
        }
        assert_eq!(results, [0x3000, 0x4000, 0x5000, 0x8000, 0x9000]);
        assert!(allocator.allocate_frame(&[]).is_none());
    }

    #[test]
    fn exclusions_and_zero_frame_are_never_yielded() {
        let bytes = [raw(7, 0, 4, 0), raw(7, 0x8000, 2, 0)].concat();
        let reserve = [ReservedFrames::new(0x1000, 2).unwrap()];
        let mut allocator = FrameAllocator::from_memory_map(&bytes, 48, 1).unwrap();
        assert_eq!(allocator.allocate_frame(&reserve), Some(0x3000));
        assert_eq!(allocator.allocate_frame(&reserve), Some(0x8000));
        assert_eq!(allocator.allocate_frame(&reserve), Some(0x9000));
        assert_eq!(allocator.allocate_frame(&reserve), None);
    }

    #[test]
    fn unsupported_versions_stride_length_and_empty_maps_fail() {
        let bytes = sample();
        assert!(matches!(
            FrameAllocator::from_memory_map(&bytes, 48, 0),
            Err(FrameError::UnsupportedVersion)
        ));
        for stride in [0, 32, 41, 40] {
            assert!(matches!(
                FrameAllocator::from_memory_map(&bytes, stride, 1),
                Err(FrameError::InvalidMap)
            ));
        }
        assert!(matches!(
            FrameAllocator::from_memory_map(&[], 48, 1),
            Err(FrameError::InvalidMap)
        ));
        // Reject adversarial oversized maps before the quadratic overlap
        // check, even when byte length is below the 16 MiB buffer limit.
        let excessive = vec![0u8; (MAX_DESCRIPTORS + 1) * STRIDE];
        assert!(matches!(
            FrameAllocator::from_memory_map(&excessive, STRIDE as u64, 1),
            Err(FrameError::InvalidMap)
        ));
    }

    #[test]
    fn reject_overlapping_or_overflowing_firmware_ranges() {
        let overlap = [raw(7, 0x1000, 3, 0), raw(2, 0x3000, 1, 0)].concat();
        assert!(matches!(
            FrameAllocator::from_memory_map(&overlap, 48, 1),
            Err(FrameError::Overlap)
        ));
        for broken in [
            raw(7, 1, 1, 0),
            raw(7, 0x1000, 0, 0),
            raw(7, MAX_PHYSICAL_EXCLUSIVE - PAGE_SIZE, 2, 0),
            raw(7, 0x1000, 1, EFI_MEMORY_RUNTIME),
        ] {
            assert!(matches!(
                FrameAllocator::from_memory_map(&broken, 48, 1),
                Err(FrameError::InvalidDescriptor)
            ));
        }
    }

    #[test]
    fn validates_readonly_firmware_acpi_spans_and_padding_pages() {
        let raw_map = [
            raw(9, 0x1000, 2, EFI_MEMORY_WB),
            raw(10, 0x3000, 2, EFI_MEMORY_WB),
            raw(7, 0x5000, 2, EFI_MEMORY_WB),
        ]
        .concat();
        let frames = FrameAllocator::from_memory_map(&raw_map, 48, 1).unwrap();
        assert!(frames.covers_acpi_bytes(0x1fff, 0x2001));
        assert!(frames.covers_acpi_bytes(0x4000, 0x1000));
        assert!(!frames.covers_acpi_bytes(0x1fff, 0x3002));
        assert!(!frames.covers_acpi_bytes(0x1000, 0));
        assert!(!frames.covers_acpi_bytes(0, 0x1000));
        assert!(!frames.covers_acpi_bytes(u64::MAX - 16, 32));
        assert!(!frames.covers_acpi_bytes(0x1000, 1u64 << 52));

        for kind in [2, 4, 7, 11] {
            let bad = raw(kind, 0x1000, 1, EFI_MEMORY_WB);
            let owner = FrameAllocator::from_memory_map(&bad, 48, 1).unwrap();
            assert!(!owner.covers_acpi_bytes(0x1000, 1));
        }
        for attr in [0, EFI_MEMORY_RUNTIME | EFI_MEMORY_WB] {
            let bad = raw(9, 0x1000, 1, attr);
            let owner = FrameAllocator::from_memory_map(&bad, 48, 1).unwrap();
            assert!(!owner.covers_acpi_bytes(0x1000, 1));
        }
    }

    #[test]
    fn mmio_must_be_firmware_described_uncached_and_non_runtime() {
        let map = [
            raw(11, 0xe000_0000, 2, EFI_MEMORY_UC),
            raw(11, 0xe000_2000, 1, EFI_MEMORY_UC),
            raw(9, 0xe000_3000, 1, EFI_MEMORY_WB),
        ]
        .concat();
        let frames = FrameAllocator::from_memory_map(&map, 48, 1).unwrap();
        assert!(frames.covers_mmio_bytes(0xe000_0ffc, 8));
        let reserved = raw(0, 0xe000_0000, 1, EFI_MEMORY_UC);
        let owner = FrameAllocator::from_memory_map(&reserved, 48, 1).unwrap();
        assert!(owner.covers_mmio_bytes(0xe000_0000, PAGE_SIZE));
        assert!(frames.covers_mmio_bytes(0xe000_2000, 0x1000));
        assert!(!frames.covers_mmio_bytes(0xe000_2ffc, 8));
        assert!(!frames.covers_mmio_bytes(0xe000_3000, 4));
        assert!(!frames.covers_mmio_bytes(0, 4096));
        assert!(!frames.covers_mmio_bytes(u64::MAX, 4));
        for (kind, attrs) in [
            (7, EFI_MEMORY_UC),
            (0, 0),
            (0, EFI_MEMORY_WB),
            (0, EFI_MEMORY_UC | EFI_MEMORY_RUNTIME),
            (11, 0),
            (11, EFI_MEMORY_WB),
            (11, EFI_MEMORY_UC | EFI_MEMORY_RUNTIME),
            (12, EFI_MEMORY_UC),
        ] {
            let invalid = raw(kind, 0xe000_0000, 1, attrs);
            let owner = FrameAllocator::from_memory_map(&invalid, 48, 1).unwrap();
            assert!(!owner.covers_mmio_bytes(0xe000_0000, 4));
        }
    }

    #[test]
    fn externally_identified_mmio_may_be_unlisted_but_never_ram_or_runtime() {
        let ram = raw(7, 0x1000, 1, EFI_MEMORY_WB);
        let frames = FrameAllocator::from_memory_map(&ram, 48, 1).unwrap();
        assert!(frames.permits_external_mmio_page(0xfec0_0000));
        assert!(!frames.permits_external_mmio_page(0x1000));
        assert!(!frames.permits_external_mmio_page(0));
        assert!(!frames.permits_external_mmio_page(0xfec0_0001));
        assert!(!frames.permits_external_mmio_page(u64::MAX & !(PAGE_SIZE - 1)));

        for (kind, attrs, expected) in [
            (0, EFI_MEMORY_UC, true),
            (11, EFI_MEMORY_UC, true),
            (0, EFI_MEMORY_UC | EFI_MEMORY_RUNTIME, false),
            (11, EFI_MEMORY_WB, false),
            (9, EFI_MEMORY_WB, false),
            (2, EFI_MEMORY_UC, false),
            (7, EFI_MEMORY_UC, false),
        ] {
            let raw = raw(kind, 0xfec0_0000, 1, attrs);
            let owner = FrameAllocator::from_memory_map(&raw, 48, 1).unwrap();
            assert_eq!(
                owner.permits_external_mmio_page(0xfec0_0000),
                expected,
                "kind={kind} attrs={attrs:#x}"
            );
        }
    }

    #[test]
    fn rejects_bad_exclusion_bounds() {
        assert_eq!(ReservedFrames::new(1, 2), Err(FrameError::InvalidExclusion));
        assert_eq!(
            ReservedFrames::new(0x1000, 0),
            Err(FrameError::InvalidExclusion)
        );
        assert_eq!(
            ReservedFrames::new(0x1000, u64::MAX),
            Err(FrameError::InvalidExclusion)
        );
    }
}
