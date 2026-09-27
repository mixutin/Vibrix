use core::ptr;
use core::slice;

use crate::elf::{ElfError, ElfInfo, LoadSegment};
use crate::uefi::{self, EFI_LOAD_ERROR, KernelFile, Status, SystemTable};

const PAGE_SIZE: u64 = 4096;
const MAX_KERNEL_SPAN_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Clone, Copy)]
pub enum LoadError {
    ElfMetadata(ElfError),
    InvalidRange,
    OverlappingSegments,
    KernelSpanTooLarge,
    Firmware(Status),
    VerificationFailed,
}

impl LoadError {
    pub fn message(self) -> &'static str {
        match self {
            Self::ElfMetadata(error) => error.message(),
            Self::InvalidRange => "VIBRIX: kernel load range is invalid\r\n",
            Self::OverlappingSegments => "VIBRIX: kernel PT_LOAD memory ranges overlap\r\n",
            Self::KernelSpanTooLarge => "VIBRIX: kernel virtual span exceeds loader policy\r\n",
            Self::Firmware(_) => "VIBRIX: firmware page allocation or release failed\r\n",
            Self::VerificationFailed => "VIBRIX: staged kernel verification failed\r\n",
        }
    }

    pub fn status(self) -> Status {
        match self {
            Self::Firmware(status) => status,
            _ => EFI_LOAD_ERROR,
        }
    }
}

pub struct LoadedKernel {
    pub physical_base: u64,
    pub virtual_base: u64,
    pub span_bytes: u64,
    pub pages: usize,
    pub entry: u64,
}

/// Stage the validated ELF kernel into loader-owned physical pages.
///
/// # Safety
///
/// `system_table` must point to a live UEFI system table with valid Boot Services.
/// `kernel_file` and `info` must refer to the same immutable kernel image, and the
/// validated PT_LOAD metadata must remain unchanged for the duration of staging.
pub unsafe fn stage_kernel(
    system_table: *mut SystemTable,
    kernel_file: &KernelFile,
    info: &ElfInfo,
) -> Result<LoadedKernel, LoadError> {
    // SAFETY: same live-firmware and immutable-image invariants as this entry point.
    unsafe {
        stage_kernel_with(
            kernel_file.as_slice(),
            info,
            |pages| uefi::allocate_loader_pages(system_table, pages),
            |base, pages| uefi::free_loader_pages(system_table, base, pages),
        )
    }
}

// Uses the production staging algorithm with narrow allocation/release injection.
// SAFETY: returned pages must be exclusively owned, writable and directly mapped;
// the validated data/info pair must remain immutable through staging.
unsafe fn stage_kernel_with(
    data: &[u8],
    info: &ElfInfo,
    mut allocate: impl FnMut(usize) -> Result<u64, Status>,
    mut release: impl FnMut(u64, usize) -> Result<(), Status>,
) -> Result<LoadedKernel, LoadError> {
    let (virtual_base, virtual_end) = planned_span(data, info)?;
    let span_bytes = virtual_end
        .checked_sub(virtual_base)
        .ok_or(LoadError::InvalidRange)?;
    if span_bytes == 0 || span_bytes > MAX_KERNEL_SPAN_BYTES {
        return Err(LoadError::KernelSpanTooLarge);
    }

    let pages_u64 = span_bytes / PAGE_SIZE;
    let pages = usize::try_from(pages_u64).map_err(|_| LoadError::KernelSpanTooLarge)?;
    let span_len = usize::try_from(span_bytes).map_err(|_| LoadError::KernelSpanTooLarge)?;

    let physical_base = allocate(pages).map_err(LoadError::Firmware)?;
    let physical_usize = match usize::try_from(physical_base) {
        Ok(address) => address,
        Err(_) => {
            return Err(cleanup_error(
                &mut release,
                physical_base,
                pages,
                LoadError::InvalidRange,
            ));
        }
    };
    let destination = physical_usize as *mut u8;

    unsafe {
        ptr::write_bytes(destination, 0, span_len);
    }

    if let Err(error) = unsafe { copy_segments(destination, virtual_base, data, info) } {
        return Err(cleanup_error(&mut release, physical_base, pages, error));
    }

    if let Err(error) = unsafe { verify_segments(destination, virtual_base, data, info) } {
        return Err(cleanup_error(&mut release, physical_base, pages, error));
    }

    Ok(LoadedKernel {
        physical_base,
        virtual_base,
        span_bytes,
        pages,
        entry: info.entry,
    })
}

