#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]

#[allow(dead_code)]
mod elf;

type Handle = *mut core::ffi::c_void;
type Status = usize;

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
    output_string: extern "efiapi" fn(*mut SimpleTextOutputProtocol, *const u16) -> Status,
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
pub struct SystemTable {
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

static MESSAGE: &[u16] = &[
    'V' as u16,
    'i' as u16,
    'b' as u16,
    'r' as u16,
    'i' as u16,
    'x' as u16,
    ' ' as u16,
    'b' as u16,
    'o' as u16,
    'o' as u16,
    't' as u16,
    'l' as u16,
    'o' as u16,
    'a' as u16,
    'd' as u16,
    'e' as u16,
    'r' as u16,
    ' ' as u16,
    'v' as u16,
    '0' as u16,
    '.' as u16,
    '0' as u16,
    '.' as u16,
    '1' as u16,
    '\r' as u16,
    '\n' as u16,
    'R' as u16,
    'u' as u16,
    's' as u16,
    't' as u16,
    '-' as u16,
    'n' as u16,
    'a' as u16,
    't' as u16,
    'i' as u16,
    'v' as u16,
    'e' as u16,
    '.' as u16,
    ' ' as u16,
    'I' as u16,
    'n' as u16,
    'd' as u16,
    'e' as u16,
    'p' as u16,
    'e' as u16,
    'n' as u16,
    'd' as u16,
    'e' as u16,
    'n' as u16,
    't' as u16,
    '.' as u16,
    '\r' as u16,
    '\n' as u16,
    'H' as u16,
    'e' as u16,
    'l' as u16,
    'l' as u16,
    'o' as u16,
    ' ' as u16,
    'f' as u16,
    'r' as u16,
    'o' as u16,
    'm' as u16,
    ' ' as u16,
    'U' as u16,
    'E' as u16,
    'F' as u16,
    'I' as u16,
    '.' as u16,
    '\r' as u16,
    '\n' as u16,
    0,
];

#[unsafe(no_mangle)]
pub extern "efiapi" fn efi_main(_image: Handle, system_table: *mut SystemTable) -> Status {
    if system_table.is_null() {
        return 2;
    }

    unsafe {
        let console = (*system_table).con_out;
        if console.is_null() {
            return 2;
        }
        ((*console).output_string)(console, MESSAGE.as_ptr());
    }

    loop {
        core::hint::spin_loop();
    }
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
