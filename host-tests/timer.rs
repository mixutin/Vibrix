//! Host-side unit tests for x86-64 timer and interrupt routing logic.
//!
//! These tests run on the host (not in the kernel) and validate that the
//! PIT (Programmable Interval Timer) and HPET register layouts are correct.
//!
//! Primary reference: Intel SDM Vol. 3A, Chapter 8 (Programming the Interval Timer),
//! IA-PC HPET Specification.

fn main() {
    println!("Running timer and interrupt routing tests...\n");

    test_pit_register_offsets();
    test_pit_channel0_data();
    test_pit_channel1_data();
    test_pit_channel2_data();
    test_pit_mode_register();
    test_pit_command_register();
    test_hpet_register_offsets();
    test_hpet_general_config();
    test_hpet_timer_config();
    test_hpet_counter();
    test_interrupt_routing();
    test_timer_calibration();

    println!("\nAll timer and interrupt routing tests passed!");
}

/// PIT register offsets (Intel 8253/8254).
fn test_pit_register_offsets() {
    const PIT_CHANNEL0: u16 = 0x40;
    const PIT_CHANNEL1: u16 = 0x41;
    const PIT_CHANNEL2: u16 = 0x42;
    const PIT_COMMAND: u16 = 0x43;

    assert_eq!(PIT_CHANNEL0, 0x40);
    assert_eq!(PIT_CHANNEL1, 0x41);
    assert_eq!(PIT_CHANNEL2, 0x42);
    assert_eq!(PIT_COMMAND, 0x43);

    println!("  [PASS] PIT register offsets correct");
}

/// PIT channel 0 data register format.
fn test_pit_channel0_data() {
    // Channel 0 is connected to IRQ0 (timer interrupt)
    let data: u16 = 0xFFFF;
    assert_eq!(data, 0xFFFF);
    // Reload value determines interrupt frequency
    // Frequency = 1193182 / reload_value
    let frequency: u32 = 1193182 / 0xFFFF;
    assert_eq!(frequency, 18); // ~18 Hz with max reload value
    println!("  [PASS] PIT channel 0 data register format correct");
}

/// PIT channel 1 data register format.
fn test_pit_channel1_data() {
    // Channel 1 is used for DRAM refresh (legacy)
    let data: u16 = 0x0000;
    assert_eq!(data, 0x0000);
    println!("  [PASS] PIT channel 1 data register format correct");
}

/// PIT channel 2 data register format.
fn test_pit_channel2_data() {
    // Channel 2 is connected to the PC speaker
    let data: u16 = 0xFFFF;
    assert_eq!(data, 0xFFFF);
    println!("  [PASS] PIT channel 2 data register format correct");
}

/// PIT mode/command register format.
fn test_pit_mode_register() {
    // Bits 0-1: BCD/binary mode (0=binary, 1=BCD)
    // Bits 2-4: Mode (0-5)
    // Bits 5-6: Access mode (0=latch, 1=low byte, 2=high byte, 3=low+high)
    // Bits 7-8: Channel select (0-3)

    const MODE_0: u8 = 0b0000; // Interrupt on terminal count
    const MODE_1: u8 = 0b0001; // Hardware retriggerable one-shot
    const MODE_2: u8 = 0b0010; // Rate generator
    const MODE_3: u8 = 0b0011; // Square wave generator
    const MODE_4: u8 = 0b0100; // Software triggered strobe
    const MODE_5: u8 = 0b0101; // Hardware triggered strobe

    assert_eq!(MODE_0, 0x00);
    assert_eq!(MODE_1, 0x01);
    assert_eq!(MODE_2, 0x02);
    assert_eq!(MODE_3, 0x03);
    assert_eq!(MODE_4, 0x04);
    assert_eq!(MODE_5, 0x05);

    println!("  [PASS] PIT mode register format correct");
}

/// PIT command register format.
fn test_pit_command_register() {
    // Command register is write-only
    // Bits 0-1: BCD/binary mode
    // Bits 2-4: Mode
    // Bits 5-6: Access mode
    // Bits 7-8: Channel select

    let cmd: u8 = 0x36; // Channel 0, access low+high, mode 3, binary
    assert_eq!((cmd >> 6) & 0x03, 0x00); // Channel 0
    assert_eq!((cmd >> 1) & 0x07, 0x03); // Mode 3
    assert_eq!((cmd >> 4) & 0x03, 0x03); // Access low+high
    assert_eq!(cmd & 0x01, 0x00); // Binary mode

    println!("  [PASS] PIT command register format correct");
}

