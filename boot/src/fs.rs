//! Clean-room EFI Simple Filesystem access for the Vibrix bootloader.
//!
//! Implements just enough of the UEFI Simple File System and File protocols
//! to open the ESP root directory, open `kernel.elf`, and read its contents
//! into a caller-provided buffer. No external crates.
//!
//! Primary reference: UEFI Specification 2.10, EFI_SIMPLE_FILE_SYSTEM_PROTOCOL
//! and EFI_FILE_PROTOCOL.

/// EFI GUID (mixed-endian: first three fields are little-endian).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Guid {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

/// EFI_SIMPLE_FILE_SYSTEM_PROTOCOL GUID.
pub const SIMPLE_FILE_SYSTEM_PROTOCOL_GUID: Guid = Guid {
    data1: 0x0964e5b22,
    data2: 0x6459,
    data3: 0x11d2,
    data4: [0x8e, 0x39, 0x00, 0xa0, 0xc9, 0x69, 0x72, 0x3b],
};

/// EFI_FILE_PROTOCOL GUID.
pub const FILE_PROTOCOL_GUID: Guid = Guid {
    data1: 0x09576e92,
    data2: 0x6d3f,
    data3: 0x11d2,
    data4: [0x8e, 0x39, 0x00, 0xa0, 0xc9, 0x69, 0x72, 0x3b],
};

/// EFI_FILE_MODE_READ: open for reading only.
const EFI_FILE_MODE_READ: u64 = 0x0000000000000001;

/// EFI_FILE_INFO GUID (for querying file size).
pub const FILE_INFO_GUID: Guid = Guid {
    data1: 0x09576e91,
    data2: 0x6d3f,
    data3: 0x11d2,
    data4: [0x8e, 0x39, 0x00, 0xa0, 0xc9, 0x69, 0x72, 0x3b],
};

/// EFI_FILE_INFO structure (variable-length due to filename at the end).
/// We only need the fixed prefix to read `file_size`.
#[repr(C)]
pub struct EfiFileInfo {
    pub size: u64,
    pub file_size: u64,
    pub physical_size: u64,
    pub create_time: u64, // EFI_TIME (simplified to u64 for alignment)
    pub last_access_time: u64,
    pub modification_time: u64,
    pub attribute: u64,
    // file_name: CHAR16[] follows
}

/// Opaque handle type for EFI file handles.
pub type FileHandle = *mut core::ffi::c_void;

/// EFI_SIMPLE_FILE_SYSTEM_PROTOCOL (UEFI Specification 2.10 §12.4).
#[repr(C)]
pub struct SimpleFileSystemProtocol {
    pub revision: u64,
    pub open_volume: extern "efiapi" fn(
        this: *mut SimpleFileSystemProtocol,
        root: *mut *mut FileProtocol,
    ) -> usize,
}

/// EFI_FILE_PROTOCOL (UEFI Specification 2.10 §12.5).
///
/// Only the fields we need are modeled. The full protocol has more
/// function pointers (Write, SetPosition, GetPosition, Flush, etc.)
/// but we only use Open, Read, GetInfo, and Close.
#[repr(C)]
pub struct FileProtocol {
    pub revision: u64,
    pub open: extern "efiapi" fn(
        this: *mut FileProtocol,
        new_handle: *mut *mut FileProtocol,
        file_name: *const u16,
        open_mode: u64,
        attributes: u64,
    ) -> usize,
    pub close: extern "efiapi" fn(this: *mut FileProtocol) -> usize,
    pub delete: usize,
    pub read: extern "efiapi" fn(
        this: *mut FileProtocol,
        buffer_size: *mut usize,
        buffer: *mut core::ffi::c_void,
    ) -> usize,
    pub write: usize,
    pub get_position: usize,
    pub set_position: usize,
    pub get_info: extern "efiapi" fn(
        this: *mut FileProtocol,
        information_type: *const Guid,
        buffer_size: *mut usize,
        buffer: *mut core::ffi::c_void,
    ) -> usize,
    pub set_info: usize,
    pub flush: usize,
}

/// Errors returned by filesystem operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsError {
    /// Protocol GUID not found on the device.
    ProtocolNotFound,
    /// OpenVolume failed.
    OpenVolumeFailed,
    /// File open failed (file not found, access denied, etc.).
    OpenFailed,
    /// GetInfo failed.
    GetInfoFailed,
    /// Read failed.
    ReadFailed,
    /// Buffer too small for file contents.
    BufferTooSmall,
    /// Close failed.
    CloseFailed,
}

