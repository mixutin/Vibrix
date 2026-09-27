use core::ffi::c_void;
use core::ptr::null_mut;
use core::slice;

pub type Handle = *mut c_void;
pub type Status = usize;

pub const EFI_SUCCESS: Status = 0;
pub const EFI_ERROR_BIT: Status = 1usize << (usize::BITS - 1);
pub const EFI_LOAD_ERROR: Status = EFI_ERROR_BIT | 1;
pub const EFI_INVALID_PARAMETER: Status = EFI_ERROR_BIT | 2;
pub const EFI_BUFFER_TOO_SMALL: Status = EFI_ERROR_BIT | 5;
pub const EFI_OUT_OF_RESOURCES: Status = EFI_ERROR_BIT | 9;

pub const EFI_LOADER_DATA: u32 = 2;
const FILE_MODE_READ: u64 = 1;

#[derive(Clone, Copy, PartialEq, Eq)]
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

// UEFI Specification 2.9A: ACPI configuration table GUIDs.
const ACPI_20_TABLE_GUID: Guid = Guid {
    data1: 0x8868e871,
    data2: 0xe4f1,
    data3: 0x11d3,
    data4: [0xbc, 0x22, 0x00, 0x80, 0xc7, 0x3c, 0x88, 0x81],
};

const ACPI_10_TABLE_GUID: Guid = Guid {
    data1: 0xeb9d2d30,
    data2: 0x2d88,
    data3: 0x11d3,
    data4: [0x9a, 0x16, 0x00, 0x90, 0x27, 0x3f, 0xc1, 0x4d],
};

#[repr(C)]
struct ConfigurationTable {
    vendor_guid: Guid,
    vendor_table: *const u8,
}

// UEFI Specification 2.11, Graphics Output Protocol.
const GRAPHICS_OUTPUT_PROTOCOL_GUID: Guid = Guid {
    data1: 0x9042a9de,
    data2: 0x23dc,
    data3: 0x4a38,
    data4: [0x96, 0xfb, 0x7a, 0xde, 0xd0, 0x80, 0x51, 0x6a],
};

static KERNEL_PATH: &[u16] = &[
    '\\' as u16,
    'v' as u16,
    'i' as u16,
    'b' as u16,
    'r' as u16,
    'i' as u16,
    'x' as u16,
    '\\' as u16,
    'k' as u16,
    'e' as u16,
    'r' as u16,
    'n' as u16,
    'e' as u16,
    'l' as u16,
    '.' as u16,
    'e' as u16,
    'l' as u16,
    'f' as u16,
    0,
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
    pub output_string: extern "efiapi" fn(*mut SimpleTextOutputProtocol, *const u16) -> Status,
    pub test_string: usize,
    pub query_mode: usize,
    pub set_mode: usize,
    pub set_attribute: usize,
    pub clear_screen: usize,
    pub set_cursor_position: usize,
    pub enable_cursor: usize,
    pub mode: usize,
}

type AllocatePages = extern "efiapi" fn(
    allocation_type: u32,
    memory_type: u32,
    pages: usize,
    memory: *mut u64,
) -> Status;
type FreePages = extern "efiapi" fn(memory: u64, pages: usize) -> Status;
/// UEFI 2.10 section 7.2.3. Firmware may return a larger descriptor stride.
#[repr(C)]
pub struct MemoryDescriptor {
    pub memory_type: u32,
    pub physical_start: u64,
    pub virtual_start: u64,
    pub number_of_pages: u64,
    pub attribute: u64,
}

pub type GetMemoryMap = unsafe extern "efiapi" fn(
    memory_map_size: *mut usize,
    memory_map: *mut MemoryDescriptor,
    map_key: *mut usize,
    descriptor_size: *mut usize,
    descriptor_version: *mut u32,
) -> Status;
pub type AllocatePool =
    extern "efiapi" fn(memory_type: u32, size: usize, buffer: *mut *mut c_void) -> Status;
pub type FreePool = extern "efiapi" fn(buffer: *mut c_void) -> Status;
type HandleProtocol = extern "efiapi" fn(
    handle: Handle,
    protocol: *const Guid,
    interface: *mut *mut c_void,
) -> Status;

