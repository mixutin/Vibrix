#![no_std]
#![no_main]

use core::{ffi::c_void, panic::PanicInfo};

type Handle = *mut c_void;
type Status = usize;
const EFI_SUCCESS: Status = 0;
const EFI_LOAD_ERROR: Status = (1usize << (usize::BITS - 1)) | 1;

#[repr(C)]
struct TableHeader {
    signature: u64,
    revision: u32,
    header_size: u32,
    crc32: u32,
    reserved: u32,
}

#[repr(C)]
struct SimpleTextOutputProtocol {
    reset: usize,
    output_string:
        extern "efiapi" fn(*mut SimpleTextOutputProtocol, *const u16) -> Status,
    test_string: usize,
    query_mode: usize,
    set_mode: usize,
    set_attribute: usize,
    clear_screen: usize,
    set_cursor_position: usize,
    enable_cursor: usize,
    mode: usize,
}

#[repr(C)]
struct SystemTable {
    header: TableHeader,
    firmware_vendor: *const u16,
    firmware_revision: u32,
    _pad: u32,
    console_in_handle: Handle,
    con_in: usize,
    console_out_handle: Handle,
    con_out: *mut SimpleTextOutputProtocol,
    standard_error_handle: Handle,
    std_err: *mut SimpleTextOutputProtocol,
    runtime_services: usize,
    boot_services: usize,
    number_of_table_entries: usize,
    configuration_table: usize,
}

static MESSAGE: [u16; 49] = [
    'V' as u16, 'I' as u16, 'B' as u16, 'R' as u16, 'I' as u16, 'X' as u16,
    ':' as u16, ' ' as u16, 'A' as u16, 'A' as u16, 'r' as u16, 'c' as u16,
    'h' as u16, '6' as u16, '4' as u16, ' ' as u16, 'U' as u16, 'E' as u16,
    'F' as u16, 'I' as u16, ' ' as u16, 'b' as u16, 'o' as u16, 'o' as u16,
    't' as u16, ' ' as u16, 'e' as u16, 'n' as u16, 't' as u16, 'r' as u16,
    'y' as u16, ' ' as u16, 'v' as u16, 'e' as u16, 'r' as u16, 'i' as u16,
    'f' as u16, 'i' as u16, 'e' as u16, 'd' as u16, '\r' as u16, '\n' as u16,
    0, 0, 0, 0, 0, 0, 0,
];

/// Minimal architecture bring-up entry point.
///
/// # Safety
/// Firmware must invoke this entry with a valid UEFI system table and the
/// AArch64 UEFI calling convention represented by Rust's efiapi ABI.
#[unsafe(no_mangle)]
unsafe extern "efiapi" fn efi_main(_image: Handle, system_table: *mut SystemTable) -> Status {
    let Some(table) = (unsafe { system_table.as_ref() }) else {
        return EFI_LOAD_ERROR;
    };
    let Some(out) = (unsafe { table.con_out.as_mut() }) else {
        return EFI_LOAD_ERROR;
    };
    let status = (out.output_string)(out, MESSAGE.as_ptr());
    if status != EFI_SUCCESS {
        return status;
    }
    EFI_SUCCESS
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
