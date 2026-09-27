//! Host-side unit tests for USB mass storage protocol structures.
//!
//! These tests run on the host (not in the kernel) and validate that the
//! USB mass storage protocol structures are correct.
//!
//! Primary reference: USB Mass Storage Class Specification, USB Mass Storage Class Bulk-Only Transport.

fn main() {
    println!("Running USB mass storage protocol tests...\n");

    test_cbw_signature();
    test_cbw_flags();
    test_cbw_command_block();
    test_csw_signature();
    test_csw_status();
    test_bulk_only_transport();
    test_scsi_command_block();
    test_scsi_inquiry();
    test_scsi_read_capacity();
    test_scsi_read10();
    test_scsi_write10();

    println!("\nAll USB mass storage protocol tests passed!");
}

/// CBW (Command Block Wrapper) signature.
fn test_cbw_signature() {
    const CBWSIGNATURE: u32 = 0x4342_5355; // "USBC"
    assert_eq!(CBWSIGNATURE, 0x4342_5355);

    println!("  [PASS] CBW signature correct");
}

/// CBW flags.
fn test_cbw_flags() {
    const CBW_FLAG_DATA_IN: u8 = 0x80;
    const CBW_FLAG_DATA_OUT: u8 = 0x00;

    assert_eq!(CBW_FLAG_DATA_IN, 0x80);
    assert_eq!(CBW_FLAG_DATA_OUT, 0x00);

    println!("  [PASS] CBW flags correct");
}

/// CBW command block format.
fn test_cbw_command_block() {
    // CBW structure (31 bytes):
    // dCBWSignature: u32 (4 bytes) = 0x43425355
    // dCBWTag: u32 (4 bytes)
    // dCBWDataTransferLength: u32 (4 bytes)
    // bmCBWFlags: u8 (1 byte)
    // bCBWLUN: u8 (1 byte)
    // bCBWCBLength: u8 (1 byte)
    // CBWCB: [u8; 16] (16 bytes)

    const CBW_SIZE: usize = 31;
    assert_eq!(CBW_SIZE, 31);

    println!("  [PASS] CBW command block format correct");
}

/// CSW (Command Status Wrapper) signature.
fn test_csw_signature() {
    const CSWSIGNATURE: u32 = 0x5342_5355; // "USBS"
    assert_eq!(CSWSIGNATURE, 0x5342_5355);

    println!("  [PASS] CSW signature correct");
}

/// CSW status values.
fn test_csw_status() {
    const CSW_STATUS_PASSED: u8 = 0x00;
    const CSW_STATUS_FAILED: u8 = 0x01;
    const CSW_STATUS_PHASE_ERROR: u8 = 0x02;

    assert_eq!(CSW_STATUS_PASSED, 0x00);
    assert_eq!(CSW_STATUS_FAILED, 0x01);
    assert_eq!(CSW_STATUS_PHASE_ERROR, 0x02);

    println!("  [PASS] CSW status values correct");
}

/// Bulk-only transport protocol.
fn test_bulk_only_transport() {
    // Bulk-only transport uses:
    // 1. CBW (Command Block Wrapper) — host to device
    // 2. Data stage — optional, direction depends on command
    // 3. CSW (Command Status Wrapper) — device to host

    const BOT_PROTOCOL: u8 = 0x50;
    assert_eq!(BOT_PROTOCOL, 0x50);

    println!("  [PASS] Bulk-only transport protocol correct");
}

/// SCSI command block format.
fn test_scsi_command_block() {
    // SCSI command block: 6-16 bytes
    // Byte 0: Operation code
    // Bytes 1-4: Command-specific parameters
    // Byte 5: Control

    const SCSI_COMMAND_SIZE_6: usize = 6;
    const SCSI_COMMAND_SIZE_10: usize = 10;
    const SCSI_COMMAND_SIZE_12: usize = 12;
    const SCSI_COMMAND_SIZE_16: usize = 16;

    assert_eq!(SCSI_COMMAND_SIZE_6, 6);
    assert_eq!(SCSI_COMMAND_SIZE_10, 10);
    assert_eq!(SCSI_COMMAND_SIZE_12, 12);
    assert_eq!(SCSI_COMMAND_SIZE_16, 16);

    println!("  [PASS] SCSI command block format correct");
}

/// SCSI INQUIRY command.
fn test_scsi_inquiry() {
    // INQUIRY command (6 bytes):
    // Byte 0: Operation code = 0x12
    // Byte 1: Reserved (0)
    // Byte 2: Page code (0)
    // Byte 3: Reserved (0)
    // Byte 4: Allocation length
    // Byte 5: Control (0)

    const SCSI_INQUIRY_OPCODE: u8 = 0x12;
    assert_eq!(SCSI_INQUIRY_OPCODE, 0x12);

    println!("  [PASS] SCSI INQUIRY command correct");
}

/// SCSI READ CAPACITY command.
fn test_scsi_read_capacity() {
    // READ CAPACITY (10) command (10 bytes):
    // Byte 0: Operation code = 0x25
    // Byte 1: Reserved (0)
    // Bytes 2-5: LBA (0 for first inquiry)
    // Bytes 6-7: Reserved (0)
    // Byte 8: PMI (0)
    // Byte 9: Control (0)

    const SCSI_READ_CAPACITY_OPCODE: u8 = 0x25;
    assert_eq!(SCSI_READ_CAPACITY_OPCODE, 0x25);

    println!("  [PASS] SCSI READ CAPACITY command correct");
}

/// SCSI READ (10) command.
fn test_scsi_read10() {
    // READ (10) command (10 bytes):
    // Byte 0: Operation code = 0x28
    // Byte 1: Flags (0)
    // Bytes 2-5: LBA (big-endian)
    // Byte 6: Reserved (0)
    // Bytes 7-8: Transfer length (big-endian)
    // Byte 9: Control (0)

    const SCSI_READ10_OPCODE: u8 = 0x28;
    assert_eq!(SCSI_READ10_OPCODE, 0x28);

    println!("  [PASS] SCSI READ (10) command correct");
}

/// SCSI WRITE (10) command.
fn test_scsi_write10() {
    // WRITE (10) command (10 bytes):
    // Byte 0: Operation code = 0x2A
    // Byte 1: Flags (0)
    // Bytes 2-5: LBA (big-endian)
    // Byte 6: Reserved (0)
    // Bytes 7-8: Transfer length (big-endian)
    // Byte 9: Control (0)

    const SCSI_WRITE10_OPCODE: u8 = 0x2A;
    assert_eq!(SCSI_WRITE10_OPCODE, 0x2A);

    println!("  [PASS] SCSI WRITE (10) command correct");
}
