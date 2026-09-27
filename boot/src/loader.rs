use core::ptr;
use core::slice;

use crate::elf::{ElfError, ElfInfo, LoadSegment};
use crate::uefi::{
    self, KernelFile, Status, SystemTable, EFI_LOAD_ERROR,
};

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
            Self::ElfMetadata(_) => "VIBRIX: kernel load metadata changed after validation\r\n",
            Self::InvalidRange => "VIBRIX: kernel load range is invalid\r\n",
            Self::OverlappingSegments => "VIBRIX: kernel PT_LOAD memory ranges overlap\r\n",
            Self::KernelSpanTooLarge => "VIBRIX: kernel virtual span exceeds loader policy\r\n",
            Self::Firmware(_) => "VIBRIX: firmware page allocation failed\r\n",
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

pub unsafe fn stage_kernel(
    system_table: *mut SystemTable,
    kernel_file: &KernelFile,
    info: &ElfInfo,
) -> Result<LoadedKernel, LoadError> {
    let data = kernel_file.as_slice();
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

    let physical_base =
        unsafe { uefi::allocate_loader_pages(system_table, pages) }.map_err(LoadError::Firmware)?;
    let physical_usize = match usize::try_from(physical_base) {
        Ok(address) => address,
        Err(_) => {
            unsafe { uefi::free_loader_pages(system_table, physical_base, pages) };
            return Err(LoadError::InvalidRange);
        }
    };
    let destination = physical_usize as *mut u8;

    unsafe {
        ptr::write_bytes(destination, 0, span_len);
    }

    if let Err(error) = unsafe { copy_segments(destination, virtual_base, data, info) } {
        unsafe { uefi::free_loader_pages(system_table, physical_base, pages) };
        return Err(error);
    }

    if let Err(error) = unsafe { verify_segments(destination, virtual_base, data, info) } {
        unsafe { uefi::free_loader_pages(system_table, physical_base, pages) };
        return Err(error);
    }

    Ok(LoadedKernel {
        physical_base,
        virtual_base,
        span_bytes,
        pages,
        entry: info.entry,
    })
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
        let source_len =
            usize::try_from(segment.file_size).map_err(|_| LoadError::InvalidRange)?;
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
    let staged = unsafe {
        slice::from_raw_parts(destination.add(destination_offset), file_len)
    };
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
