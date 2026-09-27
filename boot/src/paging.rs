use core::ptr;

use crate::elf::{ElfError, ElfInfo};
use crate::loader::LoadedKernel;
use crate::uefi::{self, EFI_LOAD_ERROR, KernelFile, Status, SystemTable};

const PAGE_SIZE: u64 = 4096;
const ENTRIES_PER_TABLE: usize = 512;
const ADDRESS_MASK: u64 = 0x000f_ffff_ffff_f000;
const PRESENT: u64 = 1 << 0;
const WRITABLE: u64 = 1 << 1;
const USER: u64 = 1 << 2;
const HUGE_PAGE: u64 = 1 << 7;
const NO_EXECUTE: u64 = 1 << 63;
const MAX_TABLE_PAGES: usize = 256;

#[derive(Clone, Copy)]
pub enum PagingError {
    ElfMetadata(ElfError),
    InvalidVirtualAddress,
    InvalidPhysicalAddress,
    KernelSpanMismatch,
    TooManyTablePages,
    Firmware(Status),
    HugePageConflict,
    MappingConflict,
    VerificationFailed,
}

impl PagingError {
    pub fn message(self) -> &'static str {
        match self {
            Self::ElfMetadata(error) => error.message(),
            Self::InvalidVirtualAddress => "VIBRIX: kernel mapping virtual address invalid\r\n",
            Self::InvalidPhysicalAddress => "VIBRIX: kernel mapping physical address invalid\r\n",
            Self::KernelSpanMismatch => "VIBRIX: kernel mapping exceeds staged span\r\n",
            Self::TooManyTablePages => "VIBRIX: kernel page-table allocation limit exceeded\r\n",
            Self::Firmware(_) => "VIBRIX: firmware page-table allocation or release failed\r\n",
            Self::HugePageConflict => "VIBRIX: unexpected huge-page entry in new page tables\r\n",
            Self::MappingConflict => "VIBRIX: conflicting kernel page mapping\r\n",
            Self::VerificationFailed => "VIBRIX: kernel page-table verification failed\r\n",
        }
    }

    pub fn status(self) -> Status {
        match self {
            Self::Firmware(status) => status,
            _ => EFI_LOAD_ERROR,
        }
    }
}

pub struct KernelPageTables {
    pub root_physical: u64,
    pub table_pages: usize,
    pub mapped_pages: usize,
}

struct TableAllocator {
    allocate: uefi::AllocatePages,
    free: uefi::FreePages,
    pages: [u64; MAX_TABLE_PAGES],
    count: usize,
}

impl TableAllocator {
    fn new(allocate: uefi::AllocatePages, free: uefi::FreePages) -> Self {
        Self {
            allocate,
            free,
            pages: [0; MAX_TABLE_PAGES],
            count: 0,
        }
    }

    unsafe fn allocate_zeroed(&mut self) -> Result<u64, PagingError> {
        if self.count == MAX_TABLE_PAGES {
            return Err(PagingError::TooManyTablePages);
        }

        let physical = unsafe { uefi::allocate_pages_with(self.allocate, self.free, 1) }
            .map_err(PagingError::Firmware)?;
        let slot = self.count;
        self.pages[slot] = physical;
        self.count += 1;
        let address = usize::try_from(physical)
            .ok()
            .filter(|_| valid_physical_page(physical));
        let Some(address) = address else {
            unsafe { uefi::free_pages_with(self.free, physical, 1) }
                .map_err(PagingError::Firmware)?;
            self.pages[slot] = 0;
            self.count -= 1;
            return Err(PagingError::InvalidPhysicalAddress);
        };

        unsafe {
            ptr::write_bytes(address as *mut u8, 0, PAGE_SIZE as usize);
        }
        Ok(physical)
    }

