//! Single-boot-CPU allocator ownership for the first post-firmware frames.
//!
//! The loader maps and retains the entire EfiLoaderData memory-map buffer
//! before switching CR3. Its UEFI descriptors stay immutable after EBS.
//! This intentionally does not claim a general SMP-safe allocator.
pub mod frame_allocator;

use core::cell::UnsafeCell;
use core::slice;

use crate::BootInfo;
use frame_allocator::{FrameAllocator, FrameError, ReservedFrames};

const MAX_MAP_BYTES: usize = 16 * 1024 * 1024;
const PAGE_SIZE: u64 = 4096;

/// Until interrupt-driven allocation or AP startup is implemented, exactly
/// one boot CPU may enter these APIs with interrupts disabled. The only
/// mutable owner lives here, never in multiple fresh allocator instances.
struct EarlyAllocator {
    frames: FrameAllocator<'static>,
    protected: [ReservedFrames; 3],
}

struct EarlyFrameState(UnsafeCell<Option<EarlyAllocator>>);

// SAFETY: every caller is unsafe and required to be the sole CPU with IRQs
// disabled. This impl MUST be replaced with synchronization before SMP or
// asynchronous interrupt users of the physical allocator are introduced.
unsafe impl Sync for EarlyFrameState {}

static EARLY_FRAMES: EarlyFrameState = EarlyFrameState(UnsafeCell::new(None));

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EarlyFrameError {
    InvalidBootMap,
    AlreadyInitialized,
    Descriptor(FrameError),
}

/// Protect a potentially unaligned physical byte span using whole 4 KiB
/// frames, including the final partial page. Numeric only, no dereference.
fn protect_span(start: u64, len: u64) -> Result<ReservedFrames, EarlyFrameError> {
    let aligned = start & !(PAGE_SIZE - 1);
    let page_offset = start - aligned;
    let pages = page_offset
        .checked_add(len)
        .and_then(|bytes| bytes.checked_add(PAGE_SIZE - 1))
        .map(|rounded| rounded / PAGE_SIZE)
        .ok_or(EarlyFrameError::InvalidBootMap)?;
    ReservedFrames::new(aligned, pages).map_err(EarlyFrameError::Descriptor)
}

/// Initialize the *one* boot-CPU frame allocator from the retained final
/// firmware map. It does not map, zero or dereference any candidate frame.
///
/// # Safety
///
/// Must be called only once by the boot CPU with interrupts disabled, after
/// the loader has identity-mapped and retained all `memory_map_len` bytes at
/// `BootInfo::memory_map`, and after `BootInfo::validate` succeeds. The
/// EfiLoaderData map backing must remain reserved for kernel lifetime.
/// Never initialize this twice, enable IRQ allocation or share with APs.
pub unsafe fn init_from_boot_info(info: &BootInfo) -> Result<(), EarlyFrameError> {
    info.validate()
        .map_err(|_| EarlyFrameError::InvalidBootMap)?;
    let start = usize::try_from(info.memory_map).map_err(|_| EarlyFrameError::InvalidBootMap)?;
    let len = usize::try_from(info.memory_map_len).map_err(|_| EarlyFrameError::InvalidBootMap)?;
    if start == 0 || len == 0 || len > MAX_MAP_BYTES || start.checked_add(len).is_none() {
        return Err(EarlyFrameError::InvalidBootMap);
    }
    // SAFETY: the documented entry invariant covers the full loader-owned
    // final map under the active PML4; neither UEFI nor any frame consumer
    // mutates EfiLoaderData after ExitBootServices. Descriptors are only read.
    let mapped: &'static [u8] = unsafe { slice::from_raw_parts(start as *const u8, len) };
    let allocator = FrameAllocator::from_memory_map(
        mapped,
        info.memory_descriptor_size,
        info.memory_descriptor_version,
    )
    .map_err(EarlyFrameError::Descriptor)?;
    // These ranges should already be non-conventional in the UEFI map.
    // Excluding them explicitly makes ownership conservative even if a
    // firmware type annotation is surprising or the RSDP spans two pages.
    let protected = [
        protect_span(info.memory_map, info.memory_map_len)?,
        protect_span(info.framebuffer_base, info.framebuffer_size)?,
        protect_span(info.rsdp, PAGE_SIZE)?,
    ];
    // SAFETY: only the boot CPU may access this cell, with IRQs disabled.
    let state = unsafe { &mut *EARLY_FRAMES.0.get() };
    if state.is_some() {
        return Err(EarlyFrameError::AlreadyInitialized);
    }
    *state = Some(EarlyAllocator {
        frames: allocator,
        protected,
    });
    Ok(())
}

/// Claim a unique conventional RAM page as a **physical number only**.
/// The page is not mapped or zeroed. No `free` API exists.
///
/// # Safety
///
/// Only the boot CPU with interrupts disabled may call this until a
/// synchronized allocator replaces this early implementation. The caller
/// must track ownership and map/zero a page before constructing references.
pub unsafe fn allocate_frame() -> Option<u64> {
    // SAFETY: sole-boot-CPU/IRQs-off invariant described above.
    let owner = unsafe { (*EARLY_FRAMES.0.get()).as_mut()? };
    owner.frames.allocate_frame(&owner.protected)
}

/// Runtime validation of two independent, suitably aligned allocations.
/// This is a numeric and firmware-type proof, NOT a mapped-page write test.
///
/// # Safety
///
/// Same preconditions as `allocate_frame`.
pub unsafe fn smoke_claim_two_frames() -> Result<(u64, u64), EarlyFrameError> {
    let first = unsafe { allocate_frame() }.ok_or(EarlyFrameError::InvalidBootMap)?;
    let second = unsafe { allocate_frame() }.ok_or(EarlyFrameError::InvalidBootMap)?;
    if first == second
        || first == 0
        || second == 0
        || !first.is_multiple_of(PAGE_SIZE)
        || !second.is_multiple_of(PAGE_SIZE)
    {
        return Err(EarlyFrameError::InvalidBootMap);
    }
    Ok((first, second))
}
