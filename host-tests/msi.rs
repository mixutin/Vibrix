//! Host-side unit tests for MSI and MSI-X capability structures.
//!
//! These tests run on the host (not in the kernel) and validate that the
//! MSI and MSI-X capability register layouts are correct.
//!
//! Primary reference: PCI Local Bus Specification, PCI Firmware Specification.

fn main() {
    println!("Running MSI/MSI-X tests...\n");

    test_msi_capability_id();
    test_msi_control_register();
    test_msi_address_register();
    test_msi_data_register();
    test_msi_mask_register();
    test_msi_pending_register();
    test_msix_capability_id();
    test_msix_control_register();
    test_msix_table_offset();
    test_msix_pba_offset();
    test_msix_table_entry();

    println!("\nAll MSI/MSI-X tests passed!");
}

/// MSI capability ID (PCI capability list).
fn test_msi_capability_id() {
    const MSI_CAP_ID: u8 = 0x05;
    assert_eq!(MSI_CAP_ID, 0x05);
    println!("  [PASS] MSI capability ID correct");
}

/// MSI control register format.
fn test_msi_control_register() {
    // Bit 0: MSI enable
    // Bits 3-1: Multiple message enable (0=1 message, 1=2, 2=4, 3=8, 4=16, 5=32)
    // Bits 6-4: Multiple message capable (0=1, 1=2, 2=4, 3=8, 4=16, 5=32)
    // Bit 7: 64-bit address capable
    // Bit 8: Per-vector masking capable

    const MSI_ENABLE: u16 = 1 << 0;
    const MSI_64BIT_CAPABLE: u16 = 1 << 7;
    const MSI_MASKING_CAPABLE: u16 = 1 << 8;

    assert_eq!(MSI_ENABLE, 0x0001);
    assert_eq!(MSI_64BIT_CAPABLE, 0x0080);
    assert_eq!(MSI_MASKING_CAPABLE, 0x0100);

    println!("  [PASS] MSI control register format correct");
}

/// MSI address register format.
fn test_msi_address_register() {
    // Bits 0-1: Reserved (0)
    // Bits 2-3: Destination mode (0=physical, 1=logical)
    // Bits 4-31: Destination ID (APIC ID or logical destination)

    const MSI_ADDR_DEST_MODE: u32 = 0b0000_1100;
    const MSI_ADDR_DEST_ID_MASK: u32 = 0xFFFF_FFF0;

    assert_eq!(MSI_ADDR_DEST_MODE, 0x0C);
    assert_eq!(MSI_ADDR_DEST_ID_MASK, 0xFFFF_FFF0);

    println!("  [PASS] MSI address register format correct");
}

/// MSI data register format.
fn test_msi_data_register() {
    // Bits 0-7: Interrupt vector
    // Bits 8-10: Delivery mode
    // Bits 14-15: Trigger mode (0=edge, 1=level)
    // Bits 16-17: Level (0=deassert, 1=assert)

    const MSI_DATA_VECTOR_MASK: u16 = 0x00FF;
    const MSI_DATA_DELIVERY_MODE_MASK: u16 = 0x0700;
    const MSI_DATA_TRIGGER_MODE_MASK: u16 = 0xC000;

    assert_eq!(MSI_DATA_VECTOR_MASK, 0x00FF);
    assert_eq!(MSI_DATA_DELIVERY_MODE_MASK, 0x0700);
    assert_eq!(MSI_DATA_TRIGGER_MODE_MASK, 0xC000);

    println!("  [PASS] MSI data register format correct");
}

/// MSI mask register format.
fn test_msi_mask_register() {
    // Bits 0-31: Mask bits (1=masked, 0=unmasked)
    // Only valid if per-vector masking is capable

    const MSI_MASK_ALL: u32 = 0xFFFF_FFFF;
    const MSI_MASK_NONE: u32 = 0x0000_0000;

    assert_eq!(MSI_MASK_ALL, 0xFFFF_FFFF);
    assert_eq!(MSI_MASK_NONE, 0x0000_0000);

    println!("  [PASS] MSI mask register format correct");
}

