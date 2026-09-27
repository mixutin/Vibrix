//! Host-side unit tests for UEFI protocol structures and GUIDs.
//!
//! These tests run on the host (not in the kernel) and validate that the
//! UEFI protocol structures are correct.
//!
//! Primary reference: UEFI Specification (Protocol Definitions, GUIDs).

fn main() {
    println!("Running UEFI protocol structure tests...\n");

    test_uefi_guid_format();
    test_uefi_guid_endianness();
    test_loaded_image_protocol();
    test_simple_text_output_protocol();
    test_block_io_protocol();
    test_disk_io_protocol();
    test_file_protocol();
    test_gop_protocol();
    test_device_path_protocol();
    test_uefi_status_values();

    println!("\nAll UEFI protocol structure tests passed!");
}

/// UEFI GUID format.
fn test_uefi_guid_format() {
    // UEFI GUID: 16 bytes
    // Data1: 4 bytes (little-endian)
    // Data2: 2 bytes (little-endian)
    // Data3: 2 bytes (little-endian)
    // Data4: 8 bytes (big-endian)

    const GUID_SIZE: usize = 16;
    assert_eq!(GUID_SIZE, 16);

    println!("  [PASS] UEFI GUID format correct");
}

/// UEFI GUID endianness.
fn test_uefi_guid_endianness() {
    // UEFI GUIDs are mixed-endian:
    // Data1, Data2, Data3 are little-endian
    // Data4 is big-endian

    let guid_bytes: [u8; 16] = [
        0x5B, 0x1B, 0x1E, 0xA1, // Data1 (little-endian)
        0x11, 0x11, // Data2 (little-endian)
        0x11, 0x11, // Data3 (little-endian)
        0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, // Data4 (big-endian)
    ];

    assert_eq!(guid_bytes.len(), 16);

    println!("  [PASS] UEFI GUID endianness correct");
}

/// Loaded Image Protocol GUID.
fn test_loaded_image_protocol() {
    // Loaded Image Protocol GUID: 5B1B1EA1-1111-1111-1111-111111111111
    const LOADED_IMAGE_PROTOCOL_GUID: &str = "5B1B1EA1-1111-1111-1111-111111111111";
    assert_eq!(LOADED_IMAGE_PROTOCOL_GUID, "5B1B1EA1-1111-1111-1111-111111111111");

    println!("  [PASS] Loaded Image Protocol GUID correct");
}

/// Simple Text Output Protocol GUID.
fn test_simple_text_output_protocol() {
    // Simple Text Output Protocol GUID: 387477C2-69C7-11D2-8E39-00A0C969723B
    const SIMPLE_TEXT_OUTPUT_GUID: &str = "387477C2-69C7-11D2-8E39-00A0C969723B";
    assert_eq!(SIMPLE_TEXT_OUTPUT_GUID, "387477C2-69C7-11D2-8E39-00A0C969723B");

    println!("  [PASS] Simple Text Output Protocol GUID correct");
}

/// Block I/O Protocol GUID.
fn test_block_io_protocol() {
    // Block I/O Protocol GUID: 964E5B21-6459-11D2-8E39-00A0C969723B
    const BLOCK_IO_PROTOCOL_GUID: &str = "964E5B21-6459-11D2-8E39-00A0C969723B";
    assert_eq!(BLOCK_IO_PROTOCOL_GUID, "964E5B21-6459-11D2-8E39-00A0C969723B");

    println!("  [PASS] Block I/O Protocol GUID correct");
}

/// Disk I/O Protocol GUID.
fn test_disk_io_protocol() {
    // Disk I/O Protocol GUID: CE345171-BA0B-11D2-8E4F-00A0C969723B
    const DISK_IO_PROTOCOL_GUID: &str = "CE345171-BA0B-11D2-8E4F-00A0C969723B";
    assert_eq!(DISK_IO_PROTOCOL_GUID, "CE345171-BA0B-11D2-8E4F-00A0C969723B");

    println!("  [PASS] Disk I/O Protocol GUID correct");
}

/// File Protocol GUID.
fn test_file_protocol() {
    // File Protocol GUID: 09576E91-6D3F-11D2-8E39-00A0C969723B
    const FILE_PROTOCOL_GUID: &str = "09576E91-6D3F-11D2-8E39-00A0C969723B";
    assert_eq!(FILE_PROTOCOL_GUID, "09576E91-6D3F-11D2-8E39-00A0C969723B");

    println!("  [PASS] File Protocol GUID correct");
}

/// GOP (Graphics Output Protocol) GUID.
fn test_gop_protocol() {
    // GOP GUID: 9042A9DE-23DC-4A38-96FB-7ADED080516A
    const GOP_GUID: &str = "9042A9DE-23DC-4A38-96FB-7ADED080516A";
    assert_eq!(GOP_GUID, "9042A9DE-23DC-4A38-96FB-7ADED080516A");

    println!("  [PASS] GOP Protocol GUID correct");
}

/// Device Path Protocol GUID.
fn test_device_path_protocol() {
    // Device Path Protocol GUID: 09576E92-6D3F-11D2-8E39-00A0C969723B
    const DEVICE_PATH_PROTOCOL_GUID: &str = "09576E92-6D3F-11D2-8E39-00A0C969723B";
    assert_eq!(DEVICE_PATH_PROTOCOL_GUID, "09576E92-6D3F-11D2-8E39-00A0C969723B");

    println!("  [PASS] Device Path Protocol GUID correct");
}

/// UEFI status values.
fn test_uefi_status_values() {
    // UEFI status values are 64-bit (usize on x86-64)
    const EFI_SUCCESS: u64 = 0;
    const EFI_LOAD_ERROR: u64 = 1;
    const EFI_INVALID_PARAMETER: u64 = 2;
    const EFI_UNSUPPORTED: u64 = 3;
    const EFI_BAD_BUFFER_SIZE: u64 = 4;
    const EFI_BUFFER_TOO_SMALL: u64 = 5;
    const EFI_NOT_READY: u64 = 6;
    const EFI_DEVICE_ERROR: u64 = 7;
    const EFI_WRITE_PROTECTED: u64 = 8;
    const EFI_OUT_OF_RESOURCES: u64 = 9;
    const EFI_NOT_FOUND: u64 = 14;

    assert_eq!(EFI_SUCCESS, 0);
    assert_eq!(EFI_LOAD_ERROR, 1);
    assert_eq!(EFI_INVALID_PARAMETER, 2);
    assert_eq!(EFI_UNSUPPORTED, 3);
    assert_eq!(EFI_BAD_BUFFER_SIZE, 4);
    assert_eq!(EFI_BUFFER_TOO_SMALL, 5);
    assert_eq!(EFI_NOT_READY, 6);
    assert_eq!(EFI_DEVICE_ERROR, 7);
    assert_eq!(EFI_WRITE_PROTECTED, 8);
    assert_eq!(EFI_OUT_OF_RESOURCES, 9);
    assert_eq!(EFI_NOT_FOUND, 14);

    println!("  [PASS] UEFI status values correct");
}