impl core::fmt::Display for FsError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            FsError::ProtocolNotFound => write!(f, "SimpleFileSystem protocol not found"),
            FsError::OpenVolumeFailed => write!(f, "OpenVolume failed"),
            FsError::OpenFailed => write!(f, "file open failed"),
            FsError::GetInfoFailed => write!(f, "GetInfo failed"),
            FsError::ReadFailed => write!(f, "read failed"),
            FsError::BufferTooSmall => write!(f, "buffer too small for file"),
            FsError::CloseFailed => write!(f, "close failed"),
        }
    }
}

/// Locate the EFI_SIMPLE_FILE_SYSTEM_PROTOCOL on a device handle.
///
/// # Safety
///
/// `boot_services` must be a valid pointer to the EFI Boot Services table,
/// and `device_handle` must be a valid device handle.
pub unsafe fn locate_simple_file_system(
    boot_services: *const core::ffi::c_void,
    _device_handle: *mut core::ffi::c_void,
) -> Result<*mut SimpleFileSystemProtocol, FsError> {
    // Boot Services layout (UEFI Specification 2.10 §4.4.1):
    // offset 0x00: Hdr (48 bytes)
    // offset 0x30: RaiseTPL
    // offset 0x38: RestoreTPL
    // offset 0x40: AllocatePages
    // offset 0x48: FreePages
    // offset 0x50: GetMemoryMap
    // offset 0x58: AllocatePool
    // offset 0x60: FreePool
    // offset 0x68: CreateEvent
    // offset 0x70: SetTimer
    // offset 0x78: WaitForEvent
    // offset 0x80: SignalEvent
    // offset 0x88: CloseEvent
    // offset 0x90: CheckEvent
    // offset 0x98: InstallProtocolInterface
    // offset 0xA0: ReinstallProtocolInterface
    // offset 0xA8: UninstallProtocolInterface
    // offset 0xB0: HandleProtocol
    // offset 0xB8: Reserved
    // offset 0xC0: RegisterProtocolNotify
    // offset 0xC8: LocateHandle
    // offset 0xD0: LocateDevicePath
    // offset 0xD8: InstallConfigurationTable
    // offset 0xE0: LoadImage
    // offset 0xE8: StartImage
    // offset 0xF0: Exit
    // offset 0xF8: UnloadImage
    // offset 0x100: ExitBootServices
    // offset 0x108: GetNextMonotonicCount
    // offset 0x110: Stall
    // offset 0x118: SetWatchdogTimer
    // offset 0x120: ConnectController
    // offset 0x128: DisconnectController
    // offset 0x130: OpenProtocol
    // offset 0x138: CloseProtocol
    // offset 0x140: OpenProtocolInformation
    // offset 0x148: ProtocolsPerHandle
    // offset 0x150: LocateHandleBuffer
    // offset 0x158: LocateProtocol
    // offset 0x160: InstallMultipleProtocolInterfaces
    // offset 0x168: UninstallMultipleProtocolInterfaces

    // LocateProtocol is at offset 0x158
    let locate_protocol_ptr = unsafe { (boot_services as *const usize).add(0x158 / 8) };
    let locate_protocol: extern "efiapi" fn(
        protocol: *const Guid,
        registration: *const core::ffi::c_void,
        interface: *mut *mut core::ffi::c_void,
    ) -> usize = unsafe { core::mem::transmute(*locate_protocol_ptr) };

    let mut interface: *mut core::ffi::c_void = core::ptr::null_mut();
    let status = locate_protocol(
        &SIMPLE_FILE_SYSTEM_PROTOCOL_GUID,
        core::ptr::null(),
        &mut interface,
    );

    if status != 0 {
        return Err(FsError::ProtocolNotFound);
    }

    Ok(interface as *mut SimpleFileSystemProtocol)
}

/// Open the root directory of a volume.
///
/// # Safety
///
/// `fs` must be a valid pointer to a SimpleFileSystemProtocol.
pub unsafe fn open_root(fs: *mut SimpleFileSystemProtocol) -> Result<*mut FileProtocol, FsError> {
    let mut root: *mut FileProtocol = core::ptr::null_mut();
    let status = unsafe { ((*fs).open_volume)(fs, &mut root) };

    if status != 0 {
        return Err(FsError::OpenVolumeFailed);
    }

    Ok(root)
}

/// Open a file by name relative to a directory.
///
/// # Safety
///
/// `dir` must be a valid FileProtocol handle, and `name` must be a valid
/// null-terminated UTF-16 string.
pub unsafe fn open_file(
    dir: *mut FileProtocol,
    name: *const u16,
) -> Result<*mut FileProtocol, FsError> {
    let mut file: *mut FileProtocol = core::ptr::null_mut();
    let status = unsafe { ((*dir).open)(dir, &mut file, name, EFI_FILE_MODE_READ, 0) };

    if status != 0 {
        return Err(FsError::OpenFailed);
    }

    Ok(file)
}

