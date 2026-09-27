//! GetMemoryMap-only foundation, from UEFI 2.10 sections 7.2.1–7.2.3.
//! No descriptor interpretation, BootInfo population, or ExitBootServices.
//! Host tests: `rustc --edition=2024 --test boot/src/memory_map.rs -o /tmp/map-tests`.

// Standalone host tests use the real firmware types and ABI, not a duplicate.
#[cfg(test)]
#[allow(dead_code)]
#[path = "uefi.rs"]
mod uefi;

use crate::uefi::{
    ALLOCATE_ANY_PAGES, AllocatePages, EFI_BUFFER_TOO_SMALL, EFI_INVALID_PARAMETER, EFI_LOAD_ERROR,
    EFI_LOADER_DATA, EFI_OUT_OF_RESOURCES, EFI_SUCCESS, FreePages, GetMemoryMap, MemoryDescriptor,
    Status, SystemTable,
};
use core::mem::{align_of, size_of};
use core::ptr;

// Policy limits, not firmware guarantees. Growth beyond the headroom is retried
// before any map/key is returned. Bound both memory use and firmware retries.
const PAGE_SIZE: usize = 4096;
const HEADROOM_DESCRIPTORS: usize = 64;
const MAX_MAP_BYTES: usize = 16 * 1024 * 1024;
const MAX_ATTEMPTS: usize = 4;

/// Owns a contiguous, page-exclusive EfiLoaderData allocation (ADR 0001).
/// Not Clone/Copy; no Drop that could silently call FreePages and invalidate a key.
/// The successful allocation is intentionally retained for the remainder of boot.
/// A future handoff must reserve all `pages` starting at `physical_base` until consumed.
/// `capacity` includes page rounding; the complete range belongs to this map.
/// `buffer` is a firmware-address-space pointer, not yet a kernel virtual address.
/// Bytes beyond `byte_len` are capacity, not memory-map descriptors.
#[derive(Debug)]
pub struct CapturedMemoryMap {
    get_map: GetMemoryMap,
    pub buffer: *mut MemoryDescriptor,
    pub physical_base: u64,
    pub pages: usize,
    pub capacity: usize,
    pub byte_len: usize,
    pub map_key: usize,
    pub descriptor_size: usize,
    pub descriptor_version: u32,
}

/// Acquire a validated map after the caller's other allocations are complete.
///
/// On success, no further allocation, free, logging or firmware call is performed
/// here. The caller must consume the key immediately without map-changing calls
/// (including firmware console output), or reacquire the map before using it.
/// Firmware events can still stale a key: this is not an ExitBootServices retry
/// implementation and does not promise that the returned key stays valid forever.
///
/// # Safety
/// `system_table` must be aligned, readable and valid, with live Boot Services
/// obeying the UEFI ABI and buffer bounds. Run at TPL_APPLICATION, with no other
/// loader thread mutating the map. Firmware allocations must be uniquely owned,
/// writable, 4096-byte aligned and mapped in this address space. Do not free or
/// repurpose the returned allocation while its descriptors are in use.
pub unsafe fn capture(system_table: *mut SystemTable) -> Result<CapturedMemoryMap, Status> {
    if system_table.is_null() {
        return Err(EFI_INVALID_PARAMETER);
    }
    // SAFETY: caller supplies a live firmware table; null is rejected explicitly.
    let services = unsafe { (*system_table).boot_services };
    if services.is_null() {
        return Err(EFI_INVALID_PARAMETER);
    }
    // Copy function pointers before final acquisition; no retained mutable table borrow.
    let (get_map, allocate, free) = unsafe {
        (
            (*services).get_memory_map,
            (*services).allocate_pages,
            (*services).free_pages,
        )
    };
    // SAFETY: the same live-firmware and allocation invariants apply to these services.
    unsafe { capture_with_services(get_map, allocate, free) }
}

/// Refresh an already owned and mapped buffer without allocating or freeing.
///
/// Call this *after* extending page tables, directly before ExitBootServices,
/// and after EFI_INVALID_PARAMETER using the same preallocated buffer.
/// Preserve the prior complete tuple on any failure.
///
/// # Safety
/// The retained firmware callback must still be callable, even after a
/// rejected ExitBootServices attempt; firmware obeys the buffer's capacity.
pub unsafe fn refresh(map: &mut CapturedMemoryMap) -> Result<(), Status> {
    let mut byte_len = map.capacity;
    let mut map_key = 0;
    let mut descriptor_size = 0;
    let mut descriptor_version = 0;
    let status = unsafe {
        (map.get_map)(
            &mut byte_len,
            map.buffer,
            &mut map_key,
            &mut descriptor_size,
            &mut descriptor_version,
        )
    };
    if status != EFI_SUCCESS {
        return Err(status);
    }
    if !valid_map(byte_len, map.capacity, descriptor_size) {
        return Err(EFI_LOAD_ERROR);
    }
    map.byte_len = byte_len;
    map.map_key = map_key;
    map.descriptor_size = descriptor_size;
    map.descriptor_version = descriptor_version;
    Ok(())
}