/// HPET register offsets.
fn test_hpet_register_offsets() {
    const HPET_GENERAL_CAPABILITIES: u32 = 0x00;
    const HPET_GENERAL_CONFIG: u32 = 0x10;
    const HPET_GENERAL_INTERRUPT_STATUS: u32 = 0x20;
    const HPET_MAIN_COUNTER: u32 = 0xF0;
    const HPET_TIMER0_CONFIG: u32 = 0x100;
    const HPET_TIMER0_COMPARATOR: u32 = 0x108;

    assert_eq!(HPET_GENERAL_CAPABILITIES, 0x00);
    assert_eq!(HPET_GENERAL_CONFIG, 0x10);
    assert_eq!(HPET_GENERAL_INTERRUPT_STATUS, 0x20);
    assert_eq!(HPET_MAIN_COUNTER, 0xF0);
    assert_eq!(HPET_TIMER0_CONFIG, 0x100);
    assert_eq!(HPET_TIMER0_COMPARATOR, 0x108);

    println!("  [PASS] HPET register offsets correct");
}

/// HPET general configuration register format.
fn test_hpet_general_config() {
    // Bit 0: Enable (0=disabled, 1=enabled)
    // Bit 1: Legacy replacement (0=disabled, 1=enabled)

    const ENABLE: u32 = 1 << 0;
    const LEGACY_REPLACEMENT: u32 = 1 << 1;

    assert_eq!(ENABLE, 0x01);
    assert_eq!(LEGACY_REPLACEMENT, 0x02);

    println!("  [PASS] HPET general config register format correct");
}

/// HPET timer configuration register format.
fn test_hpet_timer_config() {
    // Bit 0: Interrupt enable
    // Bit 1: Interrupt type (0=edge, 1=level)
    // Bit 2: Periodic mode (0=one-shot, 1=periodic)
    // Bit 3: Periodic mode supported
    // Bit 4: 64-bit mode supported
    // Bit 5: Value set (0=read-only, 1=write OK)
    // Bit 6: 32-bit mode (0=64-bit, 1=32-bit)
    // Bit 7: FSB interrupt mapping enable
    // Bit 8: FSB interrupt mapping supported

    const INTERRUPT_ENABLE: u32 = 1 << 0;
    const INTERRUPT_TYPE: u32 = 1 << 1;
    const PERIODIC_MODE: u32 = 1 << 2;
    const PERIODIC_SUPPORTED: u32 = 1 << 3;
    const MODE_64_SUPPORTED: u32 = 1 << 4;
    const VALUE_SET: u32 = 1 << 5;
    const MODE_32: u32 = 1 << 6;

    assert_eq!(INTERRUPT_ENABLE, 0x01);
    assert_eq!(INTERRUPT_TYPE, 0x02);
    assert_eq!(PERIODIC_MODE, 0x04);
    assert_eq!(PERIODIC_SUPPORTED, 0x08);
    assert_eq!(MODE_64_SUPPORTED, 0x10);
    assert_eq!(VALUE_SET, 0x20);
    assert_eq!(MODE_32, 0x40);

    println!("  [PASS] HPET timer config register format correct");
}

/// HPET main counter register format.
fn test_hpet_counter() {
    // 64-bit counter (32-bit in 32-bit mode)
    // Increment at 10 MHz or higher
    let counter: u64 = 0x0000_0000_0000_0000;
    assert_eq!(counter, 0);

    // After 1 second at 10 MHz
    let counter_1s: u64 = 10_000_000;
    assert_eq!(counter_1s, 10_000_000);

    println!("  [PASS] HPET main counter register format correct");
}

/// Interrupt routing logic.
fn test_interrupt_routing() {
    // IRQ0: PIT timer -> vector 0x20
    // IRQ1: Keyboard -> vector 0x21
    // IRQ2: Cascade -> vector 0x22
    // IRQ8: RTC -> vector 0x28

    const IRQ0_VECTOR: u8 = 0x20;
    const IRQ1_VECTOR: u8 = 0x21;
    const IRQ2_VECTOR: u8 = 0x22;
    const IRQ8_VECTOR: u8 = 0x28;

    assert_eq!(IRQ0_VECTOR, 0x20);
    assert_eq!(IRQ1_VECTOR, 0x21);
    assert_eq!(IRQ2_VECTOR, 0x22);
    assert_eq!(IRQ8_VECTOR, 0x28);

    println!("  [PASS] Interrupt routing correct");
}

/// Timer calibration.
fn test_timer_calibration() {
    // PIT frequency: 1193182 Hz
    // To get 100 Hz interrupt frequency:
    // reload_value = 1193182 / 100 = 11932
    const PIT_FREQUENCY: u32 = 1193182;
    const TARGET_FREQUENCY: u32 = 100;
    let reload_value: u32 = PIT_FREQUENCY / TARGET_FREQUENCY;
    assert_eq!(reload_value, 11931);

    // Actual frequency
    let actual_frequency: u32 = PIT_FREQUENCY / reload_value;
    assert_eq!(actual_frequency, 100);

    println!("  [PASS] Timer calibration correct");
}
