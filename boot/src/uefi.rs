use core::ffi::c_void;
use core::ptr::null_mut;
use core::slice;

pub type Handle = *mut c_void;
pub type Status = usize;

pub const EFI_SUCCESS: Status = 0;
pub const EFI_ERROR_BIT: Status = 1usize << (usize::BITS - 1);
pub const EFI_LOAD_ERROR: Status = EFI_ERROR_BIT | 1;
pub const EFI_INVALID_PARAMETER: Status = EFI_ERROR_BIT | 2;
pub const EFI_OUT_OF_RESOURCES: Status = EFI_ERROR_BIT | 9;

const EFI_LOADER_DATA: u32 = 2;
const FILE_MODE_READ: u64 = 1;

#[repr(C)]
pub struct Guid {
    data1: u32,
    data2: u16,
    data3: u16,
    data4: [u8; 8],
}

const LOADED_IMAGE_PROTOCOL_GUID: Guid = Guid {
    data1: 0x5b1b31a1,
    data2: 0x9562,
    data3: 0x11d2,
    data4: [0x8e, 0x3f, 0x00, 0xa0, 0xc9, 0x69, 0x72, 0x3b],
};

const SIMPLE_FILE_SYSTEM_PROTOCOL_GUID: Guid = Guid {
    data1: 0x964e5b22,
    data2: 0x6459,
    data3: 0x11d2,
    data4: [0x8e, 0x39, 0x00, 0xa0, 0xc9, 0x69, 0x72, 0x3b],
};

static KERNEL_PATH: &[u16] = &[
    '\\' as u16, 'v' as u16, 'i' as u16, 'b' as u16, 'r' as u16, 'i' as u16,
    'x' as u16, '\\' as u16, 'k' as u16, 'e' as u16, 'r' as u16, 'n' as u16,
    'e' as u16, 'l' as u16, '.' as u16, 'e' as u16, 'l' as u16, 'f' as u16, 0,
];

#[repr(C)]
pub struct TableHeader {
    pub signature: u64,
    pub revision: u32,
    pub header_size: u32,
    pub crc32: u32,
    pub reserved: u32,
}

#[repr(C)]
pub struct SimpleTextOutputProtocol {
    pub reset: usize,
    pub output_string:
        extern "efiapi" fn(*mut SimpleTextOutputProtocol, *const u16) -> Status,
    pub test_string: usize,
    pub query_mode: usize,
    pub set_mode: usize,
    pub set_attribute: usize,
    pub clear_screen: usize,
    pub set_cursor_position: usize,
    pub enable_cursor: usize,
    pub mode: usize,
}

type AllocatePool =
    extern "efiapi" fn(memory_type: u32, size: usize, buffer: *mut *mut c_void) -> Status;
type FreePool = extern "efiapi" fn(buffer: *mut c_void) -> Status;
type HandleProtocol =
    extern "efiapi" fn(handle: Handle, protocol: *const Guid, interface: *mut *mut c_void) -> Status;

#[repr(C)]
pub struct BootServices {
    pub header: TableHeader,
    pub raise_tpl: usize,
    pub restore_tpl: usize,
    pub allocate_pages: usize,
    pub free_pages: usize,
    pub get_memory_map: usize,
    pub allocate_pool: AllocatePool,
    pub free_pool: FreePool,
    pub create_event: usize,
    pub set_timer: usize,
    pub wait_for_event: usize,
    pub signal_event: usize,
    pub close_event: usize,
    pub check_event: usize,
    pub install_protocol_interface: usize,
    pub reinstall_protocol_interface: usize,
    pub uninstall_protocol_interface: usize,
    pub handle_protocol: HandleProtocol,
    pub reserved: usize,
}

#[repr(C)]
pub struct SystemTable {
    pub header: TableHeader,
    pub firmware_vendor: *const u16,
    pub firmware_revision: u32,
    pub _pad: u32,
    pub console_in_handle: Handle,
    pub con_in: usize,
    pub console_out_handle: Handle,
    pub con_out: *mut SimpleTextOutputProtocol,
    pub standard_error_handle: Handle,
    pub std_err: *mut SimpleTextOutputProtocol,
    pub runtime_services: usize,
    pub boot_services: *mut BootServices,
    pub number_of_table_entries: usize,
    pub configuration_table: usize,
}

#[repr(C)]
struct LoadedImageProtocol {
    revision: u32,
    parent_handle: Handle,
    system_table: *mut SystemTable,
    device_handle: Handle,
    file_path: usize,
    reserved: *mut c_void,
    load_options_size: u32,
    load_options: *mut c_void,
    image_base: *mut c_void,
    image_size: u64,
    image_code_type: u32,
    image_data_type: u32,
    unload: usize,
}

type FileOpen = extern "efiapi" fn(
    this: *mut FileProtocol,
    new_handle: *mut *mut FileProtocol,
    file_name: *const u16,
    open_mode: u64,
    attributes: u64,
) -> Status;
type FileClose = extern "efiapi" fn(this: *mut FileProtocol) -> Status;
type FileRead =
    extern "efiapi" fn(this: *mut FileProtocol, size: *mut usize, buffer: *mut c_void) -> Status;
type FileGetPosition =
    extern "efiapi" fn(this: *mut FileProtocol, position: *mut u64) -> Status;
type FileSetPosition = extern "efiapi" fn(this: *mut FileProtocol, position: u64) -> Status;

