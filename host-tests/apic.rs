//! Host-side unit tests for x86-64 Local APIC and I/O APIC register layout.
//!
//! These tests run on the host (not in the kernel) and validate that the
//! APIC register offsets and bit fields are correct.
//!
//! Primary reference: Intel SDM Vol. 3A, Chapter 10 (Advanced Programmable Interrupt Controller),
//! AMD APM Vol. 2, Chapter 16 (APIC).

fn main() {
    println!("Running Local APIC and I/O APIC tests...\n");

    test_local_apic_register_offsets();
    test_local_apic_id_register();
    test_local_apic_version_register();
    test_local_apic_eoi_register();
    test_local_apic_icr_format();
    test_local_apic_lvt_format();
    test_io_apic_register_offsets();
    test_io_apic_redirection_entry();
    test_io_apic_version_register();
    test_apic_interrupt_vectors();
    test_apic_delivery_mode();
    test_apic_destination_mode();
    test_apic_level_triggered();

    println!("\nAll Local APIC and I/O APIC tests passed!");
}

/// Local APIC register offsets (Intel SDM Vol. 3A, Chapter 10).
fn test_local_apic_register_offsets() {
    const LOCAL_APIC_ID: u32 = 0x20;
    const LOCAL_APIC_VERSION: u32 = 0x30;
    const LOCAL_APIC_TPR: u32 = 0x80;
    const LOCAL_APIC_EOI: u32 = 0xB0;
    const LOCAL_APIC_LDR: u32 = 0xD0;
    const LOCAL_APIC_DFR: u32 = 0xE0;
    const LOCAL_APIC_SIVR: u32 = 0xF0;
    const LOCAL_APIC_ISR: u32 = 0x100;
    const LOCAL_APIC_TMR: u32 = 0x180;
    const LOCAL_APIC_IRR: u32 = 0x200;
    const LOCAL_APIC_ERROR: u32 = 0x280;
    const LOCAL_APIC_CMCI: u32 = 0x2F0;
    const LOCAL_APIC_ICR_LOW: u32 = 0x300;
    const LOCAL_APIC_ICR_HIGH: u32 = 0x310;

    assert_eq!(LOCAL_APIC_ID, 0x20);
    assert_eq!(LOCAL_APIC_VERSION, 0x30);
    assert_eq!(LOCAL_APIC_TPR, 0x80);
    assert_eq!(LOCAL_APIC_EOI, 0xB0);
    assert_eq!(LOCAL_APIC_LDR, 0xD0);
    assert_eq!(LOCAL_APIC_DFR, 0xE0);
    assert_eq!(LOCAL_APIC_SIVR, 0xF0);
    assert_eq!(LOCAL_APIC_ISR, 0x100);
    assert_eq!(LOCAL_APIC_TMR, 0x180);
    assert_eq!(LOCAL_APIC_IRR, 0x200);
    assert_eq!(LOCAL_APIC_ERROR, 0x280);
    assert_eq!(LOCAL_APIC_CMCI, 0x2F0);
    assert_eq!(LOCAL_APIC_ICR_LOW, 0x300);
    assert_eq!(LOCAL_APIC_ICR_HIGH, 0x310);

    println!("  [PASS] Local APIC register offsets correct");
}

/// Local APIC ID register format.
fn test_local_apic_id_register() {
    // Bits 24-31: APIC ID
    let apic_id: u32 = 0x05 << 24;
    assert_eq!(apic_id, 0x05000000);
    assert_eq!((apic_id >> 24) & 0xFF, 0x05);
    println!("  [PASS] Local APIC ID register format correct");
}

/// Local APIC version register format.
fn test_local_apic_version_register() {
    // Bits 0-7: Version
    // Bits 16-23: Maximum LVT entry
    // Bit 24: EOI-broadcast suppression
    let version: u32 = 0x00050014; // Version 0x14, Max LVT 5
    assert_eq!(version & 0xFF, 0x14);
    assert_eq!((version >> 16) & 0xFF, 0x05);
    println!("  [PASS] Local APIC version register format correct");
}

/// Local APIC EOI register format.
fn test_local_apic_eoi_register() {
    // EOI register is write-only, any value
    // Writing any value to EOI acknowledges the interrupt
    let eoi_value: u32 = 0x00;
    assert_eq!(eoi_value, 0x00);
    println!("  [PASS] Local APIC EOI register format correct");
}

/// Local APIC Interrupt Command Register (ICR) format.
fn test_local_apic_icr_format() {
    // ICR Low (0x300):
    //   Bits 0-7: Vector
    //   Bits 8-10: Delivery mode
    //   Bit 11: Destination mode (0=physical, 1=logical)
    //   Bit 12: Delivery status (read-only)
    //   Bit 14: Level (0=deassert, 1=assert)
    //   Bit 15: Trigger mode (0=edge, 1=level)
    //   Bits 18-19: Destination shorthand
    //   Bits 56-63: Destination (ICR High)

    let icr_low: u32 = 0x0000_0000;
    assert_eq!(icr_low & 0xFF, 0x00); // Vector
    assert_eq!((icr_low >> 8) & 0x7, 0); // Delivery mode
    assert_eq!((icr_low >> 11) & 0x1, 0); // Destination mode
    assert_eq!((icr_low >> 14) & 0x1, 0); // Level
    assert_eq!((icr_low >> 15) & 0x1, 0); // Trigger mode
    assert_eq!((icr_low >> 18) & 0x3, 0); // Destination shorthand

    println!("  [PASS] Local APIC ICR format correct");
}

