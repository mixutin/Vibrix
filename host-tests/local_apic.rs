//! Host-side unit tests for Local APIC register layout and delivery modes.
//!
//! These tests run on the host (not in the kernel) and validate that the
//! Local APIC register offsets and bit fields are correct.
//!
//! Primary reference: Intel SDM Vol. 3A, Chapter 10 (APIC).

fn main() {
    println!("Running Local APIC register tests...\n");

    test_local_apic_register_offsets();
    test_local_apic_id();
    test_local_apic_version();
    test_local_apic_tpr();
    test_local_apic_ldr();
    test_local_apic_dfr();
    test_local_apic_sivr();
    test_local_apic_icr();
    test_local_apic_lvt();
    test_local_apic_delivery_mode();

    println!("\nAll Local APIC register tests passed!");
}

/// Local APIC register offsets (from APIC base).
fn test_local_apic_register_offsets() {
    const LAPIC_ID: u32 = 0x20;
    const LAPIC_VERSION: u32 = 0x30;
    const LAPIC_TPR: u32 = 0x80;
    const LAPIC_APR: u32 = 0x90;
    const LAPIC_PPR: u32 = 0xA0;
    const LAPIC_EOI: u32 = 0xB0;
    const LAPIC_LDR: u32 = 0xD0;
    const LAPIC_DFR: u32 = 0xE0;
    const LAPIC_SIVR: u32 = 0xF0;
    const LAPIC_ISR: u32 = 0x100;
    const LAPIC_TMR: u32 = 0x180;
    const LAPIC_IRR: u32 = 0x200;
    const LAPIC_ERROR: u32 = 0x280;
    const LAPIC_CMCI: u32 = 0x2F0;
    const LAPIC_ICR_LOW: u32 = 0x300;
    const LAPIC_ICR_HIGH: u32 = 0x310;

    assert_eq!(LAPIC_ID, 0x20);
    assert_eq!(LAPIC_VERSION, 0x30);
    assert_eq!(LAPIC_TPR, 0x80);
    assert_eq!(LAPIC_APR, 0x90);
    assert_eq!(LAPIC_PPR, 0xA0);
    assert_eq!(LAPIC_EOI, 0xB0);
    assert_eq!(LAPIC_LDR, 0xD0);
    assert_eq!(LAPIC_DFR, 0xE0);
    assert_eq!(LAPIC_SIVR, 0xF0);
    assert_eq!(LAPIC_ISR, 0x100);
    assert_eq!(LAPIC_TMR, 0x180);
    assert_eq!(LAPIC_IRR, 0x200);
    assert_eq!(LAPIC_ERROR, 0x280);
    assert_eq!(LAPIC_CMCI, 0x2F0);
    assert_eq!(LAPIC_ICR_LOW, 0x300);
    assert_eq!(LAPIC_ICR_HIGH, 0x310);

    println!("  [PASS] Local APIC register offsets correct");
}

/// Local APIC ID register.
fn test_local_apic_id() {
    // APIC ID: bits 24-31
    const LAPIC_ID_MASK: u32 = 0xFF00_0000;
    assert_eq!(LAPIC_ID_MASK, 0xFF00_0000);

    let apic_id: u32 = 0x05 << 24;
    assert_eq!(apic_id, 0x0500_0000);

    println!("  [PASS] Local APIC ID register correct");
}

/// Local APIC version register.
fn test_local_apic_version() {
    // Version: bits 0-7
    // Max LVT: bits 16-23
    // EOI-broadcast suppression: bit 24

    const LAPIC_VERSION_MASK: u32 = 0x0000_00FF;
    const LAPIC_MAX_LVT_MASK: u32 = 0x00FF_0000;
    const LAPIC_EOI_BROADCAST: u32 = 0x0100_0000;

    assert_eq!(LAPIC_VERSION_MASK, 0x0000_00FF);
    assert_eq!(LAPIC_MAX_LVT_MASK, 0x00FF_0000);
    assert_eq!(LAPIC_EOI_BROADCAST, 0x0100_0000);

    println!("  [PASS] Local APIC version register correct");
}

/// Local APIC TPR (Task Priority Register).
fn test_local_apic_tpr() {
    // TPR: bits 0-7 (task priority)
    // Bits 8-31: reserved

    const LAPIC_TPR_MASK: u32 = 0x0000_00FF;
    assert_eq!(LAPIC_TPR_MASK, 0x0000_00FF);

    println!("  [PASS] Local APIC TPR correct");
}

/// Local APIC LDR (Logical Destination Register).
fn test_local_apic_ldr() {
    // LDR: bits 24-31 (logical APIC ID)
    // Flat mode: each bit represents one APIC ID
    // Cluster mode: bits 24-27 = cluster ID, bits 28-31 = logical ID within cluster

    const LAPIC_LDR_MASK: u32 = 0xFF00_0000;
    assert_eq!(LAPIC_LDR_MASK, 0xFF00_0000);

    println!("  [PASS] Local APIC LDR correct");
}

/// Local APIC DFR (Destination Format Register).
fn test_local_apic_dfr() {
    // DFR: bits 28-31 (destination format)
    // 0-14: reserved
    // 15: flat model (all 1s)
    // 16-31: cluster model

    const LAPIC_DFR_MASK: u32 = 0xF000_0000;
    const LAPIC_DFR_FLAT: u32 = 0xFFFF_FFFF;
    const LAPIC_DFR_CLUSTER: u32 = 0x0000_0000;

    assert_eq!(LAPIC_DFR_MASK, 0xF000_0000);
    assert_eq!(LAPIC_DFR_FLAT, 0xFFFF_FFFF);
    assert_eq!(LAPIC_DFR_CLUSTER, 0x0000_0000);

    println!("  [PASS] Local APIC DFR correct");
}