#[repr(C)]
struct FileProtocol {
    revision: u64,
    open: FileOpen,
    close: FileClose,
    delete: usize,
    read: FileRead,
    write: usize,
    get_position: FileGetPosition,
    set_position: FileSetPosition,
    get_info: usize,
    set_info: usize,
    flush: usize,
}

type OpenVolume = extern "efiapi" fn(
    this: *mut SimpleFileSystemProtocol,
    root: *mut *mut FileProtocol,
) -> Status;

#[repr(C)]
struct SimpleFileSystemProtocol {
    revision: u64,
    open_volume: OpenVolume,
}

pub struct Console {
    out: *mut SimpleTextOutputProtocol,
}

impl Console {
    pub unsafe fn from_system_table(system_table: *mut SystemTable) -> Option<Self> {
        if system_table.is_null() {
            return None;
        }
        let out = unsafe { (*system_table).con_out };
        if out.is_null() {
            None
        } else {
            Some(Self { out })
        }
    }

    pub fn write(&mut self, text: &str) {
        for byte in text.bytes() {
            let wide = [byte as u16, 0];
            unsafe {
                ((*self.out).output_string)(self.out, wide.as_ptr());
            }
        }
        debug_write(text);
    }
}

pub struct KernelFile {
    ptr: *mut u8,
    len: usize,
}

impl KernelFile {
    pub fn as_slice(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.ptr, self.len) }
    }
}

pub unsafe fn load_kernel(
    image_handle: Handle,
    system_table: *mut SystemTable,
) -> Result<KernelFile, Status> {
    if system_table.is_null() {
        return Err(EFI_INVALID_PARAMETER);
    }

    let boot_services = unsafe { (*system_table).boot_services };
    if boot_services.is_null() {
        return Err(EFI_LOAD_ERROR);
    }

    let mut loaded_image_raw: *mut c_void = null_mut();
    let status = unsafe {
        ((*boot_services).handle_protocol)(
            image_handle,
            &LOADED_IMAGE_PROTOCOL_GUID,
            &mut loaded_image_raw,
        )
    };
    if status != EFI_SUCCESS || loaded_image_raw.is_null() {
        return Err(status);
    }

    let loaded_image = loaded_image_raw as *mut LoadedImageProtocol;
    let device_handle = unsafe { (*loaded_image).device_handle };

    let mut fs_raw: *mut c_void = null_mut();
    let status = unsafe {
        ((*boot_services).handle_protocol)(
            device_handle,
            &SIMPLE_FILE_SYSTEM_PROTOCOL_GUID,
            &mut fs_raw,
        )
    };
    if status != EFI_SUCCESS || fs_raw.is_null() {
        return Err(status);
    }

    let fs = fs_raw as *mut SimpleFileSystemProtocol;
    let mut root: *mut FileProtocol = null_mut();
    let status = unsafe { ((*fs).open_volume)(fs, &mut root) };
    if status != EFI_SUCCESS || root.is_null() {
        return Err(status);
    }

    let mut file: *mut FileProtocol = null_mut();
    let status =
        unsafe { ((*root).open)(root, &mut file, KERNEL_PATH.as_ptr(), FILE_MODE_READ, 0) };
    if status != EFI_SUCCESS || file.is_null() {
        unsafe {
            ((*root).close)(root);
        }
        return Err(status);
    }

    let status = unsafe { ((*file).set_position)(file, u64::MAX) };
    if status != EFI_SUCCESS {
        unsafe {
            ((*file).close)(file);
            ((*root).close)(root);
        }
        return Err(status);
    }

    let mut file_size = 0u64;
    let status = unsafe { ((*file).get_position)(file, &mut file_size) };
    if status != EFI_SUCCESS || file_size == 0 || file_size > usize::MAX as u64 {
        unsafe {
            ((*file).close)(file);
            ((*root).close)(root);
        }
        return Err(if status == EFI_SUCCESS {
            EFI_LOAD_ERROR
        } else {
            status
        });
    }

    let status = unsafe { ((*file).set_position)(file, 0) };
    if status != EFI_SUCCESS {
        unsafe {
            ((*file).close)(file);
            ((*root).close)(root);
        }
        return Err(status);
    }

    let len = file_size as usize;
    let mut buffer: *mut c_void = null_mut();
    let status = unsafe { ((*boot_services).allocate_pool)(EFI_LOADER_DATA, len, &mut buffer) };
    if status != EFI_SUCCESS || buffer.is_null() {
        unsafe {
            ((*file).close)(file);
            ((*root).close)(root);
        }
        return Err(if status == EFI_SUCCESS {
            EFI_OUT_OF_RESOURCES
        } else {
            status
        });
    }

    let mut total = 0usize;
    while total < len {
        let mut chunk = len - total;
        let status = unsafe {
            ((*file).read)(
                file,
                &mut chunk,
                (buffer as *mut u8).add(total) as *mut c_void,
            )
        };
        if status != EFI_SUCCESS || chunk == 0 {
            unsafe {
                ((*boot_services).free_pool)(buffer);
                ((*file).close)(file);
                ((*root).close)(root);
            }
            return Err(if status == EFI_SUCCESS {
                EFI_LOAD_ERROR
            } else {
                status
            });
        }
        total += chunk;
    }

    unsafe {
        ((*file).close)(file);
        ((*root).close)(root);
    }

    Ok(KernelFile {
        ptr: buffer as *mut u8,
        len,
    })
}

fn debug_write(text: &str) {
    for byte in text.bytes() {
        unsafe {
            core::arch::asm!(
                "out dx, al",
                in("dx") 0xE9u16,
                in("al") byte,
                options(nomem, nostack, preserves_flags)
            );
        }
    }
}
