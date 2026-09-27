//! Early, allocation-free physical frame discovery from a copied UEFI v1 map.
//!
//! This is a monotonic EfiConventionalMemory allocator: it neither frees nor
//! maps/zeroes returned frames. Map bytes must be loader-owned and already
//! identity-mapped; this pure module never dereferences a physical address.
//! All other firmware memory types, including LoaderData, remain reserved.

const PAGE_SIZE: u64 = 4096;
const EFI_CONVENTIONAL_MEMORY: u32 = 7;
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