fn valid_stride(stride: usize) -> bool {
    stride >= size_of::<MemoryDescriptor>() && stride.is_multiple_of(align_of::<MemoryDescriptor>())
}

fn buffer_capacity(required: usize, stride: usize) -> Result<usize, Status> {
    if required == 0 || !valid_stride(stride) || !required.is_multiple_of(stride) {
        return Err(EFI_LOAD_ERROR);
    }
    stride
        .checked_mul(HEADROOM_DESCRIPTORS)
        .and_then(|extra| required.checked_add(extra))
        .and_then(|bytes| bytes.checked_add(PAGE_SIZE - 1))
        .map(|bytes| bytes & !(PAGE_SIZE - 1))
        .filter(|&bytes| bytes <= MAX_MAP_BYTES && bytes <= isize::MAX as usize)
        .ok_or(EFI_OUT_OF_RESOURCES)
}

fn valid_buffer(address: usize, capacity: usize) -> bool {
    address != 0 && address.is_multiple_of(PAGE_SIZE) && address.checked_add(capacity).is_some()
}

fn valid_map(byte_len: usize, capacity: usize, stride: usize) -> bool {
    byte_len != 0 && byte_len <= capacity && valid_stride(stride) && byte_len.is_multiple_of(stride)
}

// Private service injection keeps host tests on the exact production acquisition flow.
// SAFETY: functions obey their UEFI contracts, including allocation validity and
// GetMemoryMap never writing beyond the supplied capacity (even on error).
unsafe fn capture_with_services(
    get_map: GetMemoryMap,
    allocate: AllocatePages,
    free: FreePages,
) -> Result<CapturedMemoryMap, Status> {
    let mut required = 0;
    let mut key = 0;
    let mut stride = 0;
    let mut version = 0;
    // SAFETY: size=0 permits a null map buffer; all output scalar pointers are live.
    let status = unsafe {
        get_map(
            &mut required,
            ptr::null_mut(),
            &mut key,
            &mut stride,
            &mut version,
        )
    };
    if status != EFI_BUFFER_TOO_SMALL {
        return Err(if status == EFI_SUCCESS {
            EFI_LOAD_ERROR
        } else {
            status
        });
    }

    for _ in 0..MAX_ATTEMPTS {
        let capacity = buffer_capacity(required, stride)?;
        let pages = capacity / PAGE_SIZE;
        let mut physical_base = 0;
        let status = allocate(
            ALLOCATE_ANY_PAGES,
            EFI_LOADER_DATA,
            pages,
            &mut physical_base,
        );
        if status != EFI_SUCCESS {
            return Err(status);
        }
        let address = usize::try_from(physical_base)
            .ok()
            .filter(|&address| valid_buffer(address, capacity));
        let Some(address) = address else {
            // This successful allocation will not escape: return exactly the
            // physical base and page count supplied by AllocatePages, even at 0.
            let free_status = free(physical_base, pages);
            if free_status != EFI_SUCCESS {
                return Err(free_status);
            }
            return Err(EFI_LOAD_ERROR);
        };
        // The x86-64 UEFI address space identity-maps these allocated pages.
        let buffer = address as *mut u8;
        // SAFETY: the checked region is uniquely allocated and writable. Initialize
        // padding/extension bytes as well; only byte_len will be part of the map.
        unsafe { ptr::write_bytes(buffer.cast::<u8>(), 0, capacity) };
        let mut byte_len = capacity;
        key = 0;
        stride = 0;
        version = 0;
        // SAFETY: buffer is aligned and has capacity bytes; scalar outputs are live.
        let status = unsafe {
            get_map(
                &mut byte_len,
                buffer.cast(),
                &mut key,
                &mut stride,
                &mut version,
            )
        };
        if status == EFI_SUCCESS && valid_map(byte_len, capacity, stride) {
            // No firmware calls or destructors beyond this successful boundary.
            return Ok(CapturedMemoryMap {
                get_map,
                buffer: buffer.cast(),
                physical_base,
                pages,
                capacity,
                byte_len,
                map_key: key,
                descriptor_size: stride,
                descriptor_version: version,
            });
        }
        // No usable map/key will escape this path. Free before another allocation
        // and acquisition so that a later successful map includes all our changes.
        let free_status = free(physical_base, pages);
        if free_status != EFI_SUCCESS {
            return Err(free_status);
        }
        if status != EFI_BUFFER_TOO_SMALL {
            return Err(if status == EFI_SUCCESS {
                EFI_LOAD_ERROR
            } else {
                status
            });
        }
        if byte_len <= capacity {
            return Err(EFI_LOAD_ERROR);
        }
        required = byte_len;
    }
    Err(EFI_BUFFER_TOO_SMALL)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[repr(C, align(4096))]
    struct Page([u8; PAGE_SIZE]);

    #[derive(Default)]
    struct Firmware {
        calls: Vec<&'static str>,
        buffers: Vec<Box<[Page]>>,
        grow: usize,
        final_stride: Option<usize>,
        final_len: Option<usize>,
        version: Option<u32>,
        map_error: Option<Status>,
        allocation_error: bool,
        allocation_base: Option<u64>,
        free_error: bool,
        sizing_success: bool,
    }

    thread_local! {
        static FW: RefCell<Firmware> = RefCell::new(Firmware::default());
    }

    extern "efiapi" fn allocate(
        allocation: u32,
        kind: u32,
        pages: usize,
        output: *mut u64,
    ) -> Status {
        FW.with_borrow_mut(|fw| {
            fw.calls.push("allocate");
            assert_eq!(allocation, ALLOCATE_ANY_PAGES);
            assert_eq!(kind, EFI_LOADER_DATA);
            assert!(pages > 0);
            if fw.allocation_error {
                return EFI_OUT_OF_RESOURCES;
            }
            let mut backing = Vec::new();
            backing.resize_with(pages, || Page([0; PAGE_SIZE]));
            let mut backing = backing.into_boxed_slice();
            // SAFETY: caller provides a live output pointer. Each mock page is
            // 4096-byte aligned; backing remains live until FreePages/test reset.
            unsafe { *output = fw.allocation_base.unwrap_or(backing.as_mut_ptr() as u64) };
            fw.buffers.push(backing);
            EFI_SUCCESS
        })
    }

    extern "efiapi" fn free(physical_base: u64, pages: usize) -> Status {
        FW.with_borrow_mut(|fw| {
            fw.calls.push("free");
            let owned = fw
                .buffers
                .last()
                .expect("free requires an owned allocation");
            assert_eq!(
                fw.allocation_base.unwrap_or(owned.as_ptr() as u64),
                physical_base
            );
            assert_eq!(
                owned.len(),
                pages,
                "free must cover the exact owned page range"
            );
            if fw.free_error {
                return EFI_OUT_OF_RESOURCES;
            }
            fw.buffers.pop();
            EFI_SUCCESS
        })
    }

    unsafe extern "efiapi" fn get_map(
        len: *mut usize,
        buffer: *mut MemoryDescriptor,
        key: *mut usize,
        stride: *mut usize,
        version: *mut u32,
    ) -> Status {
        FW.with_borrow_mut(|fw| {
            // SAFETY: capture_with_services supplies valid scalar pointers and a
            // null buffer only for sizing, otherwise a capacity-sized allocation.
            unsafe {
                if buffer.is_null() {
                    fw.calls.push("size");
                    assert_eq!(*len, 0);
                    *len = 96;
                    *stride = 48;
                    *version = 1;
                    *key = 11;
                    return if fw.sizing_success {
                        EFI_SUCCESS
                    } else {
                        EFI_BUFFER_TOO_SMALL
                    };
                }
                fw.calls.push("map");
                let capacity = *len;
                assert!(capacity >= 96 + 8 * 48);
                if fw.grow > 0 {
                    fw.grow -= 1;
                    *len = capacity.div_ceil(48) * 48 + 48;
                    *stride = 48;
                    return EFI_BUFFER_TOO_SMALL;
                }
                if let Some(error) = fw.map_error {
                    return error;
                }
                // Write real extension bytes as well as the known descriptor prefix.
                ptr::write_bytes(buffer.cast::<u8>(), 0x5a, 96);
                *len = fw.final_len.unwrap_or(96);
                *stride = fw.final_stride.unwrap_or(48);
                *version = fw.version.unwrap_or(7);
                *key = 22;
                EFI_SUCCESS
            }
        })
    }

    fn run(fw: Firmware) -> Result<CapturedMemoryMap, Status> {
        FW.set(fw);
        // SAFETY: mocks implement the same allocation, lifetime and ABI contracts.
        unsafe { capture_with_services(get_map, allocate, free) }
    }

    #[test]
    fn final_tuple_and_padded_descriptor_bytes_are_preserved_without_post_map_calls() {
        let map = run(Firmware::default()).unwrap();
        assert_eq!(
            (
                map.byte_len,
                map.map_key,
                map.descriptor_size,
                map.descriptor_version
            ),
            (96, 22, 48, 7)
        );
        assert_eq!(map.capacity, PAGE_SIZE);
        assert_eq!(map.pages, 1);
        assert_eq!(map.physical_base, map.buffer as u64);
        assert_eq!(map.physical_base % PAGE_SIZE as u64, 0);
        // SAFETY: mock retains the allocation; map byte_len was validated.
        let bytes = unsafe { core::slice::from_raw_parts(map.buffer.cast::<u8>(), map.byte_len) };
        assert!(bytes.iter().all(|&byte| byte == 0x5a));
        FW.with_borrow(|fw| {
            assert_eq!(fw.calls, ["size", "allocate", "map"]);
            assert_eq!(fw.buffers.len(), 1);
        });
    }

    #[test]
    fn growth_retries_free_before_allocating_and_reacquiring() {
        let map = run(Firmware {
            grow: 1,
            ..Firmware::default()
        })
        .unwrap();
        assert_eq!(map.capacity, 2 * PAGE_SIZE);
        assert_eq!(map.pages, 2);
        assert_eq!(map.map_key, 22);
        FW.with_borrow(|fw| {
            assert_eq!(
                fw.calls,
                ["size", "allocate", "map", "free", "allocate", "map"]
            );
            assert_eq!(fw.buffers.len(), 1);
        });
    }

    #[test]
    fn repeated_growth_is_bounded_and_frees_every_failed_buffer() {
        assert_eq!(
            run(Firmware {
                grow: 10,
                ..Firmware::default()
            })
            .unwrap_err(),
            EFI_BUFFER_TOO_SMALL
        );
        FW.with_borrow(|fw| {
            assert_eq!(
                fw.calls.iter().filter(|&&call| call == "map").count(),
                MAX_ATTEMPTS
            );
            assert!(fw.buffers.is_empty());
        });
    }

    #[test]
    fn malformed_final_metadata_is_rejected_and_freed() {
        for (len, stride) in [
            (0, 48),
            (96, 0),
            (96, 32),
            (96, 41),
            (95, 48),
            (PAGE_SIZE + 32, 48),
        ] {
            assert_eq!(
                run(Firmware {
                    final_len: Some(len),
                    final_stride: Some(stride),
                    ..Firmware::default()
                })
                .unwrap_err(),
                EFI_LOAD_ERROR
            );
            FW.with_borrow(|fw| {
                assert!(fw.buffers.is_empty());
                assert_eq!(fw.calls.last(), Some(&"free"));
            });
        }
    }

    #[test]
    fn zero_and_future_versions_are_preserved_without_parsing() {
        for version in [0, 1, 42, u32::MAX] {
            let map = run(Firmware {
                version: Some(version),
                ..Firmware::default()
            })
            .unwrap();
            assert_eq!(map.descriptor_version, version);
        }
    }

    #[test]
    fn allocation_and_acquisition_errors_propagate() {
        assert_eq!(
            run(Firmware {
                allocation_error: true,
                ..Firmware::default()
            })
            .unwrap_err(),
            EFI_OUT_OF_RESOURCES
        );
        FW.with_borrow(|fw| assert_eq!(fw.calls, ["size", "allocate"]));
        assert_eq!(
            run(Firmware {
                map_error: Some(EFI_INVALID_PARAMETER),
                ..Firmware::default()
            })
            .unwrap_err(),
            EFI_INVALID_PARAMETER
        );
        FW.with_borrow(|fw| assert!(fw.buffers.is_empty()));
    }

    #[test]
    fn invalid_allocated_base_releases_exact_pages_and_propagates_release_failure() {
        // Invalid firmware outputs must never reach memory initialization/GetMemoryMap.
        for base in [0, 8, u64::MAX & !(PAGE_SIZE as u64 - 1)] {
            for free_error in [false, true] {
                let status = run(Firmware {
                    allocation_base: Some(base),
                    free_error,
                    ..Firmware::default()
                })
                .unwrap_err();
                assert_eq!(
                    status,
                    if free_error {
                        EFI_OUT_OF_RESOURCES
                    } else {
                        EFI_LOAD_ERROR
                    }
                );
                FW.with_borrow(|fw| {
                    assert_eq!(fw.calls, ["size", "allocate", "free"]);
                    assert_eq!(fw.buffers.len(), usize::from(free_error));
                });
            }
        }
    }

    #[test]
    fn unexpected_sizing_success_cannot_produce_a_map() {
        assert_eq!(
            run(Firmware {
                sizing_success: true,
                ..Firmware::default()
            })
            .unwrap_err(),
            EFI_LOAD_ERROR
        );
        FW.with_borrow(|fw| assert_eq!(fw.calls, ["size"]));
    }

    #[test]
    fn refresh_keeps_page_ownership_and_updates_the_entire_final_tuple() {
        let mut map = run(Firmware::default()).unwrap();
        assert_eq!(map.map_key, 22);
        FW.with_borrow_mut(|fw| {
            fw.version = Some(1);
            fw.final_stride = Some(48);
            fw.final_len = Some(144);
            fw.calls.clear();
        });
        // SAFETY: mock retains the uniquely owned mapped buffer/callback.
        unsafe { refresh(&mut map) }.unwrap();
        assert_eq!(
            (
                map.byte_len,
                map.map_key,
                map.descriptor_size,
                map.descriptor_version
            ),
            (144, 22, 48, 1)
        );
        FW.with_borrow(|fw| {
            assert_eq!(fw.calls, ["map"]);
            assert_eq!(fw.buffers.len(), 1);
        });
    }

    #[test]
    fn refresh_failure_never_publishes_a_partial_stale_tuple() {
        let mut map = run(Firmware::default()).unwrap();
        let old = (
            map.byte_len,
            map.map_key,
            map.descriptor_size,
            map.descriptor_version,
        );
        FW.with_borrow_mut(|fw| {
            fw.grow = 1;
            fw.calls.clear();
        });
        // SAFETY: mock supplies valid ABI and live allocation; a too-small
        // refresh must not change the published tuple or free pages.
        assert_eq!(unsafe { refresh(&mut map) }, Err(EFI_BUFFER_TOO_SMALL));
        assert_eq!(
            (
                map.byte_len,
                map.map_key,
                map.descriptor_size,
                map.descriptor_version
            ),
            old
        );
        FW.with_borrow(|fw| {
            assert_eq!(fw.calls, ["map"]);
            assert_eq!(fw.buffers.len(), 1);
        });
    }

    #[test]
    fn capacity_validation_checks_stride_size_and_arithmetic() {
        assert_eq!(buffer_capacity(80, 40), Ok(PAGE_SIZE));
        // Required map fits one page, but descriptor headroom crosses its end.
        assert_eq!(buffer_capacity(4032, 48), Ok(2 * PAGE_SIZE));
        // Reserve extra descriptors for page-table allocation before final refresh.
        assert_eq!(buffer_capacity(3584, 64), Ok(2 * PAGE_SIZE));
        for (required, stride) in [(0, 40), (80, 0), (80, 32), (80, 41), (81, 40)] {
            assert_eq!(buffer_capacity(required, stride), Err(EFI_LOAD_ERROR));
        }
        let huge = usize::MAX & !7;
        assert_eq!(buffer_capacity(huge, huge), Err(EFI_OUT_OF_RESOURCES));
        assert_eq!(
            buffer_capacity(MAX_MAP_BYTES, 64),
            Err(EFI_OUT_OF_RESOURCES)
        );
        assert!(!valid_buffer(usize::MAX & !7, 480));
        assert!(!valid_buffer(0, 480));
        assert!(!valid_buffer(3, 480));
        assert!(!valid_buffer(8, PAGE_SIZE));
        assert!(valid_buffer(PAGE_SIZE, PAGE_SIZE));
    }

    #[test]
    fn x64_firmware_abi_layout_matches_specification() {
        use core::mem::offset_of;
        assert_eq!(size_of::<MemoryDescriptor>(), 40);
        assert_eq!(align_of::<MemoryDescriptor>(), 8);
        assert_eq!(offset_of!(MemoryDescriptor, physical_start), 8);
        assert_eq!(offset_of!(MemoryDescriptor, attribute), 32);
        assert_eq!(offset_of!(uefi::BootServices, allocate_pages), 40);
        assert_eq!(offset_of!(uefi::BootServices, free_pages), 48);
        assert_eq!(offset_of!(uefi::BootServices, get_memory_map), 56);
        assert_eq!(offset_of!(uefi::BootServices, allocate_pool), 64);
        assert_eq!(offset_of!(uefi::BootServices, free_pool), 72);
    }

    #[test]
    fn null_system_table_is_rejected_without_dereference() {
        // SAFETY: explicitly exercising the documented null rejection path.
        assert_eq!(
            unsafe { capture(ptr::null_mut()) }.unwrap_err(),
            EFI_INVALID_PARAMETER
        );
    }
}
