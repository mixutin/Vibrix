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

const LOADED_IMAGE_DEVICE_PATH_PROTOCOL_GUID: Guid = Guid {
    data1: 0xbc62157e,
    data2: 0x3e33,
    data3: 0x4fec,
    data4: [0x99, 0x20, 0x2d, 0x3b, 0x36, 0xd7, 0x50, 0xdf],
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

pub type AllocatePages = extern "efiapi" fn(
    allocation_type: u32,
    memory_type: u32,
    pages: usize,
    memory: *mut u64,
) -> Status;
pub type FreePages = extern "efiapi" fn(memory: u64, pages: usize) -> Status;
pub type ExitBootServices = unsafe extern "efiapi" fn(image: Handle, map_key: usize) -> Status;
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
type AllocatePool =
    extern "efiapi" fn(memory_type: u32, size: usize, buffer: *mut *mut c_void) -> Status;
type FreePool = extern "efiapi" fn(buffer: *mut c_void) -> Status;
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
type LocateHandleBuffer = extern "efiapi" fn(
    search_type: u32,
    protocol: *const Guid,
    search_key: *mut c_void,
    count: *mut usize,
    buffer: *mut *mut Handle,
) -> Status;

const LOCATE_BY_PROTOCOL: u32 = 2;

#[repr(C)]
struct BlockIoMedia {
    media_id: u32,
    removable_media: u8,
    media_present: u8,
    logical_partition: u8,
    read_only: u8,
    write_caching: u8,
    block_size: u32,
    io_align: u32,
    last_block: u64,
    lowest_aligned_lba: u64,
    logical_blocks_per_physical_block: u32,
    optimal_transfer_length_granularity: u32,
}

type BlockReset = extern "efiapi" fn(*mut BlockIoProtocol, u8) -> Status;
type BlockRead = extern "efiapi" fn(
    *mut BlockIoProtocol,
    u32,
    u64,
    usize,
    *mut c_void,
) -> Status;
type BlockWrite = extern "efiapi" fn(
    *mut BlockIoProtocol,
    u32,
    u64,
    usize,
    *const c_void,
) -> Status;
type BlockFlush = extern "efiapi" fn(*mut BlockIoProtocol) -> Status;

#[repr(C)]
struct BlockIoProtocol {
    revision: u64,
    media: *mut BlockIoMedia,
    reset: BlockReset,
    read_blocks: BlockRead,
    write_blocks: BlockWrite,
    flush_blocks: BlockFlush,
}


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
    pub exit_boot_services: ExitBootServices,
    pub get_next_monotonic_count: usize,
    pub stall: usize,
    pub set_watchdog_timer: usize,
    pub connect_controller: usize,
    pub disconnect_controller: usize,
    pub open_protocol: usize,
    pub close_protocol: usize,
    pub open_protocol_information: usize,
    pub protocols_per_handle: usize,
    pub locate_handle_buffer: LocateHandleBuffer,
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

pub const ALLOCATE_ANY_PAGES: u32 = 0;

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

    let (allocate, free) = unsafe { page_services(system_table)? };
    unsafe { allocate_pages_with(allocate, free, pages) }
}

/// Release owned pages. On failure ownership remains with the caller; do not
/// continue a handoff or erase tracking as though the pages had been freed.
/// Physical base zero is permitted here to clean up a policy-invalid allocation.
///
/// # Safety
/// Live firmware tables and an exact currently-owned allocation are required.
pub unsafe fn free_loader_pages(
    system_table: *mut SystemTable,
    physical_address: u64,
    pages: usize,
) -> Result<(), Status> {
    if system_table.is_null() || pages == 0 {
        return Err(EFI_INVALID_PARAMETER);
    }
    let (_, free) = unsafe { page_services(system_table)? };
    unsafe { free_pages_with(free, physical_address, pages) }
}

// Narrow production ABI injection for page-table rollback and host tests.
// SAFETY: any non-null system table and services pointer must be live/readable.
pub(crate) unsafe fn page_services(
    system_table: *mut SystemTable,
) -> Result<(AllocatePages, FreePages), Status> {
    if system_table.is_null() {
        return Err(EFI_INVALID_PARAMETER);
    }
    let services = unsafe { (*system_table).boot_services };
    if services.is_null() {
        return Err(EFI_LOAD_ERROR);
    }
    Ok(unsafe { ((*services).allocate_pages, (*services).free_pages) })
}

