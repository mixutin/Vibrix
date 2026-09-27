#![no_std]
#![no_main]

use core::panic::PanicInfo;

type Handle = *mut core::ffi::c_void;

#[repr(C)]
struct SimpleTextOutputProtocol {
    reset: usize,
    output_string: extern "efiapi" fn(*mut SimpleTextOutputProtocol, *const u16) -> usize,
}

#[repr(C)]
struct SystemTable {
    _header: [u8; 24],
    _firmware_vendor: usize,
    _firmware_revision: u32,
    _pad: u32,
    _console_in_handle: usize,
    _con_in: usize,
    _console_out_handle: usize,
    con_out: *mut SimpleTextOutputProtocol,
}

static MESSAGE: [u16; 73] = utf16("Vibrix bootloader v0.0.1\r\nRust-native. Independent.\r\nHello from UEFI.\r\n");

const fn utf16<const N: usize>(s: &str) -> [u16; N] {
    let bytes = s.as_bytes();
    let mut out = [0u16; N];
    let mut i = 0;
    while i < bytes.len() && i + 1 < N {
        out[i] = bytes[i] as u16;
        i += 1;
    }
    out
}

#[unsafe(no_mangle)]
pub extern "efiapi" fn efi_main(_image: Handle, system_table: *mut SystemTable) -> usize {
    unsafe {
        let console = (*system_table).con_out;
        ((*console).output_string)(console, MESSAGE.as_ptr());
    }

    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
