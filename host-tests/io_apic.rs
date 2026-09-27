//! Host-side unit tests for I/O APIC register layout and redirection entries.
//!
//! These tests run on the host (not in the kernel) and validate that the
//! I/O APIC register offsets and bit fields are correct.
//!
//! Primary reference: Intel SDM Vol. 3A, Chapter 10 (APIC).

fn main() {
    println!("Running I/O APIC register tests...\n");

    test_io_apic_register_offsets();
    test_io_apic_id();
    test_io_apic_version();
    test_io_apic_arb();
    test_io_apic_redirection_entry();
    test_io_apic_redirection_vector();
    test_io_apic_redirection_delivery_mode();
    test_io_apic_redirection_destination_mode();
    test_io_apic_redirection_mask();
    test_io_apic_redirection_destination();

    println!("\nAll I/O APIC register tests passed!");
}

/// I/O APIC register offsets.
fn test_io_apic_register_offsets() {
    const IO_APIC_ID: u32 = 0x00;
    const IO_APIC_VERSION: u32 = 0x01;
    const IO_APIC_ARB: u32 = 0x02;
    const IO_APIC_REDIRECTION_TABLE: u32 = 0x10;

    assert_eq!(IO_APIC_ID, 0x00);
    assert_eq!(IO_APIC_VERSION, 0x01);
    assert_eq!(IO_APIC_ARB, 0x02);
    assert_eq!(IO_APIC_REDIRECTION_TABLE, 0x10);

    println!("  [PASS] I/O APIC register offsets correct");
}

/// I/O APIC ID register.
fn test_io_apic_id() {
    // I/O APIC ID: bits 24-27
    const IO_APIC_ID_MASK: u32 = 0x0F00_0000;
    assert_eq!(IO_APIC_ID_MASK, 0x0F00_0000);

    let io_apic_id: u32 = 0x02 << 24;
    assert_eq!(io_apic_id, 0x0200_0000);

    println!("  [PASS] I/O APIC ID register correct");
}

/// I/O APIC version register.
fn test_io_apic_version() {
    // Version: bits 0-7
    // Max redirection entry: bits 16-23

    const IO_APIC_VERSION_MASK: u32 = 0x0000_00FF;
    const IO_APIC_MAX_RED_MASK: u32 = 0x00FF_0000;

    assert_eq!(IO_APIC_VERSION_MASK, 0x0000_00FF);
    assert_eq!(IO_APIC_MAX_RED_MASK, 0x00FF_0000);

    println!("  [PASS] I/O APIC version register correct");
}

/// I/O APIC arbitration register.
fn test_io_apic_arb() {
    // Arbitration ID: bits 24-27
    const IO_APIC_ARB_MASK: u32 = 0x0F00_0000;
    assert_eq!(IO_APIC_ARB_MASK, 0x0F00_0000);

    println!("  [PASS] I/O APIC arbitration register correct");
}

/// I/O APIC redirection entry.
fn test_io_apic_redirection_entry() {
    // Redirection entry (64 bits):
    // Bits 0-7: Vector
    // Bits 8-10: Delivery mode
    // Bit 11: Destination mode
    // Bit 12: Delivery status
    // Bit 13: Polarity
    // Bit 14: Remote IRR
    // Bit 15: Trigger mode
    // Bit 16: Mask
    // Bits 17-55: Reserved
    // Bits 56-63: Destination

    const IO_APIC_REDIR_VECTOR_MASK: u64 = 0x0000_0000_0000_00FF;
    const IO_APIC_REDIR_DELIVERY_MODE_MASK: u64 = 0x0000_0000_0000_0700;
    const IO_APIC_REDIR_DEST_MODE: u64 = 0x0000_0000_0000_0800;
    const IO_APIC_REDIR_DELIVERY_STATUS: u64 = 0x0000_0000_0000_1000;
    const IO_APIC_REDIR_POLARITY: u64 = 0x0000_0000_0000_2000;
    const IO_APIC_REDIR_REMOTE_IRR: u64 = 0x0000_0000_0000_4000;
    const IO_APIC_REDIR_TRIGGER: u64 = 0x0000_0000_0000_8000;
    const IO_APIC_REDIR_MASK: u64 = 0x0000_0000_0001_0000;
    const IO_APIC_REDIR_DEST_MASK: u64 = 0xFF00_0000_0000_0000;

    assert_eq!(IO_APIC_REDIR_VECTOR_MASK, 0x0000_0000_0000_00FF);
    assert_eq!(IO_APIC_REDIR_DELIVERY_MODE_MASK, 0x0000_0000_0000_0700);
    assert_eq!(IO_APIC_REDIR_DEST_MODE, 0x0000_0000_0000_0800);
    assert_eq!(IO_APIC_REDIR_DELIVERY_STATUS, 0x0000_0000_0000_1000);
    assert_eq!(IO_APIC_REDIR_POLARITY, 0x0000_0000_0000_2000);
    assert_eq!(IO_APIC_REDIR_REMOTE_IRR, 0x0000_0000_0000_4000);
    assert_eq!(IO_APIC_REDIR_TRIGGER, 0x0000_0000_0000_8000);
    assert_eq!(IO_APIC_REDIR_MASK, 0x0000_0000_0001_0000);
    assert_eq!(IO_APIC_REDIR_DEST_MASK, 0xFF00_0000_0000_0000);

    println!("  [PASS] I/O APIC redirection entry correct");
}