    // Reverse allocation order, attempt every page. The first release failure in
    // that order wins. Clear only successful slots; compact failed ownership for
    // inspection or an explicit retry. These tables are inactive throughout.
    unsafe fn release_all(&mut self) -> Result<(), Status> {
        let mut first_failure = None;
        for slot in (0..self.count).rev() {
            let physical = self.pages[slot];
            match unsafe { uefi::free_pages_with(self.free, physical, 1) } {
                Ok(()) => self.pages[slot] = 0,
                Err(status) => {
                    first_failure.get_or_insert(status);
                }
            }
        }
        let mut retained = 0;
        for slot in 0..self.count {
            if self.pages[slot] != 0 {
                self.pages[retained] = self.pages[slot];
                retained += 1;
            }
        }
        self.pages[retained..self.count].fill(0);
        self.count = retained;
        first_failure.map_or(Ok(()), Err)
    }
}

/// Build a new four-level x86-64 page-table hierarchy for the staged kernel.
///
/// The returned tables are not activated. They contain only supervisor 4 KiB
/// mappings for nonempty PT_LOAD memory ranges. Writable and executable
/// permissions come directly from ELF flags.
///
/// # Safety
///
/// The caller must provide a live UEFI system table and a LoadedKernel staged
/// from the same immutable kernel_file/info pair. Firmware page allocations
/// must remain directly accessible in the current pre-ExitBootServices address
/// space, matching the invariant already used by the staging loader.
pub unsafe fn build_kernel_page_tables(
    system_table: *mut SystemTable,
    kernel_file: &KernelFile,
    info: &ElfInfo,
    loaded: &LoadedKernel,
) -> Result<KernelPageTables, PagingError> {
    validate_loaded_span(loaded)?;

    let (allocate, free) =
        unsafe { uefi::page_services(system_table) }.map_err(PagingError::Firmware)?;
    let mut allocator = TableAllocator::new(allocate, free);
    unsafe { build_with_allocator(&mut allocator, kernel_file.as_slice(), info, loaded) }
}

// SAFETY: same live allocations/image invariants as the public entry point.
unsafe fn build_with_allocator(
    allocator: &mut TableAllocator,
    data: &[u8],
    info: &ElfInfo,
    loaded: &LoadedKernel,
) -> Result<KernelPageTables, PagingError> {
    let result = unsafe { build_inner(allocator, data, info, loaded) };

    match result {
        Ok((root_physical, mapped_pages)) => Ok(KernelPageTables {
            root_physical,
            table_pages: allocator.count,
            mapped_pages,
        }),
        Err(error) => {
            // A failed rollback aborts boot. Tracking remains truthful while this
            // allocator lives, but does not escape this function; this error does
            // not promise full reclamation or a recoverable ownership transfer.
            unsafe { allocator.release_all() }.map_err(PagingError::Firmware)?;
            Err(error)
        }
    }
}

unsafe fn build_inner(
    allocator: &mut TableAllocator,
    data: &[u8],
    info: &ElfInfo,
    loaded: &LoadedKernel,
) -> Result<(u64, usize), PagingError> {
    let root = unsafe { allocator.allocate_zeroed()? };
    let mut mapped_pages = 0usize;

    for segment in info.load_segments(data) {
        let segment = segment.map_err(PagingError::ElfMetadata)?;
        if segment.memory_size == 0 {
            continue;
        }

        let end = segment
            .virtual_address
            .checked_add(segment.memory_size)
            .ok_or(PagingError::InvalidVirtualAddress)?;
        let mut virtual_page = align_down(segment.virtual_address);
        let end_page = align_up(end).ok_or(PagingError::InvalidVirtualAddress)?;
        let span_end = loaded
            .virtual_base
            .checked_add(loaded.span_bytes)
            .ok_or(PagingError::KernelSpanMismatch)?;

        if virtual_page < loaded.virtual_base || end_page > span_end {
            return Err(PagingError::KernelSpanMismatch);
        }

        while virtual_page < end_page {
            let offset = virtual_page
                .checked_sub(loaded.virtual_base)
                .ok_or(PagingError::KernelSpanMismatch)?;
            let physical_page = loaded
                .physical_base
                .checked_add(offset)
                .ok_or(PagingError::InvalidPhysicalAddress)?;

            if unsafe {
                map_page(
                    allocator,
                    root,
                    virtual_page,
                    physical_page,
                    segment.writable(),
                    segment.executable(),
                )?
            } {
                mapped_pages = mapped_pages
                    .checked_add(1)
                    .ok_or(PagingError::VerificationFailed)?;
            }

            virtual_page = virtual_page
                .checked_add(PAGE_SIZE)
                .ok_or(PagingError::InvalidVirtualAddress)?;
        }
    }

    if mapped_pages == 0 {
        return Err(PagingError::VerificationFailed);
    }

    unsafe { verify_kernel_mappings(root, data, info, loaded)? };
    Ok((root, mapped_pages))
}

