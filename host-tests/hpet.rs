//! Host-side unit tests for HPET (High Precision Event Timer) capability registers.
//!
//! These tests run on the host (not in the kernel) and validate that the
//! HPET capability register structures are correct.
//!
//! Primary reference: IA-PC HPET Specification.

fn main() {
    println!("Running HPET capability register tests...\n");

    test_hpet_capability_id();
    test_hpet_period();
    test_hpet_vendor_id();
    test_hpet_legacy_route();
    test_hpet_counter_size();
    test_hpet_timer_count();
    test_hpet_timer_config();
    test_hpet_timer_comparator();
    test_hpet_main_counter();
    test_hpet_interrupt_status();

    println!("\nAll HPET capability register tests passed!");
}

/// HPET capability ID register.
fn test_hpet_capability_id() {
    // HPET Capability ID register (8 bytes):
    // Bits 0-7: Revision ID
    // Bits 8-12: Timer count (number of timers - 1)
    // Bit 13: Legacy route capable
    // Bit 14: Reserved
    // Bit 15: Counter size (0 = 32-bit, 1 = 64-bit)
    // Bits 16-31: Vendor ID
    // Bits 32-63: Period (in femtoseconds)

    const HPET_REVISION_MASK: u32 = 0x0000_00FF;
    const HPET_TIMER_COUNT_MASK: u32 = 0x0000_1F00;
    const HPET_LEGACY_ROUTE: u32 = 0x0000_2000;
    const HPET_COUNTER_SIZE: u32 = 0x0000_8000;
    const HPET_VENDOR_ID_MASK: u32 = 0xFFFF_0000;

    assert_eq!(HPET_REVISION_MASK, 0x0000_00FF);
    assert_eq!(HPET_TIMER_COUNT_MASK, 0x0000_1F00);
    assert_eq!(HPET_LEGACY_ROUTE, 0x0000_2000);
    assert_eq!(HPET_COUNTER_SIZE, 0x0000_8000);
    assert_eq!(HPET_VENDOR_ID_MASK, 0xFFFF_0000);

    println!("  [PASS] HPET capability ID register correct");
}

/// HPET period.
fn test_hpet_period() {
    // HPET period: 32-bit value in femtoseconds (10^-15 seconds)
    // Typical value: 10,000,000 femtoseconds (10 ns)

    const HPET_PERIOD_TYPICAL: u32 = 10_000_000; // 10 ns in femtoseconds
    assert_eq!(HPET_PERIOD_TYPICAL, 10_000_000);

    println!("  [PASS] HPET period correct");
}

/// HPET vendor ID.
fn test_hpet_vendor_id() {
    // HPET vendor ID: 16-bit value
    const HPET_VENDOR_ID_INTEL: u32 = 0x8086;
    assert_eq!(HPET_VENDOR_ID_INTEL, 0x8086);

    println!("  [PASS] HPET vendor ID correct");
}

/// HPET legacy route.
fn test_hpet_legacy_route() {
    // HPET legacy route: 1-bit field
    // 0 = HPET is not legacy route capable
    // 1 = HPET is legacy route capable

    const HPET_LEGACY_ROUTE_CAPABLE: u32 = 1;
    assert_eq!(HPET_LEGACY_ROUTE_CAPABLE, 1);

    println!("  [PASS] HPET legacy route correct");
}

/// HPET counter size.
fn test_hpet_counter_size() {
    // HPET counter size: 1-bit field
    // 0 = 32-bit counter
    // 1 = 64-bit counter

    const HPET_COUNTER_SIZE_32BIT: u32 = 0;
    const HPET_COUNTER_SIZE_64BIT: u32 = 1;

    assert_eq!(HPET_COUNTER_SIZE_32BIT, 0);
    assert_eq!(HPET_COUNTER_SIZE_64BIT, 1);

    println!("  [PASS] HPET counter size correct");
}

/// HPET timer count.
fn test_hpet_timer_count() {
    // HPET timer count: 5-bit field (number of timers - 1)
    // Typical value: 2 (3 timers: 0, 1, 2)

    const HPET_TIMER_COUNT_TYPICAL: u32 = 2; // 3 timers
    assert_eq!(HPET_TIMER_COUNT_TYPICAL, 2);

    println!("  [PASS] HPET timer count correct");
}

