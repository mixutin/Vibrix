//! GetMemoryMap-only foundation, from UEFI 2.10 sections 7.2.3–7.2.5.
//! No descriptor interpretation, BootInfo population, or ExitBootServices.
//! Host tests: `rustc --edition=2024 --test boot/src/memory_map.rs -o /tmp/map-tests`.

// Standalone host tests use the real firmware types and ABI, not a duplicate.
#[cfg(test)]
#[allow(dead_code)]
#[path = "uefi.rs"]
mod uefi;

use crate::uefi::{
    AllocatePool, EFI_BUFFER_TOO_SMALL, EFI_INVALID_PARAMETER, EFI_LOAD_ERROR, EFI_LOADER_DATA,
    EFI_OUT_OF_RESOURCES, EFI_SUCCESS, FreePool, GetMemoryMap, MemoryDescriptor, Status,
    SystemTable,
};
use core::mem::{align_of, size_of};
use core::ptr;

// Policy limits, not firmware guarantees. Growth beyond the headroom is retried
// before any map/key is returned. Bound both memory use and firmware retries.
const HEADROOM_DESCRIPTORS: usize = 8;
const MAX_MAP_BYTES: usize = 16 * 1024 * 1024;
const MAX_ATTEMPTS: usize = 4;

/// Owns a retained EfiLoaderData pool allocation in the current firmware mapping.
/// Not Clone/Copy; no Drop that could silently call FreePool and invalidate a key.
/// The successful allocation is intentionally retained for the remainder of boot.
/// A future handoff must reserve this buffer until the kernel has consumed it.
/// `buffer` is a firmware-address-space pointer, not yet a kernel virtual address.
/// Bytes beyond `byte_len` are capacity, not memory-map descriptors.
#[derive(Debug)]
pub struct CapturedMemoryMap {
    pub buffer: *mut MemoryDescriptor,
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
/// writable, eight-byte aligned and mapped in this address space. Do not free or
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
            (*services).allocate_pool,
            (*services).free_pool,
        )
    };
    // SAFETY: the same live-firmware and allocation invariants apply to these services.
    unsafe { capture_with_services(get_map, allocate, free) }
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
        .filter(|&bytes| bytes <= MAX_MAP_BYTES && bytes <= isize::MAX as usize)
        .ok_or(EFI_OUT_OF_RESOURCES)
}

fn valid_buffer(address: usize, capacity: usize) -> bool {
    address != 0
        && address.is_multiple_of(align_of::<MemoryDescriptor>())
        && address.checked_add(capacity).is_some()
}

fn valid_map(byte_len: usize, capacity: usize, stride: usize) -> bool {
    byte_len != 0 && byte_len <= capacity && valid_stride(stride) && byte_len.is_multiple_of(stride)
}

// Private service injection keeps host tests on the exact production acquisition flow.
// SAFETY: functions obey their UEFI contracts, including allocation validity and
// GetMemoryMap never writing beyond the supplied capacity (even on error).
unsafe fn capture_with_services(
    get_map: GetMemoryMap,
    allocate: AllocatePool,
    free: FreePool,
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
        let mut buffer = ptr::null_mut();
        let status = allocate(EFI_LOADER_DATA, capacity, &mut buffer);
        if status != EFI_SUCCESS {
            return Err(status);
        }
        if !valid_buffer(buffer as usize, capacity) {
            // Null cannot be passed to FreePool; no valid allocation is exposed.
            if !buffer.is_null() {
                free(buffer);
            }
            return Err(EFI_LOAD_ERROR);
        }
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
                buffer: buffer.cast(),
                capacity,
                byte_len,
                map_key: key,
                descriptor_size: stride,
                descriptor_version: version,
            });
        }
        // No usable map/key will escape this path. Free before another allocation
        // and acquisition so that a later successful map includes all our changes.
        let free_status = free(buffer);
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
    use core::ffi::c_void;
    use std::cell::RefCell;

    #[derive(Default)]
    struct Firmware {
        calls: Vec<&'static str>,
        buffers: Vec<Box<[u64]>>,
        grow: usize,
        final_stride: Option<usize>,
        final_len: Option<usize>,
        version: Option<u32>,
        map_error: Option<Status>,
        allocation_error: bool,
        sizing_success: bool,
    }

    thread_local! {
        static FW: RefCell<Firmware> = RefCell::new(Firmware::default());
    }

    extern "efiapi" fn allocate(kind: u32, bytes: usize, output: *mut *mut c_void) -> Status {
        FW.with_borrow_mut(|fw| {
            fw.calls.push("allocate");
            assert_eq!(kind, EFI_LOADER_DATA);
            if fw.allocation_error {
                return EFI_OUT_OF_RESOURCES;
            }
            let mut backing = vec![0u64; bytes.div_ceil(8)].into_boxed_slice();
            // SAFETY: production caller provides a live output pointer. Box backing
            // is eight-byte aligned and retained until mock FreePool (or test reset).
            unsafe { *output = backing.as_mut_ptr().cast() };
            fw.buffers.push(backing);
            EFI_SUCCESS
        })
    }

    extern "efiapi" fn free(buffer: *mut c_void) -> Status {
        FW.with_borrow_mut(|fw| {
            fw.calls.push("free");
            let owned = fw.buffers.pop().expect("free requires an owned allocation");
            assert_eq!(owned.as_ptr() as *mut c_void, buffer);
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
                    *len = capacity + 48;
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
        assert_eq!(map.capacity, 480);
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
        assert_eq!(map.capacity, 912);
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
        for (len, stride) in [(0, 48), (96, 0), (96, 32), (96, 41), (95, 48), (528, 48)] {
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
    fn capacity_validation_checks_stride_size_and_arithmetic() {
        assert_eq!(buffer_capacity(80, 40), Ok(400));
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
    }

    #[test]
    fn x64_firmware_abi_layout_matches_specification() {
        use core::mem::offset_of;
        assert_eq!(size_of::<MemoryDescriptor>(), 40);
        assert_eq!(align_of::<MemoryDescriptor>(), 8);
        assert_eq!(offset_of!(MemoryDescriptor, physical_start), 8);
        assert_eq!(offset_of!(MemoryDescriptor, attribute), 32);
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