// SAFETY: callbacks obey UEFI ABI/allocation contracts and are still available.
pub(crate) unsafe fn allocate_pages_with(
    allocate: AllocatePages,
    free: FreePages,
    pages: usize,
) -> Result<u64, Status> {
    if pages == 0 {
        return Err(EFI_INVALID_PARAMETER);
    }
    let mut physical_address = 0;
    let status = allocate(
        ALLOCATE_ANY_PAGES,
        EFI_LOADER_DATA,
        pages,
        &mut physical_address,
    );
    if status != EFI_SUCCESS {
        return Err(status);
    }
    if physical_address == 0 {
        // Release failure takes precedence over the allocation-policy error.
        unsafe { free_pages_with(free, physical_address, pages)? };
        return Err(EFI_LOAD_ERROR);
    }
    Ok(physical_address)
}

// SAFETY: exact owned base/count and live callback required. Never rejects base 0.
pub(crate) unsafe fn free_pages_with(
    free: FreePages,
    physical_address: u64,
    pages: usize,
) -> Result<(), Status> {
    if pages == 0 {
        return Err(EFI_INVALID_PARAMETER);
    }
    match free(physical_address, pages) {
        EFI_SUCCESS => Ok(()),
        status => Err(status),
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


const MAX_BOOT_DEVICE_PATH_BYTES: usize = 1024;

/// Copy and validate the loaded image's firmware device path while Boot
/// Services are live, returning only firmware-neutral GPT partition facts.
///
/// This helper is not yet a mandatory boot gate because legacy development
/// profiles do not all boot from GPT USB media. The persistent-USB boot path
/// will require it before ExitBootServices.
///
/// # Safety
/// `image_handle` and `system_table` must be live UEFI objects. The Device
/// Path protocol pointer must remain firmware-owned and readable during this
/// call only; no pointer escapes.
#[allow(dead_code)]
fn copy_device_path(raw: *const u8) -> Result<([u8; MAX_BOOT_DEVICE_PATH_BYTES], usize), Status> {
    if raw.is_null() {
        return Err(EFI_LOAD_ERROR);
    }
    let mut bytes = [0u8; MAX_BOOT_DEVICE_PATH_BYTES];
    let mut offset = 0usize;
    let mut nodes = 0usize;
    loop {
        if offset + 4 > bytes.len() || nodes >= 64 {
            return Err(EFI_LOAD_ERROR);
        }
        let header = unsafe { core::slice::from_raw_parts(raw.add(offset), 4) };
        let len = usize::from(u16::from_le_bytes([header[2], header[3]]));
        if len < 4 || offset.checked_add(len).is_none_or(|end| end > bytes.len()) {
            return Err(EFI_LOAD_ERROR);
        }
        let node = unsafe { core::slice::from_raw_parts(raw.add(offset), len) };
        bytes[offset..offset + len].copy_from_slice(node);
        let end = node[0] == crate::uefi_boot_path::DEVICE_PATH_END_TYPE
            && node[1] == crate::uefi_boot_path::DEVICE_PATH_END_ENTIRE_SUBTYPE;
        offset += len;
        nodes += 1;
        if end {
            return Ok((bytes, offset));
        }
    }
}

#[allow(dead_code)]
pub struct FirmwareBootDisk {
    block_io: *mut BlockIoProtocol,
    media_id: u32,
    io_align: u32,
    pub partition: crate::uefi_boot_path::BootPartitionPath,
    pub block_size: u32,
    pub last_block: u64,
}

/// Locate exactly one whole-disk Block I/O handle that is the device-path
/// parent of the loaded image's USB GPT partition.
///
/// # Safety
/// All UEFI pointers are valid only while Boot Services are live. The returned
/// object must be consumed before ExitBootServices and must never be copied
/// into BootInfo or retained by the kernel.
#[allow(dead_code)]
pub unsafe fn discover_boot_whole_disk(
    image_handle: Handle,
    system_table: *mut SystemTable,
) -> Result<FirmwareBootDisk, Status> {
    if image_handle.is_null() || system_table.is_null() {
        return Err(EFI_INVALID_PARAMETER);
    }
    let services = unsafe { (*system_table).boot_services };
    if services.is_null() {
        return Err(EFI_LOAD_ERROR);
    }

    let mut child_raw: *mut c_void = null_mut();
    let status = unsafe {
        ((*services).handle_protocol)(
            image_handle,
            &LOADED_IMAGE_DEVICE_PATH_PROTOCOL_GUID,
            &mut child_raw,
        )
    };
    if status != EFI_SUCCESS || child_raw.is_null() {
        return Err(if status == EFI_SUCCESS { EFI_LOAD_ERROR } else { status });
    }
    let (child, child_len) = copy_device_path(child_raw as *const u8)?;
    let partition =
        crate::uefi_boot_path::parse(&child[..child_len]).map_err(|_| EFI_LOAD_ERROR)?;

    let mut count = 0usize;
    let mut handles: *mut Handle = null_mut();
    let status = unsafe {
        ((*services).locate_handle_buffer)(
            LOCATE_BY_PROTOCOL,
            &BLOCK_IO_PROTOCOL_GUID,
            null_mut(),
            &mut count,
            &mut handles,
        )
    };
    if status != EFI_SUCCESS || handles.is_null() || count == 0 || count > 256 {
        if !handles.is_null() {
            let _ = unsafe { ((*services).free_pool)(handles.cast()) };
        }
        return Err(if status == EFI_SUCCESS { EFI_LOAD_ERROR } else { status });
    }

    let mut found: Option<FirmwareBootDisk> = None;
    for index in 0..count {
        let handle = unsafe { *handles.add(index) };
        let mut block_raw: *mut c_void = null_mut();
        if unsafe {
            ((*services).handle_protocol)(handle, &BLOCK_IO_PROTOCOL_GUID, &mut block_raw)
        } != EFI_SUCCESS
            || block_raw.is_null()
        {
            continue;
        }
        let block = block_raw as *mut BlockIoProtocol;
        let media = unsafe { (*block).media };
        if media.is_null()
            || unsafe { (*media).media_present } == 0
            || unsafe { (*media).logical_partition } != 0
        {
            continue;
        }
        let block_size = unsafe { (*media).block_size };
        if !matches!(block_size, 512 | 4096) {
            continue;
        }

        let mut path_raw: *mut c_void = null_mut();
        if unsafe {
            ((*services).handle_protocol)(handle, &DEVICE_PATH_PROTOCOL_GUID, &mut path_raw)
        } != EFI_SUCCESS
            || path_raw.is_null()
        {
            continue;
        }
        let Ok((parent, parent_len)) = copy_device_path(path_raw as *const u8) else {
            continue;
        };
        let matches = crate::uefi_boot_path::parent_matches(
            &child[..child_len],
            &parent[..parent_len],
        )
        .map_err(|_| EFI_LOAD_ERROR)?;
        if !matches {
            continue;
        }
        if found.is_some() {
            let _ = unsafe { ((*services).free_pool)(handles.cast()) };
            return Err(EFI_LOAD_ERROR);
        }
        let io_align = unsafe { (*media).io_align };
        if io_align > 4096 || (io_align > 1 && !io_align.is_power_of_two()) {
            continue;
        }
        found = Some(FirmwareBootDisk {
            block_io: block,
            media_id: unsafe { (*media).media_id },
            io_align,
            partition,
            block_size,
            last_block: unsafe { (*media).last_block },
        });
    }

    let free_status = unsafe { ((*services).free_pool)(handles.cast()) };
    if free_status != EFI_SUCCESS {
        return Err(free_status);
    }
    found.ok_or(EFI_LOAD_ERROR)
}

/// Read and validate both GPT copies from the matched firmware whole disk.
///
/// All temporary buffers are page-backed EfiLoaderData so they satisfy any
/// supported Block I/O alignment requirement. They are released before this
/// function returns; only the firmware-neutral GUID/extent tuple escapes.
///
/// # Safety
/// `disk` must come from `discover_boot_whole_disk` during the same live
/// Boot Services phase. Its Block I/O protocol and media must remain valid.
#[allow(dead_code)]
pub unsafe fn read_boot_gpt_identity(
    disk: &FirmwareBootDisk,
    system_table: *mut SystemTable,
) -> Result<crate::gpt_identity::Identity, Status> {
    if system_table.is_null()
        || disk.block_io.is_null()
        || disk.last_block < 5
        || !matches!(disk.block_size, 512 | 4096)
    {
        return Err(EFI_INVALID_PARAMETER);
    }
    let block_size = disk.block_size as usize;
    let header_pages = 1usize;
    let primary_header = unsafe { allocate_loader_pages(system_table, header_pages)? };
    let backup_header = match unsafe { allocate_loader_pages(system_table, header_pages) } {
        Ok(value) => value,
        Err(status) => {
            unsafe { free_loader_pages(system_table, primary_header, header_pages)? };
            return Err(status);
        }
    };

    let mut primary_entries = 0u64;
    let mut backup_entries = 0u64;
    let mut entry_pages = 0usize;

    let result = (|| {
        let primary_ptr = usize::try_from(primary_header).map_err(|_| EFI_LOAD_ERROR)? as *mut u8;
        let backup_ptr = usize::try_from(backup_header).map_err(|_| EFI_LOAD_ERROR)? as *mut u8;
        if disk.io_align > 1 {
            let align = disk.io_align as usize;
            if !(primary_ptr as usize).is_multiple_of(align)
                || !(backup_ptr as usize).is_multiple_of(align)
            {
                return Err(EFI_LOAD_ERROR);
            }
        }

        let read = unsafe { (*disk.block_io).read_blocks };
        let status = read(
            disk.block_io,
            disk.media_id,
            1,
            block_size,
            primary_ptr.cast(),
        );
        if status != EFI_SUCCESS {
            return Err(status);
        }
        let status = read(
            disk.block_io,
            disk.media_id,
            disk.last_block,
            block_size,
            backup_ptr.cast(),
        );
        if status != EFI_SUCCESS {
            return Err(status);
        }

        let primary_header_slice =
            unsafe { core::slice::from_raw_parts(primary_ptr.cast_const(), block_size) };
        let backup_header_slice =
            unsafe { core::slice::from_raw_parts(backup_ptr.cast_const(), block_size) };
        let (_, primary_layout) = crate::gpt_identity::validate_header(
            primary_header_slice,
            block_size,
            1,
            disk.last_block,
        )
        .map_err(|_| EFI_LOAD_ERROR)?;
        let (_, backup_layout) = crate::gpt_identity::validate_header(
            backup_header_slice,
            block_size,
            disk.last_block,
            1,
        )
        .map_err(|_| EFI_LOAD_ERROR)?;
        if primary_layout.entry_array_bytes != backup_layout.entry_array_bytes {
            return Err(EFI_LOAD_ERROR);
        }

        let read_bytes = primary_layout
            .entry_array_bytes
            .div_ceil(block_size)
            .checked_mul(block_size)
            .ok_or(EFI_LOAD_ERROR)?;
        entry_pages = read_bytes.div_ceil(4096);
        if entry_pages == 0 {
            return Err(EFI_LOAD_ERROR);
        }
        primary_entries = unsafe { allocate_loader_pages(system_table, entry_pages)? };
        backup_entries = unsafe { allocate_loader_pages(system_table, entry_pages)? };
        let primary_entries_ptr =
            usize::try_from(primary_entries).map_err(|_| EFI_LOAD_ERROR)? as *mut u8;
        let backup_entries_ptr =
            usize::try_from(backup_entries).map_err(|_| EFI_LOAD_ERROR)? as *mut u8;
        if disk.io_align > 1 {
            let align = disk.io_align as usize;
            if !(primary_entries_ptr as usize).is_multiple_of(align)
                || !(backup_entries_ptr as usize).is_multiple_of(align)
            {
                return Err(EFI_LOAD_ERROR);
            }
        }

        let status = read(
            disk.block_io,
            disk.media_id,
            primary_layout.entry_lba,
            read_bytes,
            primary_entries_ptr.cast(),
        );
        if status != EFI_SUCCESS {
            return Err(status);
        }
        let status = read(
            disk.block_io,
            disk.media_id,
            backup_layout.entry_lba,
            read_bytes,
            backup_entries_ptr.cast(),
        );
        if status != EFI_SUCCESS {
            return Err(status);
        }

        let primary_entries_slice = unsafe {
            core::slice::from_raw_parts(
                primary_entries_ptr.cast_const(),
                primary_layout.entry_array_bytes,
            )
        };
        let backup_entries_slice = unsafe {
            core::slice::from_raw_parts(
                backup_entries_ptr.cast_const(),
                backup_layout.entry_array_bytes,
            )
        };
        let identity = crate::gpt_identity::validate_identity(
            block_size,
            disk.last_block,
            primary_header_slice,
            primary_entries_slice,
            backup_header_slice,
            backup_entries_slice,
        )
        .map_err(|_| EFI_LOAD_ERROR)?;

        let expected_last = disk
            .partition
            .partition_start_lba
            .checked_add(disk.partition.partition_size_lba)
            .and_then(|end| end.checked_sub(1))
            .ok_or(EFI_LOAD_ERROR)?;
        if identity.esp_guid != disk.partition.partition_guid
            || identity.esp_first_lba != disk.partition.partition_start_lba
            || identity.esp_last_lba != expected_last
        {
            return Err(EFI_LOAD_ERROR);
        }
        Ok(identity)
    })();

    let mut cleanup_error = None;
    if primary_entries != 0
        && unsafe { free_loader_pages(system_table, primary_entries, entry_pages) }.is_err()
    {
        cleanup_error = Some(EFI_LOAD_ERROR);
    }
    if backup_entries != 0
        && unsafe { free_loader_pages(system_table, backup_entries, entry_pages) }.is_err()
    {
        cleanup_error = Some(EFI_LOAD_ERROR);
    }
    if unsafe { free_loader_pages(system_table, primary_header, header_pages) }.is_err() {
        cleanup_error = Some(EFI_LOAD_ERROR);
    }
    if unsafe { free_loader_pages(system_table, backup_header, header_pages) }.is_err() {
        cleanup_error = Some(EFI_LOAD_ERROR);
    }
    match (result, cleanup_error) {
        (_, Some(status)) => Err(status),
        (value, None) => value,
    }
}

pub unsafe fn loaded_image_boot_partition(
    image_handle: Handle,
    system_table: *mut SystemTable,
) -> Result<crate::uefi_boot_path::BootPartitionPath, Status> {
    if image_handle.is_null() || system_table.is_null() {
        return Err(EFI_INVALID_PARAMETER);
    }
    let services = unsafe { (*system_table).boot_services };
    if services.is_null() {
        return Err(EFI_LOAD_ERROR);
    }
    let mut raw: *mut c_void = null_mut();
    let status = unsafe {
        ((*services).handle_protocol)(
            image_handle,
            &LOADED_IMAGE_DEVICE_PATH_PROTOCOL_GUID,
            &mut raw,
        )
    };
    if status != EFI_SUCCESS || raw.is_null() {
        return Err(if status == EFI_SUCCESS { EFI_LOAD_ERROR } else { status });
    }

    let (bytes, len) = copy_device_path(raw as *const u8)?;
    crate::uefi_boot_path::parse(&bytes[..len]).map_err(|_| EFI_LOAD_ERROR)
}

/// Get the live loader PE/COFF image's mapped physical interval. The
/// transition code must be identity-mapped until it changes CR3 and jumps
/// directly into the higher-half kernel.
///
/// # Safety
/// The image/system-table handles and firmware Loaded Image protocol must
/// remain valid, and the returned image region must stay resident through EBS.
pub unsafe fn loader_image_range(
    image_handle: Handle,
    system_table: *mut SystemTable,
) -> Result<(u64, u64), Status> {
    if image_handle.is_null() || system_table.is_null() {
        return Err(EFI_INVALID_PARAMETER);
    }
    let services = unsafe { (*system_table).boot_services };
    if services.is_null() {
        return Err(EFI_LOAD_ERROR);
    }
    let mut raw: *mut c_void = null_mut();
    let status = unsafe {
        ((*services).handle_protocol)(image_handle, &LOADED_IMAGE_PROTOCOL_GUID, &mut raw)
    };
    if status != EFI_SUCCESS {
        return Err(status);
    }
    if raw.is_null() {
        return Err(EFI_LOAD_ERROR);
    }
    let image = unsafe { &*(raw as *const LoadedImageProtocol) };
    let base = image.image_base as usize as u64;
    if base == 0 || image.image_size == 0 || base.checked_add(image.image_size).is_none() {
        return Err(EFI_LOAD_ERROR);
    }
    Ok((base, image.image_size))
}

/// Cache ExitBootServices' firmware ABI function pointer while boot services
/// are live. After the first exit attempt call **only** GetMemoryMap on retry.
///
/// # Safety
/// The system table must point to live firmware before the first EBS call.
pub unsafe fn exit_boot_services_service(
    system_table: *mut SystemTable,
) -> Result<ExitBootServices, Status> {
    if system_table.is_null() {
        return Err(EFI_INVALID_PARAMETER);
    }
    let services = unsafe { (*system_table).boot_services };
    if services.is_null() {
        return Err(EFI_LOAD_ERROR);
    }
    Ok(unsafe { (*services).exit_boot_services })
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
