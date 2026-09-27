//! Physical frame allocator for x86-64.
//!
//! A simple bitmap-based allocator that tracks which 4 KiB physical frames
//! are free or in use. Initialized from the UEFI memory map (passed via
//! BootInfo) once the kernel has access to it.
//!
//! This is a bootstrap allocator — it's not fast, but it's correct and
//! has no external dependencies. A more sophisticated allocator (buddy,
//! slab, etc.) can replace it later.

use core::sync::atomic::{AtomicU64, Ordering};

/// Size of a physical frame in bytes (4 KiB).
pub const FRAME_SIZE: usize = 4096;

/// Maximum number of frames we can track (512 * 64 = 32768 frames = 128 MiB).
const MAX_BITMAP_WORDS: usize = 512;
const FRAMES_PER_WORD: usize = 64;

/// A simple bitmap-based physical frame allocator.
///
/// Tracks frame allocation state in a static bitmap. Not thread-safe —
/// the kernel must provide external synchronization (or use it only
/// during single-threaded bootstrap).
pub struct FrameAllocator {
    bitmap: [AtomicU64; MAX_BITMAP_WORDS],
    first_frame: AtomicU64,
    total_frames: AtomicU64,
}

/// Errors returned by the frame allocator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameAllocError {
    OutOfMemory,
    OutOfRange,
    AlreadyAllocated,
    AlreadyFree,
}

impl core::fmt::Display for FrameAllocError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            FrameAllocError::OutOfMemory => write!(f, "no free frames available"),
            FrameAllocError::OutOfRange => write!(f, "frame number out of range"),
            FrameAllocError::AlreadyAllocated => write!(f, "frame already allocated"),
            FrameAllocError::AlreadyFree => write!(f, "frame already free"),
        }
    }
}

impl FrameAllocator {
    /// Create a new, empty frame allocator.
    pub const fn new() -> Self {
        Self {
            bitmap: [const { AtomicU64::new(u64::MAX) }; MAX_BITMAP_WORDS],
            first_frame: AtomicU64::new(0),
            total_frames: AtomicU64::new(0),
        }
    }

    /// Initialize the allocator with a free region of physical memory.
    pub fn init_free_region(&self, start: u64, len: u64) {
        assert!(start % FRAME_SIZE as u64 == 0, "start must be page-aligned");
        assert!(len % FRAME_SIZE as u64 == 0, "len must be page-aligned");

        let start_frame = start / FRAME_SIZE as u64;
        let num_frames = len / FRAME_SIZE as u64;

        self.first_frame.store(start_frame, Ordering::Relaxed);
        self.total_frames.store(num_frames, Ordering::Relaxed);

        for frame in start_frame..start_frame + num_frames {
            self.clear_bit(frame);
        }
    }

    /// Allocate a single physical frame.
    pub fn alloc(&self) -> Result<u64, FrameAllocError> {
        let first = self.first_frame.load(Ordering::Relaxed);
        let total = self.total_frames.load(Ordering::Relaxed);

        for word_idx in 0..MAX_BITMAP_WORDS {
            let word = self.bitmap[word_idx].load(Ordering::Relaxed);
            if word == 0 {
                continue;
            }

            for bit_idx in 0..FRAMES_PER_WORD {
                let bit = 1u64 << bit_idx;
                if word & bit == 0 {
                    let frame = (word_idx * FRAMES_PER_WORD + bit_idx) as u64;
                    if frame < first || frame >= first + total {
                        continue;
                    }

                    let old = self.bitmap[word_idx].fetch_or(bit, Ordering::Relaxed);
                    if old & bit == 0 {
                        return Ok(frame * FRAME_SIZE as u64);
                    }
                }
            }
        }

        Err(FrameAllocError::OutOfMemory)
    }

    /// Free a previously allocated physical frame.
    pub fn free(&self, addr: u64) -> Result<(), FrameAllocError> {
        assert!(addr % FRAME_SIZE as u64 == 0, "address must be page-aligned");

        let frame = addr / FRAME_SIZE as u64;
        let first = self.first_frame.load(Ordering::Relaxed);
        let total = self.total_frames.load(Ordering::Relaxed);

        if frame < first || frame >= first + total {
            return Err(FrameAllocError::OutOfRange);
        }

        let word_idx = (frame / FRAMES_PER_WORD as u64) as usize;
        let bit_idx = (frame % FRAMES_PER_WORD as u64) as usize;
        let bit = 1u64 << bit_idx;

        let old = self.bitmap[word_idx].fetch_and(!bit, Ordering::Relaxed);
        if old & bit == 0 {
            return Err(FrameAllocError::AlreadyFree);
        }

        Ok(())
    }