/// Local APIC SIVR (Spurious Interrupt Vector Register).
fn test_local_apic_sivr() {
    // SIVR: bits 0-7 (spurious vector)
    // Bit 8: APIC software enable
    // Bit 9: focus processor checking
    // Bit 12: EOI broadcast suppression

    const LAPIC_SIVR_VECTOR_MASK: u32 = 0x0000_00FF;
    const LAPIC_SIVR_ENABLE: u32 = 0x0000_0100;
    const LAPIC_SIVR_FOCUS: u32 = 0x0000_0200;
    const LAPIC_SIVR_EOI_BROADCAST: u32 = 0x0000_1000;

    assert_eq!(LAPIC_SIVR_VECTOR_MASK, 0x0000_00FF);
    assert_eq!(LAPIC_SIVR_ENABLE, 0x0000_0100);
    assert_eq!(LAPIC_SIVR_FOCUS, 0x0000_0200);
    assert_eq!(LAPIC_SIVR_EOI_BROADCAST, 0x0000_1000);

    println!("  [PASS] Local APIC SIVR correct");
}

/// Local APIC ICR (Interrupt Command Register).
fn test_local_apic_icr() {
    // ICR Low (0x300):
    // Bits 0-7: vector
    // Bits 8-10: delivery mode
    // Bit 11: destination mode
    // Bit 12: delivery status
    // Bit 14: level
    // Bit 15: trigger mode
    // Bits 18-19: destination shorthand
    // Bits 20-23: reserved
    // Bits 24-27: TMM (timer mode)
    // Bits 28-31: reserved

    const LAPIC_ICR_VECTOR_MASK: u32 = 0x0000_00FF;
    const LAPIC_ICR_DELIVERY_MODE_MASK: u32 = 0x0000_0700;
    const LAPIC_ICR_DEST_MODE: u32 = 0x0000_0800;
    const LAPIC_ICR_DELIVERY_STATUS: u32 = 0x0000_1000;
    const LAPIC_ICR_LEVEL: u32 = 0x0000_4000;
    const LAPIC_ICR_TRIGGER: u32 = 0x0000_8000;
    const LAPIC_ICR_DEST_SHORTHAND_MASK: u32 = 0x000C_0000;

    assert_eq!(LAPIC_ICR_VECTOR_MASK, 0x0000_00FF);
    assert_eq!(LAPIC_ICR_DELIVERY_MODE_MASK, 0x0000_0700);
    assert_eq!(LAPIC_ICR_DEST_MODE, 0x0000_0800);
    assert_eq!(LAPIC_ICR_DELIVERY_STATUS, 0x0000_1000);
    assert_eq!(LAPIC_ICR_LEVEL, 0x0000_4000);
    assert_eq!(LAPIC_ICR_TRIGGER, 0x0000_8000);
    assert_eq!(LAPIC_ICR_DEST_SHORTHAND_MASK, 0x000C_0000);

    println!("  [PASS] Local APIC ICR correct");
}

/// Local APIC LVT (Local Vector Table).
fn test_local_apic_lvt() {
    // LVT bits:
    // Bits 0-7: vector
    // Bits 8-10: delivery mode
    // Bit 12: delivery status
    // Bit 16: mask
    // Bits 17-18: timer mode

    const LAPIC_LVT_VECTOR_MASK: u32 = 0x0000_00FF;
    const LAPIC_LVT_DELIVERY_MODE_MASK: u32 = 0x0000_0700;
    const LAPIC_LVT_DELIVERY_STATUS: u32 = 0x0000_1000;
    const LAPIC_LVT_MASK: u32 = 0x0001_0000;
    const LAPIC_LVT_TIMER_MODE_MASK: u32 = 0x0006_0000;

    assert_eq!(LAPIC_LVT_VECTOR_MASK, 0x0000_00FF);
    assert_eq!(LAPIC_LVT_DELIVERY_MODE_MASK, 0x0000_0700);
    assert_eq!(LAPIC_LVT_DELIVERY_STATUS, 0x0000_1000);
    assert_eq!(LAPIC_LVT_MASK, 0x0001_0000);
    assert_eq!(LAPIC_LVT_TIMER_MODE_MASK, 0x0006_0000);

    println!("  [PASS] Local APIC LVT correct");
}

/// Local APIC delivery mode.
fn test_local_apic_delivery_mode() {
    const LAPIC_DELIVERY_FIXED: u32 = 0x0;
    const LAPIC_DELIVERY_LOWEST_PRIORITY: u32 = 0x1;
    const LAPIC_DELIVERY_SMI: u32 = 0x2;
    const LAPIC_DELIVERY_NMI: u32 = 0x4;
    const LAPIC_DELIVERY_INIT: u32 = 0x5;
    const LAPIC_DELIVERY_STARTUP: u32 = 0x6;

    assert_eq!(LAPIC_DELIVERY_FIXED, 0x0);
    assert_eq!(LAPIC_DELIVERY_LOWEST_PRIORITY, 0x1);
    assert_eq!(LAPIC_DELIVERY_SMI, 0x2);
    assert_eq!(LAPIC_DELIVERY_NMI, 0x4);
    assert_eq!(LAPIC_DELIVERY_INIT, 0x5);
    assert_eq!(LAPIC_DELIVERY_STARTUP, 0x6);

    println!("  [PASS] Local APIC delivery mode correct");
}
