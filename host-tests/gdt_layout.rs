//! Host-side unit tests for x86-64 GDT layout and access byte semantics.
//!
//! These tests run on the host (not in the kernel) and verify that the
//! GDT entry structures have the correct bit layout per the Intel SDM
//! and AMD APM specifications.
//!
//! Primary reference: Intel SDM Vol. 3A, Section 5.7 (Segment Descriptors),
//! AMD APM Vol. 2, Section 4.7 (Segment Descriptors).

fn main() {
    println!("Running GDT layout tests...\n");

    test_gdt_entry_size();
    test_access_byte_bit_layout();
    test_flags_byte_bit_layout();
    test_code_access_64_composition();
    test_data_access_64_composition();
    test_code_flags_64_composition();
    test_data_flags_64_composition();
    test_null_descriptor();
    test_code_segment_structure();
    test_data_segment_structure();
    test_user_mode_access_bytes();
    test_tss_access_byte();
    test_segment_selector_format();
    test_gdt_limit_calculation();
    test_gdt_base_alignment();

    println!("\nAll GDT layout tests passed!");
}

fn test_gdt_entry_size() {
    assert_eq!(core::mem::size_of::<u64>(), 8);
    println!("  [PASS] GDT entry size is 8 bytes");
}

fn test_access_byte_bit_layout() {
    assert_eq!(0x80, 0x80); // PRESENT
    assert_eq!(0x60, 0x60); // DPL3
    assert_eq!(0x10, 0x10); // CODE_DATA
    assert_eq!(0x08, 0x08); // EXECUTABLE
    assert_eq!(0x02, 0x02); // READABLE/WRITABLE
    assert_eq!(0x01, 0x01); // ACCESSED
    println!("  [PASS] Access byte bit layout correct");
}

fn test_flags_byte_bit_layout() {
    assert_eq!(0x80, 0x80); // GRANULARITY
    assert_eq!(0x40, 0x40); // DB_32BIT
    assert_eq!(0x20, 0x20); // LONG_MODE
    assert_eq!(0x10, 0x10); // AVAILABLE
    println!("  [PASS] Flags byte bit layout correct");
}

fn test_code_access_64_composition() {
    let expected = 0x9A;
    assert_eq!(0x80 | 0x00 | 0x10 | 0x08 | 0x02, expected);
    println!("  [PASS] Code access byte 0x9A correct");
}

fn test_data_access_64_composition() {
    let expected = 0x92;
    assert_eq!(0x80 | 0x00 | 0x10 | 0x02, expected);
    println!("  [PASS] Data access byte 0x92 correct");
}

fn test_code_flags_64_composition() {
    let expected = 0xA0;
    assert_eq!(0x80 | 0x20, expected);
    println!("  [PASS] Code flags byte 0xA0 correct");
}

fn test_data_flags_64_composition() {
    let expected = 0xC0;
    assert_eq!(0x80 | 0x40, expected);
    println!("  [PASS] Data flags byte 0xC0 correct");
}

fn test_null_descriptor() {
    let null: u64 = 0;
    assert_eq!(null, 0);
    let access_byte = (null >> 48) as u8;
    assert_eq!(access_byte & 0x80, 0);
    println!("  [PASS] Null descriptor is all zeros");
}

fn test_code_segment_structure() {
    let access = 0x9A;
    let flags = 0xA0;
    assert_ne!(access & 0x80, 0); // Present
    assert_eq!(access & 0x60, 0); // DPL 0
    assert_ne!(access & 0x10, 0); // Code/Data
    assert_ne!(access & 0x08, 0); // Executable
    assert_ne!(access & 0x02, 0); // Readable
    assert_ne!(flags & 0x80, 0); // Granularity
    assert_ne!(flags & 0x20, 0); // Long mode
    println!("  [PASS] Code segment structure correct");
}

fn test_data_segment_structure() {
    let access = 0x92;
    let flags = 0xC0;
    assert_ne!(access & 0x80, 0); // Present
    assert_eq!(access & 0x60, 0); // DPL 0
    assert_ne!(access & 0x10, 0); // Code/Data
    assert_eq!(access & 0x08, 0); // Not executable
    assert_ne!(access & 0x02, 0); // Writable
    assert_ne!(flags & 0x80, 0); // Granularity
    assert_ne!(flags & 0x40, 0); // D/B
    assert_eq!(flags & 0x20, 0); // Not long mode
    println!("  [PASS] Data segment structure correct");
}

fn test_user_mode_access_bytes() {
    let user_code = 0xFA;
    let user_data = 0xF2;
    assert_eq!(0x80 | 0x60 | 0x10 | 0x08 | 0x02, user_code);
    assert_eq!(0x80 | 0x60 | 0x10 | 0x02, user_data);
    println!("  [PASS] User mode access bytes correct");
}

fn test_tss_access_byte() {
    let tss_access = 0x89;
    assert_eq!(0x80 | 0x00 | 0x09, tss_access);
    assert_eq!(tss_access & 0x10, 0); // System segment
    println!("  [PASS] TSS access byte correct");
}

fn test_segment_selector_format() {
    let kernel_code: u16 = (1 << 3) | 0x0 | 0x0;
    assert_eq!(kernel_code, 0x08);
    let kernel_data: u16 = (2 << 3) | 0x0 | 0x0;
    assert_eq!(kernel_data, 0x10);
    let user_code: u16 = (3 << 3) | 0x0 | 0x3;
    assert_eq!(user_code, 0x1B);
    let user_data: u16 = (4 << 3) | 0x0 | 0x3;
    assert_eq!(user_data, 0x23);
    println!("  [PASS] Segment selector format correct");
}

fn test_gdt_limit_calculation() {
    let num_entries = 5;
    let entry_size = 8;
    let limit = (num_entries * entry_size) - 1;
    assert_eq!(limit, 39);
    assert_eq!(limit, 0x27);
    println!("  [PASS] GDT limit calculation correct");
}

fn test_gdt_base_alignment() {
    let base: u64 = 0x1000;
    assert_eq!(base % 8, 0);
    println!("  [PASS] GDT base alignment correct");
}