unsafe fn map_page(
    allocator: &mut TableAllocator,
    root: u64,
    virtual_page: u64,
    physical_page: u64,
    writable: bool,
    executable: bool,
) -> Result<bool, PagingError> {
    if !valid_canonical_page(virtual_page) {
        return Err(PagingError::InvalidVirtualAddress);
    }
    if !valid_physical_page(physical_page) {
        return Err(PagingError::InvalidPhysicalAddress);
    }

    let [pml4_index, pdpt_index, pd_index, pt_index] = indices(virtual_page);
    let pdpt = unsafe { ensure_next_table(allocator, root, pml4_index)? };
    let pd = unsafe { ensure_next_table(allocator, pdpt, pdpt_index)? };
    let pt = unsafe { ensure_next_table(allocator, pd, pd_index)? };

    let mut flags = PRESENT;
    if writable {
        flags |= WRITABLE;
    }
    if !executable {
        flags |= NO_EXECUTE;
    }

    let desired = physical_page | flags;
    let current = unsafe { read_entry(pt, pt_index)? };
    if current == 0 {
        unsafe { write_entry(pt, pt_index, desired)? };
        return Ok(true);
    }
    if current != desired {
        return Err(PagingError::MappingConflict);
    }
    Ok(false)
}

unsafe fn ensure_next_table(
    allocator: &mut TableAllocator,
    table: u64,
    index: usize,
) -> Result<u64, PagingError> {
    let current = unsafe { read_entry(table, index)? };
    if current & PRESENT != 0 {
        if current & HUGE_PAGE != 0 {
            return Err(PagingError::HugePageConflict);
        }
        let physical = current & ADDRESS_MASK;
        if !valid_physical_page(physical) {
            return Err(PagingError::InvalidPhysicalAddress);
        }
        return Ok(physical);
    }

    let next = unsafe { allocator.allocate_zeroed()? };
    unsafe { write_entry(table, index, next | PRESENT | WRITABLE)? };
    Ok(next)
}

unsafe fn verify_kernel_mappings(
    root: u64,
    data: &[u8],
    info: &ElfInfo,
    loaded: &LoadedKernel,
) -> Result<(), PagingError> {
    for segment in info.load_segments(data) {
        let segment = segment.map_err(PagingError::ElfMetadata)?;
        if segment.memory_size == 0 {
            continue;
        }

        let end = segment
            .virtual_address
            .checked_add(segment.memory_size)
            .ok_or(PagingError::InvalidVirtualAddress)?;
        let mut virtual_page = align_down(segment.virtual_address);
        let end_page = align_up(end).ok_or(PagingError::InvalidVirtualAddress)?;

        while virtual_page < end_page {
            let offset = virtual_page
                .checked_sub(loaded.virtual_base)
                .ok_or(PagingError::KernelSpanMismatch)?;
            let expected_physical = loaded
                .physical_base
                .checked_add(offset)
                .ok_or(PagingError::InvalidPhysicalAddress)?;
            let leaf = unsafe { walk_leaf(root, virtual_page)? };

            if leaf & ADDRESS_MASK != expected_physical
                || (leaf & WRITABLE != 0) != segment.writable()
                || (leaf & NO_EXECUTE == 0) != segment.executable()
                || leaf & USER != 0
            {
                return Err(PagingError::VerificationFailed);
            }

            virtual_page = virtual_page
                .checked_add(PAGE_SIZE)
                .ok_or(PagingError::InvalidVirtualAddress)?;
        }
    }

    Ok(())
}