// Cleanup failure overrides the operation error: the allocation is still owned.
fn cleanup_error(
    release: &mut impl FnMut(u64, usize) -> Result<(), Status>,
    base: u64,
    pages: usize,
    original: LoadError,
) -> LoadError {
    match release(base, pages) {
        Ok(()) => original,
        Err(status) => LoadError::Firmware(status),
    }
}

fn planned_span(data: &[u8], info: &ElfInfo) -> Result<(u64, u64), LoadError> {
    let mut virtual_base = u64::MAX;
    let mut virtual_end = 0u64;
    let mut previous_end = None;
    let mut found = false;

    for segment in info.load_segments(data) {
        let segment = segment.map_err(LoadError::ElfMetadata)?;
        if segment.memory_size == 0 {
            continue;
        }

        let end = segment
            .virtual_address
            .checked_add(segment.memory_size)
            .ok_or(LoadError::InvalidRange)?;
        if previous_end.is_some_and(|previous| segment.virtual_address < previous) {
            return Err(LoadError::OverlappingSegments);
        }
        previous_end = Some(end);

        let start_page = align_down(segment.virtual_address);
        let end_page = align_up(end).ok_or(LoadError::InvalidRange)?;
        virtual_base = virtual_base.min(start_page);
        virtual_end = virtual_end.max(end_page);
        found = true;
    }

    if !found || virtual_base >= virtual_end {
        return Err(LoadError::InvalidRange);
    }

    Ok((virtual_base, virtual_end))
}

unsafe fn copy_segments(
    destination: *mut u8,
    virtual_base: u64,
    data: &[u8],
    info: &ElfInfo,
) -> Result<(), LoadError> {
    for segment in info.load_segments(data) {
        let segment = segment.map_err(LoadError::ElfMetadata)?;
        if segment.file_size == 0 {
            continue;
        }

        let source_start =
            usize::try_from(segment.file_offset).map_err(|_| LoadError::InvalidRange)?;
        let source_len = usize::try_from(segment.file_size).map_err(|_| LoadError::InvalidRange)?;
        let source_end = source_start
            .checked_add(source_len)
            .ok_or(LoadError::InvalidRange)?;
        let source = data
            .get(source_start..source_end)
            .ok_or(LoadError::InvalidRange)?;

        let destination_offset = segment
            .virtual_address
            .checked_sub(virtual_base)
            .ok_or(LoadError::InvalidRange)?;
        let destination_offset =
            usize::try_from(destination_offset).map_err(|_| LoadError::InvalidRange)?;

        unsafe {
            ptr::copy_nonoverlapping(
                source.as_ptr(),
                destination.add(destination_offset),
                source.len(),
            );
        }
    }

    Ok(())
}

unsafe fn verify_segments(
    destination: *mut u8,
    virtual_base: u64,
    data: &[u8],
    info: &ElfInfo,
) -> Result<(), LoadError> {
    for segment in info.load_segments(data) {
        let segment = segment.map_err(LoadError::ElfMetadata)?;
        verify_file_bytes(destination, virtual_base, data, segment)?;
        verify_bss(destination, virtual_base, segment)?;
    }

    Ok(())
}

fn verify_file_bytes(
    destination: *mut u8,
    virtual_base: u64,
    data: &[u8],
    segment: LoadSegment,
) -> Result<(), LoadError> {
    if segment.file_size == 0 {
        return Ok(());
    }

    let source_start = usize::try_from(segment.file_offset).map_err(|_| LoadError::InvalidRange)?;
    let file_len = usize::try_from(segment.file_size).map_err(|_| LoadError::InvalidRange)?;
    let source_end = source_start
        .checked_add(file_len)
        .ok_or(LoadError::InvalidRange)?;
    let source = data
        .get(source_start..source_end)
        .ok_or(LoadError::InvalidRange)?;

    let destination_offset = segment
        .virtual_address
        .checked_sub(virtual_base)
        .ok_or(LoadError::InvalidRange)?;
    let destination_offset =
        usize::try_from(destination_offset).map_err(|_| LoadError::InvalidRange)?;
    let staged = unsafe { slice::from_raw_parts(destination.add(destination_offset), file_len) };
    if staged != source {
        return Err(LoadError::VerificationFailed);
    }

    Ok(())
}

