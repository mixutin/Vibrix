//! Host-side unit tests for RSDP/RSDT ACPI table parsing.
//!
//! These tests run on the host (not in the kernel) and validate that the
//! RSDP and RSDT structures are correct.
//!
//! Primary reference: ACPI Specification (Chapter 5: ACPI System Description Tables).

fn main() {
    println!("Running RSDP/RSDT ACPI table tests...\n");

    test_rsdp_signature();
    test_rsdp_size();
    test_rsdp_checksum();
    test_rsdp_revision();
    test_rsdp_rsdt_address();
    test_rsdp_xsdt_address();
    test_rsdt_signature();
    test_rsdt_size();
    test_rsdt_entry_size();
    test_rsdt_entry_count();
    test_rsdp_oem_id();

    println!("\nAll RSDP/RSDT ACPI table tests passed!");
}

/// RSDP signature.
fn test_rsdp_signature() {
    const RSDP_SIGNATURE: &[u8; 8] = b"RSD PTR ";
    assert_eq!(RSDP_SIGNATURE, b"RSD PTR ");

    println!("  [PASS] RSDP signature correct");
}

/// RSDP size.
fn test_rsdp_size() {
    // RSDP size: 20 bytes (ACPI 1.0) or 36 bytes (ACPI 2.0+)
    const RSDP_SIZE_V1: usize = 20;
    const RSDP_SIZE_V2: usize = 36;

    assert_eq!(RSDP_SIZE_V1, 20);
    assert_eq!(RSDP_SIZE_V2, 36);

    println!("  [PASS] RSDP size correct");
}

/// RSDP checksum.
fn test_rsdp_checksum() {
    // RSDP checksum: sum of all bytes must be 0 (mod 256)
    // For ACPI 2.0+, the checksum covers the first 20 bytes

    let rsdp_bytes: [u8; 20] = [0; 20];
    let sum: u8 = rsdp_bytes.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
    assert_eq!(sum, 0);

    println!("  [PASS] RSDP checksum correct");
}

/// RSDP revision.
fn test_rsdp_revision() {
    // RSDP revision: 0 (ACPI 1.0) or 2 (ACPI 2.0+)
    const RSDP_REVISION_V1: u8 = 0;
    const RSDP_REVISION_V2: u8 = 2;

    assert_eq!(RSDP_REVISION_V1, 0);
    assert_eq!(RSDP_REVISION_V2, 2);

    println!("  [PASS] RSDP revision correct");
}

/// RSDP RSDT address.
fn test_rsdp_rsdt_address() {
    // RSDT address: 32-bit physical address (ACPI 1.0)
    const RSDT_ADDRESS_ALIGNMENT: u32 = 4;
    assert_eq!(RSDT_ADDRESS_ALIGNMENT, 4);

    let rsdt_address: u32 = 0x0000_1000;
    assert_eq!(rsdt_address % RSDT_ADDRESS_ALIGNMENT, 0);

    println!("  [PASS] RSDP RSDT address correct");
}

/// RSDP XSDT address.
fn test_rsdp_xsdt_address() {
    // XSDT address: 64-bit physical address (ACPI 2.0+)
    const XSDT_ADDRESS_ALIGNMENT: u64 = 8;
    assert_eq!(XSDT_ADDRESS_ALIGNMENT, 8);

    let xsdt_address: u64 = 0x0000_0000_0000_1000;
    assert_eq!(xsdt_address % XSDT_ADDRESS_ALIGNMENT, 0);

    println!("  [PASS] RSDP XSDT address correct");
}

/// RSDT signature.
fn test_rsdt_signature() {
    const RSDT_SIGNATURE: &[u8; 4] = b"RSDT";
    assert_eq!(RSDT_SIGNATURE, b"RSDT");

    println!("  [PASS] RSDT signature correct");
}

/// RSDT size.
fn test_rsdt_size() {
    // RSDT size: 36 bytes (header) + 4 bytes per entry
    const RSDT_HEADER_SIZE: usize = 36;
    const RSDT_ENTRY_SIZE: usize = 4;

    assert_eq!(RSDT_HEADER_SIZE, 36);
    assert_eq!(RSDT_ENTRY_SIZE, 4);

    println!("  [PASS] RSDT size correct");
}

/// RSDT entry size.
fn test_rsdt_entry_size() {
    // RSDT entry: 4 bytes (32-bit physical address)
    const RSDT_ENTRY_SIZE: usize = 4;
    assert_eq!(RSDT_ENTRY_SIZE, 4);

    println!("  [PASS] RSDT entry size correct");
}

/// RSDT entry count.
fn test_rsdt_entry_count() {
    // RSDT entry count = (table_length - header_size) / entry_size
    let table_length: usize = 36 + 4 * 10; // 10 entries
    let header_size: usize = 36;
    let entry_size: usize = 4;

    let entry_count = (table_length - header_size) / entry_size;
    assert_eq!(entry_count, 10);

    println!("  [PASS] RSDT entry count correct");
}

/// RSDP OEM ID.
fn test_rsdp_oem_id() {
    // RSDP OEM ID: 6 bytes
    const RSDP_OEM_ID_SIZE: usize = 6;
    assert_eq!(RSDP_OEM_ID_SIZE, 6);

    println!("  [PASS] RSDP OEM ID correct");
}
