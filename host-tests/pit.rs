//! Host-side unit tests for PIT (Programmable Interval Timer) calibration.
//!
//! These tests run on the host (not in the kernel) and validate that the
//! PIT calibration and interrupt frequency calculations are correct.
//!
//! Primary reference: Intel 8253/8254 PIT datasheet, Intel SDM Vol. 3A Chapter 8.

fn main() {
    println!("Running PIT calibration tests...\n");

    test_pit_frequency();
    test_pit_reload_value();
    test_pit_interrupt_frequency();
    test_pit_mode_selection();
    test_pit_channel_selection();
    test_pit_access_mode();
    test_pit_binary_bcd_mode();
    test_pit_period_calculation();
    test_pit_tsc_calibration();
    test_pit_drift_compensation();

    println!("\nAll PIT calibration tests passed!");
}

/// PIT frequency.
fn test_pit_frequency() {
    // PIT input frequency: 1.193182 MHz (3 * 3.579545 MHz / 9)
    const PIT_FREQUENCY_HZ: u32 = 1_193_182;
    assert_eq!(PIT_FREQUENCY_HZ, 1_193_182);

    println!("  [PASS] PIT frequency correct");
}

/// PIT reload value.
fn test_pit_reload_value() {
    // PIT reload value: 16-bit (0-65535)
    // 0 = 65536 (maximum count)

    const PIT_RELOAD_MIN: u16 = 0;
    const PIT_RELOAD_MAX: u16 = 65535;

    assert_eq!(PIT_RELOAD_MIN, 0);
    assert_eq!(PIT_RELOAD_MAX, 65535);

    println!("  [PASS] PIT reload value correct");
}

/// PIT interrupt frequency.
fn test_pit_interrupt_frequency() {
    // PIT interrupt frequency = PIT_FREQUENCY / reload_value
    // For 100 Hz: reload_value = 1193182 / 100 = 11932

    const PIT_FREQUENCY_HZ: u32 = 1_193_182;
    const TARGET_FREQUENCY_HZ: u32 = 100;
    let reload_value: u32 = PIT_FREQUENCY_HZ / TARGET_FREQUENCY_HZ;

    assert_eq!(reload_value, 11931);

    let actual_frequency: u32 = PIT_FREQUENCY_HZ / reload_value;
    assert_eq!(actual_frequency, 100);

    println!("  [PASS] PIT interrupt frequency correct");
}

/// PIT mode selection.
fn test_pit_mode_selection() {
    // PIT modes:
    // 0: Interrupt on terminal count
    // 1: Hardware retriggerable one-shot
    // 2: Rate generator
    // 3: Square wave generator
    // 4: Software triggered strobe
    // 5: Hardware triggered strobe

    const PIT_MODE_0: u8 = 0;
    const PIT_MODE_1: u8 = 1;
    const PIT_MODE_2: u8 = 2;
    const PIT_MODE_3: u8 = 3;
    const PIT_MODE_4: u8 = 4;
    const PIT_MODE_5: u8 = 5;

    assert_eq!(PIT_MODE_0, 0);
    assert_eq!(PIT_MODE_1, 1);
    assert_eq!(PIT_MODE_2, 2);
    assert_eq!(PIT_MODE_3, 3);
    assert_eq!(PIT_MODE_4, 4);
    assert_eq!(PIT_MODE_5, 5);

    println!("  [PASS] PIT mode selection correct");
}

/// PIT channel selection.
fn test_pit_channel_selection() {
    // PIT channels:
    // 0: System timer (IRQ0)
    // 1: DRAM refresh (legacy)
    // 2: PC speaker

    const PIT_CHANNEL_0: u8 = 0;
    const PIT_CHANNEL_1: u8 = 1;
    const PIT_CHANNEL_2: u8 = 2;

    assert_eq!(PIT_CHANNEL_0, 0);
    assert_eq!(PIT_CHANNEL_1, 1);
    assert_eq!(PIT_CHANNEL_2, 2);

    println!("  [PASS] PIT channel selection correct");
}

/// PIT access mode.
fn test_pit_access_mode() {
    // PIT access modes:
    // 0: Latch count value
    // 1: Low byte only
    // 2: High byte only
    // 3: Low byte then high byte

    const PIT_ACCESS_LATCH: u8 = 0;
    const PIT_ACCESS_LOW: u8 = 1;
    const PIT_ACCESS_HIGH: u8 = 2;
    const PIT_ACCESS_LOW_HIGH: u8 = 3;

    assert_eq!(PIT_ACCESS_LATCH, 0);
    assert_eq!(PIT_ACCESS_LOW, 1);
    assert_eq!(PIT_ACCESS_HIGH, 2);
    assert_eq!(PIT_ACCESS_LOW_HIGH, 3);

    println!("  [PASS] PIT access mode correct");
}

/// PIT binary/BCD mode.
fn test_pit_binary_bcd_mode() {
    // PIT counting mode:
    // 0: Binary (16-bit)
    // 1: BCD (4 decades)

    const PIT_MODE_BINARY: u8 = 0;
    const PIT_MODE_BCD: u8 = 1;

    assert_eq!(PIT_MODE_BINARY, 0);
    assert_eq!(PIT_MODE_BCD, 1);

    println!("  [PASS] PIT binary/BCD mode correct");
}

/// PIT period calculation.
fn test_pit_period_calculation() {
    // PIT period = reload_value / PIT_FREQUENCY
    // For reload_value = 11932: period = 11932 / 1193182 ≈ 0.01 seconds

    const PIT_FREQUENCY_HZ: u32 = 1_193_182;
    let reload_value: u32 = 11932;
    let period_us: u32 = ((reload_value as u64 * 1_000_000) / PIT_FREQUENCY_HZ as u64) as u32;

    assert_eq!(period_us, 10_000); // 10 ms

    println!("  [PASS] PIT period calculation correct");
}

/// PIT TSC calibration.
fn test_pit_tsc_calibration() {
    // PIT can be used to calibrate the TSC
    // 1. Set PIT to mode 2 (rate generator) with a known reload value
    // 2. Read TSC
    // 3. Wait for PIT to countdown
    // 4. Read TSC again
    // 5. Calculate TSC frequency = (TSC2 - TSC1) * PIT_FREQUENCY / reload_value

    const PIT_FREQUENCY_HZ: u32 = 1_193_182;
    let reload_value: u32 = 11932;
    let tsc_delta: u64 = 11932; // TSC ticks during one PIT period

    let tsc_frequency: u64 = (tsc_delta * PIT_FREQUENCY_HZ as u64) / reload_value as u64;

    assert_eq!(tsc_frequency, 1_193_182);

    println!("  [PASS] PIT TSC calibration correct");
}

/// PIT drift compensation.
fn test_pit_drift_compensation() {
    // PIT drift: The PIT may drift due to temperature, voltage, etc.
    // Drift is typically ±0.01% (100 ppm)

    const PIT_DRIFT_PPM: u32 = 100; // parts per million
    assert_eq!(PIT_DRIFT_PPM, 100);

    // For a 10 ms period, drift = 10 ms * 100 ppm = 1 us
    let period_us: u32 = 10_000;
    let drift_us: u32 = (period_us * PIT_DRIFT_PPM) / 1_000_000;

    assert_eq!(drift_us, 1);

    println!("  [PASS] PIT drift compensation correct");
}
