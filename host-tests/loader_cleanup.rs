//! Host harness for production loader cleanup paths; no alternate implementations.
#![allow(dead_code)]
#[path = "../boot/src/elf.rs"]
mod elf;
#[path = "../boot/src/loader.rs"]
mod loader;
#[path = "../boot/src/paging.rs"]
mod paging;
#[path = "../boot/src/uefi.rs"]
mod uefi;

mod test_support {
    use crate::uefi::*;
    use std::cell::RefCell;

    #[repr(C, align(4096))]
    pub struct Page(pub [u8; 4096]);

    #[derive(Debug, PartialEq, Eq)]
    pub enum Call {
        Allocate(u64, usize),
        AllocationFailed,
        Free(u64, usize),
    }

    #[derive(Default)]
    pub struct Firmware {
        pub calls: Vec<Call>,
        pub allocations: Vec<(u64, Box<[Page]>)>,
        pub failed_frees: Vec<(u64, Status)>,
        pub next_base: Option<u64>,
        pub fail_allocate_after: Option<usize>,
        pub allocated: usize,
    }

    thread_local! { pub static FW: RefCell<Firmware> = RefCell::new(Firmware::default()); }

    pub fn reset() {
        FW.set(Firmware::default());
    }

    pub extern "efiapi" fn allocate(
        kind: u32,
        memory_type: u32,
        pages: usize,
        out: *mut u64,
    ) -> Status {
        FW.with_borrow_mut(|fw| {
            assert_eq!(kind, ALLOCATE_ANY_PAGES);
            assert_eq!(memory_type, EFI_LOADER_DATA);
            assert!(pages > 0);
            if fw.fail_allocate_after == Some(fw.allocated) {
                fw.calls.push(Call::AllocationFailed);
                return EFI_OUT_OF_RESOURCES;
            }
            let mut allocation = Vec::new();
            allocation.resize_with(pages, || Page([0; 4096]));
            let mut allocation = allocation.into_boxed_slice();
            let base = fw
                .next_base
                .take()
                .unwrap_or(allocation.as_mut_ptr() as u64);
            fw.calls.push(Call::Allocate(base, pages));
            fw.allocations.push((base, allocation));
            fw.allocated += 1;
            // SAFETY: production callers supply a live scalar output pointer.
            unsafe {
                *out = base;
            }
            EFI_SUCCESS
        })
    }

    pub extern "efiapi" fn free(base: u64, pages: usize) -> Status {
        FW.with_borrow_mut(|fw| {
            fw.calls.push(Call::Free(base, pages));
            let index = fw
                .allocations
                .iter()
                .position(|(owned, _)| *owned == base)
                .expect("owned base");
            assert_eq!(fw.allocations[index].1.len(), pages);
            if let Some((_, status)) = fw.failed_frees.iter().find(|(owned, _)| *owned == base) {
                return *status;
            }
            fw.allocations.remove(index);
            EFI_SUCCESS
        })
    }

    pub fn image() -> (Vec<u8>, crate::elf::ElfInfo) {
        let mut data = vec![0u8; 4096];
        data[0..7].copy_from_slice(&[0x7f, b'E', b'L', b'F', 2, 1, 1]);
        data[16..18].copy_from_slice(&2u16.to_le_bytes());
        data[18..20].copy_from_slice(&62u16.to_le_bytes());
        data[20..24].copy_from_slice(&1u32.to_le_bytes());
        data[24..32].copy_from_slice(&0xffff_ffff_8000_0000u64.to_le_bytes());
        data[32..40].copy_from_slice(&64u64.to_le_bytes());
        data[52..54].copy_from_slice(&64u16.to_le_bytes());
        data[54..56].copy_from_slice(&56u16.to_le_bytes());
        data[56..58].copy_from_slice(&1u16.to_le_bytes());
        data[64..68].copy_from_slice(&1u32.to_le_bytes());
        data[68..72].copy_from_slice(&5u32.to_le_bytes());
        data[80..88].copy_from_slice(&0xffff_ffff_8000_0000u64.to_le_bytes());
        data[96..104].copy_from_slice(&256u64.to_le_bytes());
        data[104..112].copy_from_slice(&4096u64.to_le_bytes());
        data[112..120].copy_from_slice(&4096u64.to_le_bytes());
        let info = crate::elf::validate(&data)
            .map_err(|e| e.message())
            .unwrap();
        (data, info)
    }
}

#[test]
fn shared_free_rejects_missing_tables_and_zero_page_count() {
    use uefi::*;
    // SystemTable contains only integers, TableHeader and raw pointers; unlike
    // BootServices it has no function-pointer values. Zero is valid for its fields.
    let mut table: SystemTable = unsafe { core::mem::zeroed() };
    unsafe {
        assert_eq!(
            free_loader_pages(core::ptr::null_mut(), 4096, 1),
            Err(EFI_INVALID_PARAMETER)
        );
        assert_eq!(
            free_loader_pages(&mut table, 4096, 0),
            Err(EFI_INVALID_PARAMETER)
        );
        assert_eq!(free_loader_pages(&mut table, 4096, 1), Err(EFI_LOAD_ERROR));
    }
}

#[test]
fn shared_allocation_invalid_result_frees_zero_and_preserves_release_failure() {
    use test_support::*;
    use uefi::*;
    for fail in [false, true] {
        reset();
        FW.with_borrow_mut(|fw| {
            fw.next_base = Some(0);
            if fail {
                fw.failed_frees.push((0, EFI_OUT_OF_RESOURCES));
            }
        });
        let result = unsafe { allocate_pages_with(allocate, free, 2) };
        assert_eq!(
            result,
            Err(if fail {
                EFI_OUT_OF_RESOURCES
            } else {
                EFI_LOAD_ERROR
            })
        );
        FW.with_borrow(|fw| {
            assert_eq!(fw.calls, [Call::Allocate(0, 2), Call::Free(0, 2)]);
            assert_eq!(fw.allocations.len(), usize::from(fail));
        });
    }
}

#[test]
fn shared_zero_count_rejection_makes_no_firmware_call() {
    use test_support::*;
    reset();
    unsafe {
        assert_eq!(
            uefi::allocate_pages_with(allocate, free, 0),
            Err(uefi::EFI_INVALID_PARAMETER)
        );
        assert_eq!(
            uefi::free_pages_with(free, 0, 0),
            Err(uefi::EFI_INVALID_PARAMETER)
        );
    }
    FW.with_borrow(|fw| assert!(fw.calls.is_empty()));
}