fn verify_bss(
    destination: *mut u8,
    virtual_base: u64,
    segment: LoadSegment,
) -> Result<(), LoadError> {
    let bss_len = segment
        .memory_size
        .checked_sub(segment.file_size)
        .ok_or(LoadError::InvalidRange)?;
    if bss_len == 0 {
        return Ok(());
    }

    let bss_offset = segment
        .virtual_address
        .checked_sub(virtual_base)
        .and_then(|offset| offset.checked_add(segment.file_size))
        .ok_or(LoadError::InvalidRange)?;
    let bss_offset = usize::try_from(bss_offset).map_err(|_| LoadError::InvalidRange)?;
    let bss_len = usize::try_from(bss_len).map_err(|_| LoadError::InvalidRange)?;
    let bss = unsafe { slice::from_raw_parts(destination.add(bss_offset), bss_len) };

    if bss.iter().any(|&byte| byte != 0) {
        return Err(LoadError::VerificationFailed);
    }

    Ok(())
}

fn align_down(address: u64) -> u64 {
    address & !(PAGE_SIZE - 1)
}

fn align_up(address: u64) -> Option<u64> {
    address
        .checked_add(PAGE_SIZE - 1)
        .map(|value| value & !(PAGE_SIZE - 1))
}

#[cfg(test)]
mod cleanup_tests {
    use super::*;
    use crate::test_support::*;
    use crate::uefi::EFI_OUT_OF_RESOURCES;

    #[test]
    fn stage_copy_failure_releases_and_prioritizes_free_status() {
        for fail in [false, true] {
            reset();
            let (mut data, info) = image();
            // Keep validated span metadata but force the defensive file-range check
            // to fail before copy. No out-of-bounds pointer access is performed.
            data[72..80].copy_from_slice(&8192u64.to_le_bytes());
            let result = unsafe {
                stage_kernel_with(
                    &data,
                    &info,
                    |pages| {
                        let base = uefi::allocate_pages_with(allocate, free, pages)?;
                        if fail {
                            FW.with_borrow_mut(|fw| {
                                fw.failed_frees.push((base, EFI_OUT_OF_RESOURCES))
                            });
                        }
                        Ok(base)
                    },
                    |base, pages| uefi::free_pages_with(free, base, pages),
                )
            };
            match result {
                Err(LoadError::Firmware(s)) if fail => assert_eq!(s, EFI_OUT_OF_RESOURCES),
                Err(LoadError::InvalidRange) if !fail => (),
                _ => panic!("wrong cleanup precedence"),
            }
            FW.with_borrow(|fw| {
                let Call::Allocate(base, 1) = fw.calls[0] else {
                    panic!("missing allocation")
                };
                assert_eq!(fw.calls, [Call::Allocate(base, 1), Call::Free(base, 1)]);
                assert_eq!(fw.allocations.len(), usize::from(fail));
            });
        }
    }

    #[test]
    fn actual_verification_failure_uses_same_cleanup_precedence() {
        for fail in [false, true] {
            reset();
            let (data, info) = image();
            let base = unsafe { uefi::allocate_pages_with(allocate, free, 1) }.unwrap();
            // Zero pages differ from the ELF file bytes. This exercises the real
            // verifier followed by the exact cleanup helper used by stage_kernel.
            let original = match unsafe {
                verify_segments(base as *mut u8, 0xffff_ffff_8000_0000, &data, &info)
            } {
                Err(error) => error,
                Ok(()) => panic!("expected verification failure"),
            };
            assert!(matches!(original, LoadError::VerificationFailed));
            if fail {
                FW.with_borrow_mut(|fw| fw.failed_frees.push((base, EFI_OUT_OF_RESOURCES)));
            }
            let error = cleanup_error(
                &mut |base, pages| unsafe { uefi::free_pages_with(free, base, pages) },
                base,
                1,
                original,
            );
            assert!(matches!(
                (fail, error),
                (true, LoadError::Firmware(EFI_OUT_OF_RESOURCES))
                    | (false, LoadError::VerificationFailed)
            ));
            FW.with_borrow(|fw| {
                assert_eq!(fw.calls, [Call::Allocate(base, 1), Call::Free(base, 1)])
            });
        }
    }

    #[test]
    fn successful_staging_retains_pages_without_release() {
        reset();
        let (data, info) = image();
        let loaded = unsafe {
            stage_kernel_with(
                &data,
                &info,
                |pages| uefi::allocate_pages_with(allocate, free, pages),
                |base, pages| uefi::free_pages_with(free, base, pages),
            )
        }
        .map_err(|e| e.message())
        .unwrap();
        FW.with_borrow(|fw| {
            assert_eq!(fw.calls, [Call::Allocate(loaded.physical_base, 1)]);
            assert_eq!(fw.allocations.len(), 1);
            assert_eq!(&fw.allocations[0].1[0].0[..256], &data[..256]);
            assert!(fw.allocations[0].1[0].0[256..].iter().all(|&b| b == 0));
        });
    }
}
