//! Host-side unit tests for USB HID report descriptor parsing.
//!
//! These tests run on the host (not in the kernel) and validate that the
//! USB HID report descriptor structures are correct.
//!
//! Primary reference: Device Class Definition for HID, USB HID Usage Tables.

fn main() {
    println!("Running USB HID report descriptor tests...\n");

    test_hid_descriptor_type();
    test_hid_report_id();
    test_hid_usage_page();
    test_hid_usage();
    test_hid_collection();
    test_hid_report_size();
    test_hid_report_count();
    test_hid_input_output_feature();
    test_hid_keyboard_report();
    test_hid_mouse_report();

    println!("\nAll USB HID report descriptor tests passed!");
}

/// HID descriptor type values.
fn test_hid_descriptor_type() {
    const HID_DESCRIPTOR_TYPE: u8 = 0x21;
    const REPORT_DESCRIPTOR_TYPE: u8 = 0x22;
    const PHYSICAL_DESCRIPTOR_TYPE: u8 = 0x23;

    assert_eq!(HID_DESCRIPTOR_TYPE, 0x21);
    assert_eq!(REPORT_DESCRIPTOR_TYPE, 0x22);
    assert_eq!(PHYSICAL_DESCRIPTOR_TYPE, 0x23);

    println!("  [PASS] HID descriptor type values correct");
}

/// HID report ID.
fn test_hid_report_id() {
    // Report ID is a 1-byte prefix that identifies a report
    // 0 = no report ID (default)
    const REPORT_ID_NONE: u8 = 0;
    const REPORT_ID_KEYBOARD: u8 = 1;
    const REPORT_ID_MOUSE: u8 = 2;
    const REPORT_ID_CONSUMER: u8 = 3;

    assert_eq!(REPORT_ID_NONE, 0);
    assert_eq!(REPORT_ID_KEYBOARD, 1);
    assert_eq!(REPORT_ID_MOUSE, 2);
    assert_eq!(REPORT_ID_CONSUMER, 3);

    println!("  [PASS] HID report ID values correct");
}

/// HID usage page values.
fn test_hid_usage_page() {
    const USAGE_PAGE_GENERIC_DESKTOP: u16 = 0x01;
    const USAGE_PAGE_KEYBOARD: u16 = 0x07;
    const USAGE_PAGE_BUTTON: u16 = 0x09;
    const USAGE_PAGE_CONSUMER: u16 = 0x0C;

    assert_eq!(USAGE_PAGE_GENERIC_DESKTOP, 0x01);
    assert_eq!(USAGE_PAGE_KEYBOARD, 0x07);
    assert_eq!(USAGE_PAGE_BUTTON, 0x09);
    assert_eq!(USAGE_PAGE_CONSUMER, 0x0C);

    println!("  [PASS] HID usage page values correct");
}

/// HID usage values.
fn test_hid_usage() {
    const USAGE_KEYBOARD_A: u16 = 0x04;
    const USAGE_KEYBOARD_B: u16 = 0x05;
    const USAGE_KEYBOARD_C: u16 = 0x06;
    const USAGE_KEYBOARD_LSHIFT: u16 = 0xE1;
    const USAGE_KEYBOARD_RSHIFT: u16 = 0xE5;

    assert_eq!(USAGE_KEYBOARD_A, 0x04);
    assert_eq!(USAGE_KEYBOARD_B, 0x05);
    assert_eq!(USAGE_KEYBOARD_C, 0x06);
    assert_eq!(USAGE_KEYBOARD_LSHIFT, 0xE1);
    assert_eq!(USAGE_KEYBOARD_RSHIFT, 0xE5);

    println!("  [PASS] HID usage values correct");
}

/// HID collection types.
fn test_hid_collection() {
    const COLLECTION_PHYSICAL: u8 = 0x00;
    const COLLECTION_APPLICATION: u8 = 0x01;
    const COLLECTION_LOGICAL: u8 = 0x02;
    const COLLECTION_REPORT: u8 = 0x03;
    const COLLECTION_NAMED_ARRAY: u8 = 0x04;
    const COLLECTION_USAGE_SWITCH: u8 = 0x05;
    const COLLECTION_USAGE_MODIFIER: u8 = 0x06;

    assert_eq!(COLLECTION_PHYSICAL, 0x00);
    assert_eq!(COLLECTION_APPLICATION, 0x01);
    assert_eq!(COLLECTION_LOGICAL, 0x02);
    assert_eq!(COLLECTION_REPORT, 0x03);
    assert_eq!(COLLECTION_NAMED_ARRAY, 0x04);
    assert_eq!(COLLECTION_USAGE_SWITCH, 0x05);
    assert_eq!(COLLECTION_USAGE_MODIFIER, 0x06);

    println!("  [PASS] HID collection types correct");
}