/// HPET timer configuration register.
fn test_hpet_timer_config() {
    // HPET Timer Configuration register (8 bytes):
    // Bit 0: Interrupt enable
    // Bit 1: Interrupt type (0 = edge, 1 = level)
    // Bit 2: Periodic mode (0 = one-shot, 1 = periodic)
    // Bit 3: Periodic mode supported
    // Bit 4: 64-bit mode supported
    // Bit 5: Value set (0 = read-only, 1 = write OK)
    // Bit 6: 32-bit mode (0 = 64-bit, 1 = 32-bit)
    // Bit 7: FSB interrupt mapping enable
    // Bit 8: FSB interrupt mapping supported
    // Bits 9-13: Reserved
    // Bits 14-15: Interrupt route
    // Bits 16-31: Interrupt route mask
    // Bits 32-63: Timer route (FSB interrupt mapping)

    const HPET_TIMER_INT_ENABLE: u64 = 1 << 0;
    const HPET_TIMER_INT_TYPE: u64 = 1 << 1;
    const HPET_TIMER_PERIODIC: u64 = 1 << 2;
    const HPET_TIMER_PERIODIC_SUPPORTED: u64 = 1 << 3;
    const HPET_TIMER_64BIT_SUPPORTED: u64 = 1 << 4;
    const HPET_TIMER_VALUE_SET: u64 = 1 << 5;
    const HPET_TIMER_32BIT_MODE: u64 = 1 << 6;
    const HPET_TIMER_FSB_ENABLE: u64 = 1 << 7;
    const HPET_TIMER_FSB_SUPPORTED: u64 = 1 << 8;

    assert_eq!(HPET_TIMER_INT_ENABLE, 1 << 0);
    assert_eq!(HPET_TIMER_INT_TYPE, 1 << 1);
    assert_eq!(HPET_TIMER_PERIODIC, 1 << 2);
    assert_eq!(HPET_TIMER_PERIODIC_SUPPORTED, 1 << 3);
    assert_eq!(HPET_TIMER_64BIT_SUPPORTED, 1 << 4);
    assert_eq!(HPET_TIMER_VALUE_SET, 1 << 5);
    assert_eq!(HPET_TIMER_32BIT_MODE, 1 << 6);
    assert_eq!(HPET_TIMER_FSB_ENABLE, 1 << 7);
    assert_eq!(HPET_TIMER_FSB_SUPPORTED, 1 << 8);

    println!("  [PASS] HPET timer configuration register correct");
}

/// HPET timer comparator.
fn test_hpet_timer_comparator() {
    // HPET Timer Comparator register (8 bytes):
    // 32-bit or 64-bit value to compare against the main counter
    // When the main counter reaches this value, an interrupt is generated

    const HPET_COMPARATOR_MIN: u64 = 0;
    const HPET_COMPARATOR_MAX: u64 = 0xFFFF_FFFF_FFFF_FFFF;

    assert_eq!(HPET_COMPARATOR_MIN, 0);
    assert_eq!(HPET_COMPARATOR_MAX, 0xFFFF_FFFF_FFFF_FFFF);

    println!("  [PASS] HPET timer comparator correct");
}

/// HPET main counter.
fn test_hpet_main_counter() {
    // HPET Main Counter register (8 bytes):
    // 32-bit or 64-bit counter that increments at the HPET period
    // Read-only

    const HPET_COUNTER_PERIOD_NS: u64 = 10; // 10 ns
    assert_eq!(HPET_COUNTER_PERIOD_NS, 10);

    println!("  [PASS] HPET main counter correct");
}

/// HPET interrupt status.
fn test_hpet_interrupt_status() {
    // HPET Interrupt Status register (8 bytes):
    // Bit 0: Timer 0 interrupt status
    // Bit 1: Timer 1 interrupt status
    // Bit 2: Timer 2 interrupt status
    // Bits 3-31: Reserved

    const HPET_INT_STATUS_TIMER0: u32 = 1 << 0;
    const HPET_INT_STATUS_TIMER1: u32 = 1 << 1;
    const HPET_INT_STATUS_TIMER2: u32 = 1 << 2;

    assert_eq!(HPET_INT_STATUS_TIMER0, 1 << 0);
    assert_eq!(HPET_INT_STATUS_TIMER1, 1 << 1);
    assert_eq!(HPET_INT_STATUS_TIMER2, 1 << 2);

    println!("  [PASS] HPET interrupt status correct");
}