type LocateProtocol = extern "efiapi" fn(
    protocol: *const Guid,
    registration: *mut c_void,
    interface: *mut *mut c_void,
) -> Status;

#[repr(C)]
pub struct BootServices {
    pub header: TableHeader,
    pub raise_tpl: usize,
    pub restore_tpl: usize,
    pub allocate_pages: AllocatePages,
    pub free_pages: FreePages,
    pub get_memory_map: GetMemoryMap,
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
    pub register_protocol_notify: usize,
    pub locate_handle: usize,
    pub locate_device_path: usize,
    pub install_configuration_table: usize,
    pub load_image: usize,
    pub start_image: usize,
    pub exit: usize,
    pub unload_image: usize,
    pub exit_boot_services: usize,
    pub get_next_monotonic_count: usize,
    pub stall: usize,
    pub set_watchdog_timer: usize,
    pub connect_controller: usize,
    pub disconnect_controller: usize,
    pub open_protocol: usize,
    pub close_protocol: usize,
    pub open_protocol_information: usize,
    pub protocols_per_handle: usize,
    pub locate_handle_buffer: usize,
    pub locate_protocol: LocateProtocol,
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
type FileGetPosition = extern "efiapi" fn(this: *mut FileProtocol, position: *mut u64) -> Status;
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

type OpenVolume =
    extern "efiapi" fn(this: *mut SimpleFileSystemProtocol, root: *mut *mut FileProtocol) -> Status;

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

/// Discover a validated ACPI RSDP before ExitBootServices.
///
/// # Safety
/// The caller supplies a live firmware SystemTable. Firmware configuration-table
/// entries and their vendor-table pointers must remain mapped while called.
/// The returned physical address requires appropriate kernel mapping before use.
pub unsafe fn find_rsdp(system_table: *mut SystemTable) -> Option<u64> {
    if system_table.is_null() {
        return None;
    }
    let system = unsafe { &*system_table };
    let count = system.number_of_table_entries;
    if count == 0 || count > 4096 || system.configuration_table == 0 {
        return None;
    }
    let table = system.configuration_table as *const ConfigurationTable;

    // ACPI 6.6: prefer the ACPI 2.0+ GUID and fall back to ACPI 1.0.
    for guid in [ACPI_20_TABLE_GUID, ACPI_10_TABLE_GUID] {
        for index in 0..count {
            let entry = unsafe { &*table.add(index) };
            if entry.vendor_guid != guid || entry.vendor_table.is_null() {
                continue;
            }
            let header = unsafe { slice::from_raw_parts(entry.vendor_table, 20) };
            if header.get(..8) != Some(b"RSD PTR ".as_slice()) || !checksum_zero(header) {
                continue;
            }
            if header[15] >= 2 {
                let base = unsafe { slice::from_raw_parts(entry.vendor_table, 36) };
                let length = u32::from_le_bytes([base[20], base[21], base[22], base[23]]) as usize;
                if !(36..=4096).contains(&length) {
                    continue;
                }
                let extended = unsafe { slice::from_raw_parts(entry.vendor_table, length) };
                if !checksum_zero(extended) {
                    continue;
                }
            }
            return Some(entry.vendor_table as usize as u64);
        }
    }
    None
}

fn checksum_zero(bytes: &[u8]) -> bool {
    bytes.iter().fold(0u8, |sum, &byte| sum.wrapping_add(byte)) == 0
}

#[repr(C)]
struct GraphicsOutputProtocol {
    query_mode: usize,
    set_mode: usize,
    blt: usize,
    mode: *const GraphicsOutputProtocolMode,
}

#[repr(C)]
struct GraphicsOutputProtocolMode {
    max_mode: u32,
    mode: u32,
    info: *const GraphicsOutputModeInformation,
    size_of_info: usize,
    framebuffer_base: u64,
    framebuffer_size: usize,
}

#[repr(C)]
struct GraphicsOutputModeInformation {
    version: u32,
    horizontal_resolution: u32,
    vertical_resolution: u32,
    pixel_format: u32,
    pixel_bitmask: [u32; 4],
    pixels_per_scan_line: u32,
}

/// Physical GOP framebuffer details for the future BootInfo.
pub struct Framebuffer {
    pub base: u64,
    pub size: u64,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub format: u32,
}

/// Read the current GOP mode without changing it.
///
/// # Safety
/// The caller provides a live UEFI SystemTable, and the firmware must expose
/// valid protocol/mode/info pointers through LocateProtocol. The framebuffer
/// physical address must not be dereferenced without suitable mappings.
pub unsafe fn discover_framebuffer(system_table: *mut SystemTable) -> Option<Framebuffer> {
    if system_table.is_null() {
        return None;
    }
    let services = unsafe { (*system_table).boot_services };
    if services.is_null() {
        return None;
    }

    let mut raw: *mut c_void = null_mut();
    let status = unsafe {
        ((*services).locate_protocol)(&GRAPHICS_OUTPUT_PROTOCOL_GUID, null_mut(), &mut raw)
    };
    if status != EFI_SUCCESS || raw.is_null() {
        return None;
    }
    let gop = unsafe { &*(raw as *const GraphicsOutputProtocol) };
    if gop.mode.is_null() {
        return None;
    }
    let mode = unsafe { &*gop.mode };
    if mode.info.is_null()
        || mode.size_of_info < core::mem::size_of::<GraphicsOutputModeInformation>()
    {
        return None;
    }
    let info = unsafe { &*mode.info };
    // PixelBltOnly has no usable linear framebuffer; the other current formats
    // use a 32-bit pixel element per the UEFI GOP specification.
    if info.pixel_format >= 3
        || info.horizontal_resolution == 0
        || info.vertical_resolution == 0
        || info.pixels_per_scan_line < info.horizontal_resolution
        || mode.framebuffer_base == 0
    {
        return None;
    }
    let needed = u64::from(info.pixels_per_scan_line)
        .checked_mul(u64::from(info.vertical_resolution))?
        .checked_mul(4)?;
    let size = u64::try_from(mode.framebuffer_size).ok()?;
    if needed > size || mode.framebuffer_base.checked_add(size).is_none() {
        return None;
    }
    Some(Framebuffer {
        base: mode.framebuffer_base,
        size,
        width: info.horizontal_resolution,
        height: info.vertical_resolution,
        stride: info.pixels_per_scan_line,
        format: info.pixel_format,
    })
}

const ALLOCATE_ANY_PAGES: u32 = 0;

/// Allocate loader-owned physical pages that intentionally survive the firmware handoff.
///
/// # Safety
///
/// `system_table` must point to a live UEFI system table whose Boot Services table is
/// valid for the duration of this call. The caller becomes responsible for either freeing
/// the returned allocation before ExitBootServices or reserving it for kernel ownership.
pub unsafe fn allocate_loader_pages(
    system_table: *mut SystemTable,
    pages: usize,
) -> Result<u64, Status> {
    if system_table.is_null() || pages == 0 {
        return Err(EFI_INVALID_PARAMETER);
    }

    let services = unsafe { (*system_table).boot_services };
    if services.is_null() {
        return Err(EFI_LOAD_ERROR);
    }

    let mut physical_address = 0u64;
    let status = unsafe {
        ((*services).allocate_pages)(
            ALLOCATE_ANY_PAGES,
            EFI_LOADER_DATA,
            pages,
            &mut physical_address,
        )
    };
    if status != EFI_SUCCESS {
        return Err(status);
    }
    if physical_address == 0 {
        unsafe {
            ((*services).free_pages)(physical_address, pages);
        }
        return Err(EFI_LOAD_ERROR);
    }

    Ok(physical_address)
}

/// Release pages previously returned by `allocate_loader_pages`.
///
/// # Safety
///
/// `system_table` must still expose live UEFI Boot Services. `physical_address` and
/// `pages` must identify an allocation currently owned by the loader and must not have
/// been freed already.
pub unsafe fn free_loader_pages(
    system_table: *mut SystemTable,
    physical_address: u64,
    pages: usize,
) {
    if system_table.is_null() || pages == 0 {
        return;
    }

    let services = unsafe { (*system_table).boot_services };
    if services.is_null() {
        return;
    }

    unsafe {
        ((*services).free_pages)(physical_address, pages);
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

/// QEMU-only port output: no firmware calls or allocations.
pub fn debug_write(text: &str) {
    #[cfg(feature = "qemu-debugcon")]
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

    #[cfg(not(feature = "qemu-debugcon"))]
    let _ = text;
}