/// Query file information (specifically, the file size).
///
/// # Safety
///
/// `file` must be a valid FileProtocol handle.
pub unsafe fn get_file_size(file: *mut FileProtocol) -> Result<u64, FsError> {
    let mut info_buffer = [0u8; 128]; // Enough for EfiFileInfo prefix + some filename
    let mut buffer_size = info_buffer.len();

    let status = unsafe {
        ((*file).get_info)(
            file,
            &FILE_INFO_GUID,
            &mut buffer_size,
            info_buffer.as_mut_ptr() as *mut core::ffi::c_void,
        )
    };

    if status != 0 {
        return Err(FsError::GetInfoFailed);
    }

    // Read file_size from the EfiFileInfo structure (offset 8).
    let file_size = u64::from_le_bytes([
        info_buffer[8],
        info_buffer[9],
        info_buffer[10],
        info_buffer[11],
        info_buffer[12],
        info_buffer[13],
        info_buffer[14],
        info_buffer[15],
    ]);

    Ok(file_size)
}

/// Read the entire contents of a file into a buffer.
///
/// Returns the number of bytes read. The buffer must be at least as large
/// as the file (query with `get_file_size` first).
///
/// # Safety
///
/// `file` must be a valid FileProtocol handle, and `buffer` must point to
/// at least `buffer.len()` bytes of writable memory.
pub unsafe fn read_file(file: *mut FileProtocol, buffer: &mut [u8]) -> Result<usize, FsError> {
    let mut buffer_size = buffer.len();

    let status = unsafe {
        ((*file).read)(
            file,
            &mut buffer_size,
            buffer.as_mut_ptr() as *mut core::ffi::c_void,
        )
    };

    if status != 0 {
        return Err(FsError::ReadFailed);
    }

    Ok(buffer_size)
}

/// Close a file handle.
///
/// # Safety
///
/// `file` must be a valid FileProtocol handle.
pub unsafe fn close_file(file: *mut FileProtocol) -> Result<(), FsError> {
    let status = unsafe { ((*file).close)(file) };
    if status != 0 {
        return Err(FsError::CloseFailed);
    }
    Ok(())
}

/// High-level helper: open a file by name from the root of a volume and
/// read its entire contents into a buffer.
///
/// # Safety
///
/// All pointers must be valid. See individual function documentation.
pub unsafe fn read_whole_file(
    fs: *mut SimpleFileSystemProtocol,
    name: *const u16,
    buffer: &mut [u8],
) -> Result<usize, FsError> {
    let root = unsafe { open_root(fs) }?;
    let file = unsafe { open_file(root, name) }?;

    // Close the root directory immediately; we don't need it anymore.
    // Ignore close errors for the root since we already have the file.
    let _ = unsafe { ((*root).close)(root) };

    let file_size = unsafe { get_file_size(file) }?;
    if file_size > buffer.len() as u64 {
        unsafe { close_file(file) }?;
        return Err(FsError::BufferTooSmall);
    }

    let bytes_read = unsafe { read_file(file, buffer) }?;
    unsafe { close_file(file) }?;

    Ok(bytes_read)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_guid_layout() {
        // Verify GUID sizes and basic layout
        assert_eq!(core::mem::size_of::<Guid>(), 16);
    }

    #[test]
    fn test_file_info_layout() {
        // EfiFileInfo: size(8) + file_size(8) + physical_size(8) + ...
        // file_size should be at offset 8
        let info = EfiFileInfo {
            size: 128,
            file_size: 4096,
            physical_size: 4096,
            create_time: 0,
            last_access_time: 0,
            modification_time: 0,
            attribute: 0,
        };
        let ptr = &info as *const _ as *const u8;
        unsafe {
            let fs = u64::from_le_bytes([
                *ptr.add(8),
                *ptr.add(9),
                *ptr.add(10),
                *ptr.add(11),
                *ptr.add(12),
                *ptr.add(13),
                *ptr.add(14),
                *ptr.add(15),
            ]);
            assert_eq!(fs, 4096);
        }
    }

    #[test]
    fn test_simple_file_system_protocol_layout() {
        // Verify the struct is the right size
        assert_eq!(core::mem::size_of::<SimpleFileSystemProtocol>(), 16);
    }

    #[test]
    fn test_file_protocol_layout() {
        // FileProtocol: revision(8) + 10 function pointers = 11 * 8 = 88 bytes
        assert_eq!(core::mem::size_of::<FileProtocol>(), 88);
    }

    #[test]
    fn test_error_display() {
        assert_eq!(
            format!("{}", FsError::ProtocolNotFound),
            "SimpleFileSystem protocol not found"
        );
        assert_eq!(format!("{}", FsError::OpenFailed), "file open failed");
        assert_eq!(
            format!("{}", FsError::BufferTooSmall),
            "buffer too small for file"
        );
    }
}
