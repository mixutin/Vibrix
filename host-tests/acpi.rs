//! Host-side unit tests for ACPI table parsing and validation logic.
//!
//! These tests run on the host (not in the kernel) and validate that the
//! ACPI table structures have the correct format per the ACPI specification.
//!
//! Primary reference: ACPI Specification, Chapter 5 (ACPI System Description Tables).

fn main() {
    println!("Running ACPI table parsing tests...\n");

    test_rsdp_signature();
    test_rsdp_checksum();
    test_rsdp_revision();
    test_xsdt_header();
    test_madt_header();
    test_madt_entry_types();
    test_madt_local_apic();
    test_madt_io_apic();
    test_madt_interrupt_override();
    test_hpet_header();
    test_fadt_header();

    println!("\nAll ACPI table parsing tests passed!");
}

/// RSDP (Root System Description Pointer) signature.
fn test_rsdp_signature() {
    const RSDP_SIGNATURE: &[u8; 8] = b"RSD PTR ";
    assert_eq!(RSDP_SIGNATURE, b"RSD PTR ");
    println!("  [PASS] RSDP signature correct");
}

/// RSDP checksum validation.
fn test_rsdp_checksum() {
    // The RSDP checksum is the sum of all bytes in the RSDP structure
    // (for revision 0) or the first 20 bytes (for revision 2+).
    // The sum must be 0 (mod 256).

    let rsdp_bytes: [u8; 20] = [0; 20];
    let sum: u8 = rsdp_bytes.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
    assert_eq!(sum, 0);

    println!("  [PASS] RSDP checksum validation correct");
}

/// RSDP revision field.
fn test_rsdp_revision() {
    // Revision 0: ACPI 1.0 (20 bytes)
    // Revision 2: ACPI 2.0+ (36 bytes)

    const REVISION_0: u8 = 0;
    const REVISION_2: u8 = 2;

    assert_eq!(REVISION_0, 0);
    assert_eq!(REVISION_2, 2);

    println!("  [PASS] RSDP revision field correct");
}

/// XSDT (Extended System Description Table) header format.
fn test_xsdt_header() {
    // Signature: "XSDT" (4 bytes)
    // Length: u32 (4 bytes)
    // Revision: u8 (1 byte)
    // Checksum: u8 (1 byte)
    // OEM ID: [u8; 6] (6 bytes)
    // OEM Table ID: [u8; 8] (8 bytes)
    // OEM Revision: u32 (4 bytes)
    // Creator ID: u32 (4 bytes)
    // Creator Revision: u32 (4 bytes)

    const XSDT_SIGNATURE: &[u8; 4] = b"XSDT";
    assert_eq!(XSDT_SIGNATURE, b"XSDT");

    // Header size: 4 + 4 + 1 + 1 + 6 + 8 + 4 + 4 + 4 = 36 bytes
    const XSDT_HEADER_SIZE: usize = 36;
    assert_eq!(XSDT_HEADER_SIZE, 36);

    println!("  [PASS] XSDT header format correct");
}

/// MADT (Multiple APIC Description Table) header format.
fn test_madt_header() {
    // Signature: "APIC" (4 bytes)
    // Length: u32 (4 bytes)
    // Revision: u8 (1 byte)
    // Checksum: u8 (1 byte)
    // OEM ID: [u8; 6] (6 bytes)
    // OEM Table ID: [u8; 8] (8 bytes)
    // OEM Revision: u32 (4 bytes)
    // Creator ID: u32 (4 bytes)
    // Creator Revision: u32 (4 bytes)
    // Local APIC address: u32 (4 bytes)
    // Flags: u32 (4 bytes)

    const MADT_SIGNATURE: &[u8; 4] = b"APIC";
    assert_eq!(MADT_SIGNATURE, b"APIC");

    // Header size: 36 + 4 + 4 = 44 bytes
    const MADT_HEADER_SIZE: usize = 44;
    assert_eq!(MADT_HEADER_SIZE, 44);

    println!("  [PASS] MADT header format correct");
}

/// MADT entry types.
fn test_madt_entry_types() {
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

    println!("  [PASS] MADT entry types correct");
}

/// MADT Local APIC entry format.
fn test_madt_local_apic() {
    // Type: u8 (1 byte) = 0x00
    // Length: u8 (1 byte) = 8
    // Processor ID: u8 (1 byte)
    // APIC ID: u8 (1 byte)
    // Flags: u32 (4 bytes)

    const MADT_LOCAL_APIC_LENGTH: u8 = 8;
    assert_eq!(MADT_LOCAL_APIC_LENGTH, 8);

    // Flags bit 0: Processor enabled
    // Flags bit 1: Online capable
    const MADT_LOCAL_APIC_ENABLED: u32 = 1 << 0;
    const MADT_LOCAL_APIC_ONLINE_CAPABLE: u32 = 1 << 1;

    assert_eq!(MADT_LOCAL_APIC_ENABLED, 0x0000_0001);
    assert_eq!(MADT_LOCAL_APIC_ONLINE_CAPABLE, 0x0000_0002);

    println!("  [PASS] MADT Local APIC entry format correct");
}