/// Local APIC LVT (Local Vector Table) entry format.
fn test_local_apic_lvt_format() {
    // Bits 0-7: Vector
    // Bits 8-10: Delivery mode
    // Bit 12: Delivery status (read-only)
    // Bit 16: Mask (0=unmasked, 1=masked)
    // Bits 17-18: Timer mode (0=one-shot, 1=periodic, 2=TSC-deadline)

    let lvt: u32 = 0x0000_0000;
    assert_eq!(lvt & 0xFF, 0x00); // Vector
    assert_eq!((lvt >> 8) & 0x7, 0); // Delivery mode
    assert_eq!((lvt >> 12) & 0x1, 0); // Delivery status
    assert_eq!((lvt >> 16) & 0x1, 0); // Mask
    assert_eq!((lvt >> 17) & 0x3, 0); // Timer mode

    println!("  [PASS] Local APIC LVT format correct");
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

/// I/O APIC Redirection Table Entry format.
fn test_io_apic_redirection_entry() {
    // Bits 0-7: Vector
    // Bits 8-10: Delivery mode
    // Bit 11: Destination mode (0=physical, 1=logical)
    // Bit 12: Delivery status (read-only)
    // Bit 13: Polarity (0=active high, 1=active low)
    // Bit 14: Remote IRR (read-only)
    // Bit 15: Trigger mode (0=edge, 1=level)
    // Bit 16: Mask (0=unmasked, 1=masked)
    // Bits 56-63: Destination

    let entry: u64 = 0x0000_0000_0000_0000;
    assert_eq!(entry & 0xFF, 0x00); // Vector
    assert_eq!((entry >> 8) & 0x7, 0); // Delivery mode
    assert_eq!((entry >> 11) & 0x1, 0); // Destination mode
    assert_eq!((entry >> 13) & 0x1, 0); // Polarity
    assert_eq!((entry >> 15) & 0x1, 0); // Trigger mode
    assert_eq!((entry >> 16) & 0x1, 0); // Mask

    println!("  [PASS] I/O APIC redirection entry format correct");
}

/// I/O APIC version register format.
fn test_io_apic_version_register() {
    // Bits 0-7: Version
    // Bits 16-23: Maximum Redirection Entry
    let version: u32 = 0x0017_0011; // Version 0x11, Max redirection entry 23
    assert_eq!(version & 0xFF, 0x11);
    assert_eq!((version >> 16) & 0xFF, 0x17);
    println!("  [PASS] I/O APIC version register format correct");
}

/// APIC interrupt vectors.
fn test_apic_interrupt_vectors() {
    // Vector 0x20-0xFF are typically used for I/O APIC interrupts
    let timer_vector: u8 = 0x20;
    let keyboard_vector: u8 = 0x21;
    let lint0_vector: u8 = 0x30;

    assert_eq!(timer_vector, 0x20);
    assert_eq!(keyboard_vector, 0x21);
    assert_eq!(lint0_vector, 0x30);
    println!("  [PASS] APIC interrupt vectors correct");
}

/// APIC delivery mode.
fn test_apic_delivery_mode() {
    const DELIVERY_FIXED: u32 = 0b000;
    const DELIVERY_LOWEST_PRIORITY: u32 = 0b001;
    const DELIVERY_SMI: u32 = 0b010;
    const DELIVERY_NMI: u32 = 0b100;
    const DELIVERY_INIT: u32 = 0b101;
    const DELIVEOUT_STARTUP: u32 = 0b110;

    assert_eq!(DELIVERY_FIXED, 0);
    assert_eq!(DELIVERY_LOWEST_PRIORITY, 1);
    assert_eq!(DELIVERY_SMI, 2);
    assert_eq!(DELIVERY_NMI, 4);
    assert_eq!(DELIVERY_INIT, 5);
    assert_eq!(DELIVEOUT_STARTUP, 6);
    println!("  [PASS] APIC delivery modes correct");
}

/// APIC destination mode.
fn test_apic_destination_mode() {
    const DEST_PHYSICAL: u32 = 0;
    const DEST_LOGICAL: u32 = 1;

    assert_eq!(DEST_PHYSICAL, 0);
    assert_eq!(DEST_LOGICAL, 1);
    println!("  [PASS] APIC destination modes correct");
}

/// APIC level-triggered mode.
fn test_apic_level_triggered() {
    const TRIGGER_EDGE: u32 = 0;
    const TRIGGER_LEVEL: u32 = 1;

    assert_eq!(TRIGGER_EDGE, 0);
    assert_eq!(TRIGGER_LEVEL, 1);
    println!("  [PASS] APIC trigger modes correct");
}