unsafe fn walk_leaf(root: u64, virtual_page: u64) -> Result<u64, PagingError> {
    let [pml4_index, pdpt_index, pd_index, pt_index] = indices(virtual_page);
    let mut table = root;

    for index in [pml4_index, pdpt_index, pd_index] {
        let entry = unsafe { read_entry(table, index)? };
        if entry & PRESENT == 0 {
            return Err(PagingError::VerificationFailed);
        }
        if entry & HUGE_PAGE != 0 {
            return Err(PagingError::HugePageConflict);
        }
        table = entry & ADDRESS_MASK;
        if !valid_physical_page(table) {
            return Err(PagingError::InvalidPhysicalAddress);
        }
    }

    let leaf = unsafe { read_entry(table, pt_index)? };
    if leaf & PRESENT == 0 {
        return Err(PagingError::VerificationFailed);
    }
    Ok(leaf)
}

unsafe fn read_entry(table: u64, index: usize) -> Result<u64, PagingError> {
    if index >= ENTRIES_PER_TABLE || !valid_physical_page(table) {
        return Err(PagingError::InvalidPhysicalAddress);
    }
    let address = usize::try_from(table).map_err(|_| PagingError::InvalidPhysicalAddress)?;
    Ok(unsafe { ptr::read((address as *const u64).add(index)) })
}

unsafe fn write_entry(table: u64, index: usize, value: u64) -> Result<(), PagingError> {
    if index >= ENTRIES_PER_TABLE || !valid_physical_page(table) {
        return Err(PagingError::InvalidPhysicalAddress);
    }
    let address = usize::try_from(table).map_err(|_| PagingError::InvalidPhysicalAddress)?;
    unsafe {
        ptr::write((address as *mut u64).add(index), value);
    }
    Ok(())
}

fn validate_loaded_span(loaded: &LoadedKernel) -> Result<(), PagingError> {
    if !valid_canonical_page(loaded.virtual_base)
        || !valid_physical_page(loaded.physical_base)
        || loaded.span_bytes == 0
        || !loaded.span_bytes.is_multiple_of(PAGE_SIZE)
        || loaded.pages as u64 != loaded.span_bytes / PAGE_SIZE
    {
        return Err(PagingError::KernelSpanMismatch);
    }

    let virtual_end = loaded
        .virtual_base
        .checked_add(loaded.span_bytes)
        .ok_or(PagingError::KernelSpanMismatch)?;
    let physical_end = loaded
        .physical_base
        .checked_add(loaded.span_bytes)
        .ok_or(PagingError::KernelSpanMismatch)?;

    if virtual_end <= loaded.virtual_base
        || physical_end <= loaded.physical_base
        || !is_canonical_48(virtual_end - 1)
        || physical_end - 1 > ADDRESS_MASK | (PAGE_SIZE - 1)
    {
        return Err(PagingError::KernelSpanMismatch);
    }

    Ok(())
}

fn valid_canonical_page(address: u64) -> bool {
    address.is_multiple_of(PAGE_SIZE) && is_canonical_48(address)
}

fn valid_physical_page(address: u64) -> bool {
    address != 0 && address.is_multiple_of(PAGE_SIZE) && address & !ADDRESS_MASK == 0
}