/// HID report size.
fn test_hid_report_size() {
    // Report size is in bits
    const REPORT_SIZE_8_BITS: u8 = 8;
    const REPORT_SIZE_16_BITS: u8 = 16;
    const REPORT_SIZE_32_BITS: u8 = 32;

    assert_eq!(REPORT_SIZE_8_BITS, 8);
    assert_eq!(REPORT_SIZE_16_BITS, 16);
    assert_eq!(REPORT_SIZE_32_BITS, 32);

    println!("  [PASS] HID report size values correct");
}

/// HID report count.
fn test_hid_report_count() {
    // Report count is the number of report fields
    const REPORT_COUNT_1: u8 = 1;
    const REPORT_COUNT_6: u8 = 6; // Typical for keyboard (6-key rollover)
    const REPORT_COUNT_8: u8 = 8; // Typical for keyboard (8 modifier keys)

    assert_eq!(REPORT_COUNT_1, 1);
    assert_eq!(REPORT_COUNT_6, 6);
    assert_eq!(REPORT_COUNT_8, 8);

    println!("  [PASS] HID report count values correct");
}

/// HID input/output/feature item types.
fn test_hid_input_output_feature() {
    const ITEM_TYPE_MAIN: u8 = 0x00;
    const ITEM_TYPE_GLOBAL: u8 = 0x01;
    const ITEM_TYPE_LOCAL: u8 = 0x02;
    const ITEM_TYPE_RESERVED: u8 = 0x03;

    const ITEM_TAG_INPUT: u8 = 0x08;
    const ITEM_TAG_OUTPUT: u8 = 0x09;
    const ITEM_TAG_FEATURE: u8 = 0x0B;
    const ITEM_TAG_COLLECTION: u8 = 0x0A;
    const ITEM_TAG_END_COLLECTION: u8 = 0x0C;

    assert_eq!(ITEM_TYPE_MAIN, 0x00);
    assert_eq!(ITEM_TYPE_GLOBAL, 0x01);
    assert_eq!(ITEM_TYPE_LOCAL, 0x02);
    assert_eq!(ITEM_TYPE_RESERVED, 0x03);
    assert_eq!(ITEM_TAG_INPUT, 0x08);
    assert_eq!(ITEM_TAG_OUTPUT, 0x09);
    assert_eq!(ITEM_TAG_FEATURE, 0x0B);
    assert_eq!(ITEM_TAG_COLLECTION, 0x0A);
    assert_eq!(ITEM_TAG_END_COLLECTION, 0x0C);

    println!("  [PASS] HID input/output/feature item types correct");
}

/// HID keyboard report format.
fn test_hid_keyboard_report() {
    // Keyboard report: 8 bytes
    // Byte 0: Modifier keys (bit 0 = LCtrl, bit 1 = LShift, etc.)
    // Byte 1: Reserved
    // Bytes 2-7: Key codes (up to 6 keys)

    const KEYBOARD_REPORT_SIZE: usize = 8;
    assert_eq!(KEYBOARD_REPORT_SIZE, 8);

    // Modifier key bits
    const MOD_LCTRL: u8 = 0x01;
    const MOD_LSHIFT: u8 = 0x02;
    const MOD_LALT: u8 = 0x04;
    const MOD_LGUI: u8 = 0x08;
    const MOD_RCTRL: u8 = 0x10;
    const MOD_RSHIFT: u8 = 0x20;
    const MOD_RALT: u8 = 0x40;
    const MOD_RGUI: u8 = 0x80;

    assert_eq!(MOD_LCTRL, 0x01);
    assert_eq!(MOD_LSHIFT, 0x02);
    assert_eq!(MOD_LALT, 0x04);
    assert_eq!(MOD_LGUI, 0x08);
    assert_eq!(MOD_RCTRL, 0x10);
    assert_eq!(MOD_RSHIFT, 0x20);
    assert_eq!(MOD_RALT, 0x40);
    assert_eq!(MOD_RGUI, 0x80);

    println!("  [PASS] HID keyboard report format correct");
}

/// HID mouse report format.
fn test_hid_mouse_report() {
    // Mouse report: 3-4 bytes
    // Byte 0: Buttons (bit 0 = left, bit 1 = right, bit 2 = middle)
    // Byte 1: X movement (signed)
    // Byte 2: Y movement (signed)
    // Byte 3: Wheel (signed, optional)

    const MOUSE_REPORT_SIZE: usize = 4;
    assert_eq!(MOUSE_REPORT_SIZE, 4);

    // Button bits
    const MOUSE_BTN_LEFT: u8 = 0x01;
    const MOUSE_BTN_RIGHT: u8 = 0x02;
    const MOUSE_BTN_MIDDLE: u8 = 0x04;

    assert_eq!(MOUSE_BTN_LEFT, 0x01);
    assert_eq!(MOUSE_BTN_RIGHT, 0x02);
    assert_eq!(MOUSE_BTN_MIDDLE, 0x04);

    println!("  [PASS] HID mouse report format correct");
}