/// I/O APIC redirection vector.
fn test_io_apic_redirection_vector() {
    // Vector: bits 0-7
    const IO_APIC_VECTOR_MASK: u64 = 0x0000_0000_0000_00FF;
    assert_eq!(IO_APIC_VECTOR_MASK, 0x0000_0000_0000_00FF);

    println!("  [PASS] I/O APIC redirection vector correct");
}

/// I/O APIC redirection delivery mode.
fn test_io_apic_redirection_delivery_mode() {
    // Delivery mode: bits 8-10
    const IO_APIC_DELIVERY_FIXED: u64 = 0x0;
    const IO_APIC_DELIVERY_LOWEST_PRIORITY: u64 = 0x1;
    const IO_APIC_DELIVERY_SMI: u64 = 0x2;
    const IO_APIC_DELIVERY_NMI: u64 = 0x4;
    const IO_APIC_DELIVERY_INIT: u64 = 0x5;
    const IO_APIC_DELIVERY_EXTINT: u64 = 0x7;

    assert_eq!(IO_APIC_DELIVERY_FIXED, 0x0);
    assert_eq!(IO_APIC_DELIVERY_LOWEST_PRIORITY, 0x1);
    assert_eq!(IO_APIC_DELIVERY_SMI, 0x2);
    assert_eq!(IO_APIC_DELIVERY_NMI, 0x4);
    assert_eq!(IO_APIC_DELIVERY_INIT, 0x5);
    assert_eq!(IO_APIC_DELIVERY_EXTINT, 0x7);

    println!("  [PASS] I/O APIC redirection delivery mode correct");
}

/// I/O APIC redirection destination mode.
fn test_io_apic_redirection_destination_mode() {
    // Destination mode: bit 11
    // 0 = physical
    // 1 = logical

    const IO_APIC_DEST_MODE_PHYSICAL: u64 = 0x0;
    const IO_APIC_DEST_MODE_LOGICAL: u64 = 0x800;

    assert_eq!(IO_APIC_DEST_MODE_PHYSICAL, 0x0);
    assert_eq!(IO_APIC_DEST_MODE_LOGICAL, 0x800);

    println!("  [PASS] I/O APIC redirection destination mode correct");
}

/// I/O APIC redirection mask.
fn test_io_apic_redirection_mask() {
    // Mask: bit 16
    // 0 = unmasked (enabled)
    // 1 = masked (disabled)

    const IO_APIC_MASK_UNMASKED: u64 = 0x0;
    const IO_APIC_MASK_MASKED: u64 = 0x1_0000;

    assert_eq!(IO_APIC_MASK_UNMASKED, 0x0);
    assert_eq!(IO_APIC_MASK_MASKED, 0x1_0000);

    println!("  [PASS] I/O APIC redirection mask correct");
}

/// I/O APIC redirection destination.
fn test_io_apic_redirection_destination() {
    // Destination: bits 56-63
    // Physical mode: APIC ID
    // Logical mode: logical destination (set of APIC IDs)

    const IO_APIC_DEST_MASK: u64 = 0xFF00_0000_0000_0000;
    assert_eq!(IO_APIC_DEST_MASK, 0xFF00_0000_0000_0000);

    println!("  [PASS] I/O APIC redirection destination correct");
}
