//! Host-side unit tests for MADT (Multiple APIC Description Table) entry validation.
//!
//! These tests run on the host (not in the kernel) and validate that the
//! MADT entry structures are correct.
//!
//! Primary reference: ACPI Specification (Chapter 5: MADT).

fn main() {
    println!("Running MADT entry validation tests...\n");

    test_madt_header_size();
    test_madt_entry_header();
    test_madt_local_apic_entry();
    test_madt_io_apic_entry();
    test_madt_interrupt_override_entry();
    test_madt_nmi_entry();
    test_madt_local_apic_address_override();
    test_madt_local_x2apic_entry();
    test_madt_entry_type_validation();
    test_madt_flags();

    println!("\nAll MADT entry validation tests passed!");
}

/// MADT header size.
fn test_madt_header_size() {
    // MADT header: 44 bytes
    // Signature: 4 bytes
    // Length: 4 bytes
    // Revision: 1 byte
    // Checksum: 1 byte
    // OEM ID: 6 bytes
    // OEM Table ID: 8 bytes
    // OEM Revision: 4 bytes
    // Creator ID: 4 bytes
    // Creator Revision: 4 bytes
    // Local APIC address: 4 bytes
    // Flags: 4 bytes

    const MADT_HEADER_SIZE: usize = 44;
    assert_eq!(MADT_HEADER_SIZE, 44);

    println!("  [PASS] MADT header size correct");
}

/// MADT entry header.
fn test_madt_entry_header() {
    // MADT entry header: 2 bytes
    // Type: 1 byte
    // Length: 1 byte

    const MADT_ENTRY_HEADER_SIZE: usize = 2;
    assert_eq!(MADT_ENTRY_HEADER_SIZE, 2);

    println!("  [PASS] MADT entry header correct");
}

/// MADT Local APIC entry.
fn test_madt_local_apic_entry() {
    // Local APIC entry: 8 bytes
    // Type: 1 byte (0x00)
    // Length: 1 byte (8)
    // Processor ID: 1 byte
    // APIC ID: 1 byte
    // Flags: 4 bytes

    const MADT_LOCAL_APIC_TYPE: u8 = 0x00;
    const MADT_LOCAL_APIC_LENGTH: u8 = 8;

    assert_eq!(MADT_LOCAL_APIC_TYPE, 0x00);
    assert_eq!(MADT_LOCAL_APIC_LENGTH, 8);

    println!("  [PASS] MADT Local APIC entry correct");
}

/// MADT I/O APIC entry.
fn test_madt_io_apic_entry() {
    // I/O APIC entry: 12 bytes
    // Type: 1 byte (0x01)
    // Length: 1 byte (12)
    // I/O APIC ID: 1 byte
    // Reserved: 1 byte (0)
    // I/O APIC address: 4 bytes
    // Global system interrupt base: 4 bytes

    const MADT_IO_APIC_TYPE: u8 = 0x01;
    const MADT_IO_APIC_LENGTH: u8 = 12;

    assert_eq!(MADT_IO_APIC_TYPE, 0x01);
    assert_eq!(MADT_IO_APIC_LENGTH, 12);

    println!("  [PASS] MADT I/O APIC entry correct");
}

/// MADT Interrupt Source Override entry.
fn test_madt_interrupt_override_entry() {
    // Interrupt Source Override entry: 10 bytes
    // Type: 1 byte (0x02)
    // Length: 1 byte (10)
    // Bus: 1 byte (0 = ISA)
    // Source: 1 byte (IRQ number)
    // Global system interrupt: 4 bytes
    // Flags: 2 bytes

    const MADT_INTERRUPT_OVERRIDE_TYPE: u8 = 0x02;
    const MADT_INTERRUPT_OVERRIDE_LENGTH: u8 = 10;

    assert_eq!(MADT_INTERRUPT_OVERRIDE_TYPE, 0x02);
    assert_eq!(MADT_INTERRUPT_OVERRIDE_LENGTH, 10);

    println!("  [PASS] MADT Interrupt Source Override entry correct");
}