fn is_canonical_48(address: u64) -> bool {
    let upper = address >> 48;
    if address & (1 << 47) == 0 {
        upper == 0
    } else {
        upper == 0xffff
    }
}

fn indices(address: u64) -> [usize; 4] {
    [
        ((address >> 39) & 0x1ff) as usize,
        ((address >> 30) & 0x1ff) as usize,
        ((address >> 21) & 0x1ff) as usize,
        ((address >> 12) & 0x1ff) as usize,
    ]
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
    use crate::uefi::{EFI_INVALID_PARAMETER, EFI_OUT_OF_RESOURCES};

    fn loaded() -> LoadedKernel {
        LoadedKernel {
            physical_base: 0x20_0000,
            virtual_base: 0xffff_ffff_8000_0000,
            span_bytes: 4096,
            pages: 1,
            entry: 0xffff_ffff_8000_0000,
        }
    }

    #[test]
    fn rollback_tries_every_page_preserves_failed_ownership_and_first_error() {
        reset();
        let mut allocator = TableAllocator::new(allocate, free);
        let a = unsafe { allocator.allocate_zeroed() }
            .map_err(|e| e.message())
            .unwrap();
        let b = unsafe { allocator.allocate_zeroed() }
            .map_err(|e| e.message())
            .unwrap();
        let c = unsafe { allocator.allocate_zeroed() }
            .map_err(|e| e.message())
            .unwrap();
        FW.with_borrow_mut(|fw| {
            fw.failed_frees
                .extend([(c, EFI_OUT_OF_RESOURCES), (a, EFI_INVALID_PARAMETER)])
        });
        assert_eq!(
            unsafe { allocator.release_all() },
            Err(EFI_OUT_OF_RESOURCES)
        );
        assert_eq!(allocator.count, 2);
        assert_eq!(&allocator.pages[..3], &[a, c, 0]);
        FW.with_borrow(|fw| {
            assert_eq!(
                &fw.calls[3..],
                &[Call::Free(c, 1), Call::Free(b, 1), Call::Free(a, 1)]
            )
        });
        FW.with_borrow_mut(|fw| fw.failed_frees.clear());
        assert_eq!(unsafe { allocator.release_all() }, Ok(()));
        assert_eq!(allocator.count, 0);
        FW.with_borrow(|fw| {
            assert_eq!(&fw.calls[6..], &[Call::Free(c, 1), Call::Free(a, 1)]);
            assert!(fw.allocations.is_empty());
        });
    }

    #[test]
    fn rollback_compacts_one_high_failed_page_after_lower_pages_are_freed() {
        reset();
        let mut allocator = TableAllocator::new(allocate, free);
        let low = unsafe { allocator.allocate_zeroed() }
            .map_err(|e| e.message())
            .unwrap();
        let high = unsafe { allocator.allocate_zeroed() }
            .map_err(|e| e.message())
            .unwrap();
        FW.with_borrow_mut(|fw| fw.failed_frees.push((high, EFI_OUT_OF_RESOURCES)));
        assert_eq!(
            unsafe { allocator.release_all() },
            Err(EFI_OUT_OF_RESOURCES)
        );
        assert_eq!(allocator.count, 1);
        assert_eq!(allocator.pages[0], high);
        assert!(allocator.pages[1..].iter().all(|&page| page == 0));
        FW.with_borrow(|fw| {
            assert_eq!(&fw.calls[2..], &[Call::Free(high, 1), Call::Free(low, 1)]);
            assert_eq!(fw.allocations.len(), 1);
        });
    }

    #[test]
    fn invalid_table_allocation_release_failure_keeps_owned_slot() {
        for fail in [false, true] {
            reset();
            FW.with_borrow_mut(|fw| {
                fw.next_base = Some(8);
                if fail {
                    fw.failed_frees.push((8, EFI_OUT_OF_RESOURCES));
                }
            });
            let mut allocator = TableAllocator::new(allocate, free);
            let result = unsafe { allocator.allocate_zeroed() };
            assert!(matches!(
                (fail, result),
                (true, Err(PagingError::Firmware(EFI_OUT_OF_RESOURCES)))
                    | (false, Err(PagingError::InvalidPhysicalAddress))
            ));
            assert_eq!(allocator.count, usize::from(fail));
            assert_eq!(allocator.pages[0], if fail { 8 } else { 0 });
            FW.with_borrow(|fw| assert_eq!(fw.calls, [Call::Allocate(8, 1), Call::Free(8, 1)]));
        }
    }

    #[test]
    fn failed_build_propagates_rollback_failure_and_retains_failed_root() {
        for fail in [false, true] {
            reset();
            let (mut data, info) = image();
            // Out-of-span virtual metadata fails after root allocation, before writes
            // to any leaf, so build_with_allocator must roll back its root.
            data[80..88].copy_from_slice(&0xffff_ffff_8000_2000u64.to_le_bytes());
            extern "efiapi" fn allocate_failing_free(
                kind: u32,
                ty: u32,
                pages: usize,
                out: *mut u64,
            ) -> Status {
                let status = allocate(kind, ty, pages, out);
                let base = unsafe { *out };
                FW.with_borrow_mut(|fw| fw.failed_frees.push((base, EFI_OUT_OF_RESOURCES)));
                status
            }
            let mut allocator = TableAllocator::new(
                if fail {
                    allocate_failing_free
                } else {
                    allocate
                },
                free,
            );
            let result = unsafe { build_with_allocator(&mut allocator, &data, &info, &loaded()) };
            assert!(matches!(
                (fail, result),
                (true, Err(PagingError::Firmware(EFI_OUT_OF_RESOURCES)))
                    | (false, Err(PagingError::KernelSpanMismatch))
            ));
            assert_eq!(allocator.count, usize::from(fail));
            FW.with_borrow(|fw| {
                let Call::Allocate(base, 1) = fw.calls[0] else {
                    panic!("root missing")
                };
                assert_eq!(fw.calls, [Call::Allocate(base, 1), Call::Free(base, 1)]);
                if fail {
                    assert_eq!(allocator.pages[0], base);
                }
            });
        }
    }

    #[test]
    fn table_allocation_failure_rolls_back_previously_allocated_pages() {
        reset();
        FW.with_borrow_mut(|fw| fw.fail_allocate_after = Some(2));
        let (data, info) = image();
        let mut allocator = TableAllocator::new(allocate, free);
        let result = unsafe { build_with_allocator(&mut allocator, &data, &info, &loaded()) };
        assert!(matches!(
            result,
            Err(PagingError::Firmware(EFI_OUT_OF_RESOURCES))
        ));
        assert_eq!(allocator.count, 0);
        FW.with_borrow(|fw| {
            let Call::Allocate(a, 1) = fw.calls[0] else {
                panic!("root missing")
            };
            let Call::Allocate(b, 1) = fw.calls[1] else {
                panic!("table missing")
            };
            assert_eq!(
                &fw.calls[2..],
                &[Call::AllocationFailed, Call::Free(b, 1), Call::Free(a, 1)]
            );
            assert!(fw.allocations.is_empty());
        });
    }

    #[test]
    fn successful_build_retains_all_tables_for_handoff() {
        reset();
        let (data, info) = image();
        let mut allocator = TableAllocator::new(allocate, free);
        let tables = unsafe { build_with_allocator(&mut allocator, &data, &info, &loaded()) }
            .map_err(|e| e.message())
            .unwrap();
        assert_eq!(tables.table_pages, 4);
        assert_eq!(tables.mapped_pages, 1);
        FW.with_borrow(|fw| {
            assert_eq!(fw.allocations.len(), 4);
            assert!(
                fw.calls
                    .iter()
                    .all(|call| matches!(call, Call::Allocate(_, 1)))
            );
        });
    }
}