    /// Check if a frame is allocated.
    pub fn is_allocated(&self, addr: u64) -> bool {
        if addr % FRAME_SIZE as u64 != 0 {
            return false;
        }

        let frame = addr / FRAME_SIZE as u64;
        let first = self.first_frame.load(Ordering::Relaxed);
        let total = self.total_frames.load(Ordering::Relaxed);

        if frame < first || frame >= first + total {
            return false;
        }

        let word_idx = (frame / FRAMES_PER_WORD as u64) as usize;
        let bit_idx = (frame % FRAMES_PER_WORD as u64) as usize;
        let bit = 1u64 << bit_idx;

        self.bitmap[word_idx].load(Ordering::Relaxed) & bit != 0
    }

    /// Get the total number of frames.
    pub fn total_frames(&self) -> u64 {
        self.total_frames.load(Ordering::Relaxed)
    }

    /// Get the number of free frames.
    pub fn free_frames(&self) -> u64 {
        let mut free = 0;
        for word_idx in 0..MAX_BITMAP_WORDS {
            let word = self.bitmap[word_idx].load(Ordering::Relaxed);
            free += word.count_ones() as u64;
        }
        self.total_frames.load(Ordering::Relaxed).saturating_sub(free)
    }

    fn clear_bit(&self, frame: u64) {
        let word_idx = (frame / FRAMES_PER_WORD as u64) as usize;
        let bit_idx = (frame % FRAMES_PER_WORD as u64) as usize;
        let bit = 1u64 << bit_idx;
        self.bitmap[word_idx].fetch_and(!bit, Ordering::Relaxed);
    }
}

/// Global frame allocator instance.
pub static FRAME_ALLOCATOR: FrameAllocator = FrameAllocator::new();

/// Initialize the global frame allocator.
pub fn init(start: u64, len: u64) {
    FRAME_ALLOCATOR.init_free_region(start, len);
}

/// Allocate a physical frame.
pub fn alloc() -> Result<u64, FrameAllocError> {
    FRAME_ALLOCATOR.alloc()
}

/// Free a physical frame.
pub fn free(addr: u64) -> Result<(), FrameAllocError> {
    FRAME_ALLOCATOR.free(addr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_size() {
        assert_eq!(FRAME_SIZE, 4096);
    }

    #[test]
    fn test_alloc_and_free() {
        let alloc = FrameAllocator::new();
        alloc.init_free_region(0x100000, 0x100000);

        let addr = alloc.alloc().expect("should allocate");
        assert_eq!(addr % FRAME_SIZE as u64, 0);
        assert!(alloc.is_allocated(addr));

        alloc.free(addr).expect("should free");
        assert!(!alloc.is_allocated(addr));
    }

    #[test]
    fn test_out_of_memory() {
        let alloc = FrameAllocator::new();
        alloc.init_free_region(0x100000, FRAME_SIZE as u64);

        let _ = alloc.alloc().expect("should allocate");
        assert_eq!(alloc.alloc(), Err(FrameAllocError::OutOfMemory));
    }

    #[test]
    fn test_double_free() {
        let alloc = FrameAllocator::new();
        alloc.init_free_region(0x100000, FRAME_SIZE as u64);

        let addr = alloc.alloc().expect("should allocate");
        alloc.free(addr).expect("should free");
        assert_eq!(alloc.free(addr), Err(FrameAllocError::AlreadyFree));
    }

    #[test]
    fn test_free_frames_count() {
        let alloc = FrameAllocator::new();
        alloc.init_free_region(0x100000, 0x100000);

        assert_eq!(alloc.free_frames(), 256);

        let addr = alloc.alloc().expect("should allocate");
        assert_eq!(alloc.free_frames(), 255);

        alloc.free(addr).expect("should free");
        assert_eq!(alloc.free_frames(), 256);
    }
}