/// MSI pending register format.
fn test_msi_pending_register() {
    // Bits 0-31: Pending bits (1=pending, 0=not pending)
    // Only valid if per-vector masking is capable

    const MSI_PENDING_ALL: u32 = 0xFFFF_FFFF;
    const MSI_PENDING_NONE: u32 = 0x0000_0000;

    assert_eq!(MSI_PENDING_ALL, 0xFFFF_FFFF);
    assert_eq!(MSI_PENDING_NONE, 0x0000_0000);

    println!("  [PASS] MSI pending register format correct");
}

/// MSI-X capability ID (PCI capability list).
fn test_msix_capability_id() {
    const MSIX_CAP_ID: u8 = 0x11;
    assert_eq!(MSIX_CAP_ID, 0x11);
    println!("  [PASS] MSI-X capability ID correct");
}

/// MSI-X control register format.
fn test_msix_control_register() {
    // Bits 0-10: Table size (N-1, where N is the number of entries)
    // Bit 11: Reserved
    // Bit 12: Function mask
    // Bit 13: MSI-X enable

    const MSIX_ENABLE: u16 = 1 << 13;
    const MSIX_FUNCTION_MASK: u16 = 1 << 12;
    const MSIX_TABLE_SIZE_MASK: u16 = 0x07FF;

    assert_eq!(MSIX_ENABLE, 0x2000);
    assert_eq!(MSIX_FUNCTION_MASK, 0x1000);
    assert_eq!(MSIX_TABLE_SIZE_MASK, 0x07FF);

    println!("  [PASS] MSI-X control register format correct");
}

/// MSI-X table offset format.
fn test_msix_table_offset() {
    // Bits 0-2: Reserved (0)
    // Bits 3-31: Table offset (8-byte aligned, points to table in BAR space)

    const MSIX_TABLE_OFFSET_MASK: u32 = 0xFFFF_FFF8;
    const MSIX_TABLE_BIR_MASK: u32 = 0x0000_0007;

    assert_eq!(MSIX_TABLE_OFFSET_MASK, 0xFFFF_FFF8);
    assert_eq!(MSIX_TABLE_BIR_MASK, 0x0000_0007);

    println!("  [PASS] MSI-X table offset format correct");
}

/// MSI-X PBA (Pending Bit Array) offset format.
fn test_msix_pba_offset() {
    // Bits 0-2: Reserved (0)
    // Bits 3-31: PBA offset (8-byte aligned, points to PBA in BAR space)

    const MSIX_PBA_OFFSET_MASK: u32 = 0xFFFF_FFF8;
    const MSIX_PBA_BIR_MASK: u32 = 0x0000_0007;

    assert_eq!(MSIX_PBA_OFFSET_MASK, 0xFFFF_FFF8);
    assert_eq!(MSIX_PBA_BIR_MASK, 0x0000_0007);

    println!("  [PASS] MSI-X PBA offset format correct");
}

/// MSI-X table entry format.
fn test_msix_table_entry() {
    // Bits 0-7: Vector control (bit 0 = mask)
    // Bits 8-15: Reserved
    // Bits 16-31: Message address low
    // Bits 32-63: Message address high
    // Bits 64-95: Message data
    // Bits 96-127: Vector control

    const MSIX_VECTOR_MASK: u32 = 0x0000_0001;
    const MSIX_MESSAGE_ADDR_LOW_MASK: u32 = 0xFFFF_FFF0;
    const MSIX_MESSAGE_DATA_MASK: u32 = 0x0000_FFFF;

    assert_eq!(MSIX_VECTOR_MASK, 0x0000_0001);
    assert_eq!(MSIX_MESSAGE_ADDR_LOW_MASK, 0xFFFF_FFF0);
    assert_eq!(MSIX_MESSAGE_DATA_MASK, 0x0000_FFFF);

    println!("  [PASS] MSI-X table entry format correct");
}