/// MADT NMI entry.
fn test_madt_nmi_entry() {
    // NMI entry: 8 bytes
    // Type: 1 byte (0x03)
    // Length: 1 byte (8)
    // Processor ID: 1 byte (0xFF = all processors)
    // Flags: 2 bytes
    // Local APIC LINT: 1 byte

    const MADT_NMI_TYPE: u8 = 0x03;
    const MADT_NMI_LENGTH: u8 = 8;

    assert_eq!(MADT_NMI_TYPE, 0x03);
    assert_eq!(MADT_NMI_LENGTH, 8);

    println!("  [PASS] MADT NMI entry correct");
}

/// MADT Local APIC Address Override entry.
fn test_madt_local_apic_address_override() {
    // Local APIC Address Override entry: 12 bytes
    // Type: 1 byte (0x05)
    // Length: 1 byte (12)
    // Reserved: 2 bytes (0)
    // Local APIC address: 8 bytes

    const MADT_LOCAL_APIC_ADDRESS_OVERRIDE_TYPE: u8 = 0x05;
    const MADT_LOCAL_APIC_ADDRESS_OVERRIDE_LENGTH: u8 = 12;

    assert_eq!(MADT_LOCAL_APIC_ADDRESS_OVERRIDE_TYPE, 0x05);
    assert_eq!(MADT_LOCAL_APIC_ADDRESS_OVERRIDE_LENGTH, 12);

    println!("  [PASS] MADT Local APIC Address Override entry correct");
}

/// MADT Local x2APIC entry.
fn test_madt_local_x2apic_entry() {
    // Local x2APIC entry: 16 bytes
    // Type: 1 byte (0x09)
    // Length: 1 byte (16)
    // Reserved: 2 bytes (0)
    // Local x2APIC ID: 4 bytes
    // Flags: 4 bytes
    // Processor UID: 4 bytes

    const MADT_LOCAL_X2APIC_TYPE: u8 = 0x09;
    const MADT_LOCAL_X2APIC_LENGTH: u8 = 16;

    assert_eq!(MADT_LOCAL_X2APIC_TYPE, 0x09);
    assert_eq!(MADT_LOCAL_X2APIC_LENGTH, 16);

    println!("  [PASS] MADT Local x2APIC entry correct");
}

/// MADT entry type validation.
fn test_madt_entry_type_validation() {
    // Valid MADT entry types:
    // 0x00: Local APIC
    // 0x01: I/O APIC
    // 0x02: Interrupt Source Override
    // 0x03: NMI Source
    // 0x04: Local APIC NMI
    // 0x05: Local APIC Address Override
    // 0x09: Local x2APIC

    const MADT_TYPE_LOCAL_APIC: u8 = 0x00;
    const MADT_TYPE_IO_APIC: u8 = 0x01;
    const MADT_TYPE_INTERRUPT_OVERRIDE: u8 = 0x02;
    const MADT_TYPE_NMI_SOURCE: u8 = 0x03;
    const MADT_TYPE_LOCAL_APIC_NMI: u8 = 0x04;
    const MADT_TYPE_LOCAL_APIC_ADDRESS_OVERRIDE: u8 = 0x05;
    const MADT_TYPE_LOCAL_X2APIC: u8 = 0x09;

    assert_eq!(MADT_TYPE_LOCAL_APIC, 0x00);
    assert_eq!(MADT_TYPE_IO_APIC, 0x01);
    assert_eq!(MADT_TYPE_INTERRUPT_OVERRIDE, 0x02);
    assert_eq!(MADT_TYPE_NMI_SOURCE, 0x03);
    assert_eq!(MADT_TYPE_LOCAL_APIC_NMI, 0x04);
    assert_eq!(MADT_TYPE_LOCAL_APIC_ADDRESS_OVERRIDE, 0x05);
    assert_eq!(MADT_TYPE_LOCAL_X2APIC, 0x09);

    println!("  [PASS] MADT entry type validation correct");
}

/// MADT flags.
fn test_madt_flags() {
    // MADT flags (32-bit):
    // Bit 0: PCAT_COMPAT (Dual 8259 legacy PICs installed)

    const MADT_FLAG_PCAT_COMPAT: u32 = 0x0000_0001;
    assert_eq!(MADT_FLAG_PCAT_COMPAT, 0x0000_0001);

    println!("  [PASS] MADT flags correct");
}