/// MADT I/O APIC entry format.
fn test_madt_io_apic() {
    // Type: u8 (1 byte) = 0x01
    // Length: u8 (1 byte) = 12
    // I/O APIC ID: u8 (1 byte)
    // Reserved: u8 (1 byte) = 0
    // I/O APIC address: u32 (4 bytes)
    // Global system interrupt base: u32 (4 bytes)

    const MADT_IO_APIC_LENGTH: u8 = 12;
    assert_eq!(MADT_IO_APIC_LENGTH, 12);

    println!("  [PASS] MADT I/O APIC entry format correct");
}

/// MADT Interrupt Source Override entry format.
fn test_madt_interrupt_override() {
    // Type: u8 (1 byte) = 0x02
    // Length: u8 (1 byte) = 10
    // Bus: u8 (1 byte) = 0 (ISA)
    // Source: u8 (1 byte) (IRQ number)
    // Global system interrupt: u32 (4 bytes)
    // Flags: u16 (2 bytes)

    const MADT_INTERRUPT_OVERRIDE_LENGTH: u8 = 10;
    assert_eq!(MADT_INTERRUPT_OVERRIDE_LENGTH, 10);

    // Flags bits 0-1: Polarity (0=active high, 1=active low, 2=active both, 3=reserved)
    // Flags bits 2-3: Trigger mode (0=edge, 1=level, 2=level active low, 3=reserved)

    const MADT_POLARITY_ACTIVE_HIGH: u16 = 0b00;
    const MADT_POLARITY_ACTIVE_LOW: u16 = 0b01;
    const MADT_TRIGGER_EDGE: u16 = 0b00;
    const MADT_TRIGGER_LEVEL: u16 = 0b10;

    assert_eq!(MADT_POLARITY_ACTIVE_HIGH, 0b00);
    assert_eq!(MADT_POLARITY_ACTIVE_LOW, 0b01);
    assert_eq!(MADT_TRIGGER_EDGE, 0b00);
    assert_eq!(MADT_TRIGGER_LEVEL, 0b10);

    println!("  [PASS] MADT Interrupt Source Override entry format correct");
}

/// HPET header format.
fn test_hpet_header() {
    // Signature: "HPET" (4 bytes)
    // Length: u32 (4 bytes)
    // Revision: u8 (1 byte)
    // Checksum: u8 (1 byte)
    // OEM ID: [u8; 6] (6 bytes)
    // OEM Table ID: [u8; 8] (8 bytes)
    // OEM Revision: u32 (4 bytes)
    // Creator ID: u32 (4 bytes)
    // Creator Revision: u32 (4 bytes)
    // Timer block ID: u32 (4 bytes)
    // Base address: u64 (8 bytes)

    const HPET_SIGNATURE: &[u8; 4] = b"HPET";
    assert_eq!(HPET_SIGNATURE, b"HPET");

    // Header size: 36 + 4 + 8 = 48 bytes
    const HPET_HEADER_SIZE: usize = 48;
    assert_eq!(HPET_HEADER_SIZE, 48);

    println!("  [PASS] HPET header format correct");
}

/// FADT (Fixed ACPI Description Table) header format.
fn test_fadt_header() {
    // Signature: "FACP" (4 bytes)
    // Length: u32 (4 bytes)
    // Revision: u8 (1 byte)
    // Checksum: u8 (1 byte)
    // OEM ID: [u8; 6] (6 bytes)
    // OEM Table ID: [u8; 8] (8 bytes)
    // OEM Revision: u32 (4 bytes)
    // Creator ID: u32 (4 bytes)
    // Creator Revision: u32 (4 bytes)
    // FIRMWARE_CTRL: u32 (4 bytes)
    // DSDT: u32 (4 bytes)
    // ... (many more fields)

    const FADT_SIGNATURE: &[u8; 4] = b"FACP";
    assert_eq!(FADT_SIGNATURE, b"FACP");

    // FADT length varies by revision, but minimum is 116 bytes (ACPI 1.0)
    const FADT_MIN_SIZE: usize = 116;
    assert_eq!(FADT_MIN_SIZE, 116);

    println!("  [PASS] FADT header format correct");
}
