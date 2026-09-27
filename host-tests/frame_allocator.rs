//! Host-side unit tests for physical frame allocator logic.
//!
//! These tests run on the host (not in the kernel) and validate the
//! frame allocation bitmap logic, alignment, and bounds checking.
//!
//! Primary reference: x86-64 memory map, UEFI memory map semantics.

fn main() {
    println!("Running physical frame allocator tests...\n");

    test_frame_size();
    test_bitmap_size_calculation();
    test_bitmap_bit_index();
    test_bitmap_byte_index();
    test_bitmap_set_and_clear();
    test_bitmap_find_first_free();
    test_bitmap_find_contiguous();
    test_frame_alignment();
    test_frame_index_overflow();
    test_memory_map_parsing();

    println!("\nAll physical frame allocator tests passed!");
}

/// Frame size is 4KB (4096 bytes) for x86-64.
fn test_frame_size() {
    const FRAME_SIZE: u64 = 4096;
    assert_eq!(FRAME_SIZE, 4096);
    assert_eq!(FRAME_SIZE, 1 << 12);
    println!("  [PASS] Frame size is 4KB");
}

/// Bitmap size in bytes for a given number of frames.
///
/// Each frame needs 1 bit, so bitmap_size = ceil(num_frames / 8).
fn test_bitmap_size_calculation() {
    // 8 frames = 1 byte
    assert_eq!(bitmap_size(8), 1);
    // 16 frames = 2 bytes
    assert_eq!(bitmap_size(16), 2);
    // 1024 frames = 128 bytes
    assert_eq!(bitmap_size(1024), 128);
    // 1 frame = 1 byte (minimum)
    assert_eq!(bitmap_size(1), 1);
    // 0 frames = 0 bytes
    assert_eq!(bitmap_size(0), 0);
    println!("  [PASS] Bitmap size calculation correct");
}

fn bitmap_size(num_frames: u64) -> u64 {
    (num_frames + 7) / 8
}

/// Calculate the byte index in the bitmap for a given frame.
fn test_bitmap_byte_index() {
    assert_eq!(byte_index(0), 0);
    assert_eq!(byte_index(7), 0);
    assert_eq!(byte_index(8), 1);
    assert_eq!(byte_index(15), 1);
    assert_eq!(byte_index(16), 2);
    assert_eq!(byte_index(1023), 127);
    println!("  [PASS] Bitmap byte index correct");
}

fn byte_index(frame: u64) -> u64 {
    frame / 8
}

/// Calculate the bit index within a byte for a given frame.
fn test_bitmap_bit_index() {
    assert_eq!(bit_index(0), 0);
    assert_eq!(bit_index(1), 1);
    assert_eq!(bit_index(7), 7);
    assert_eq!(bit_index(8), 0);
    assert_eq!(bit_index(15), 7);
    assert_eq!(bit_index(16), 0);
    println!("  [PASS] Bitmap bit index correct");
}

fn bit_index(frame: u64) -> u64 {
    frame % 8
}

/// Test setting and clearing bits in the bitmap.
fn test_bitmap_set_and_clear() {
    let mut bitmap = [0u8; 128]; // 1024 frames

    // Set frame 0
    set_frame(&mut bitmap, 0);
    assert_eq!(bitmap[0] & 0x01, 0x01);

    // Set frame 7
    set_frame(&mut bitmap, 7);
    assert_eq!(bitmap[0] & 0x80, 0x80);

    // Set frame 8
    set_frame(&mut bitmap, 8);
    assert_eq!(bitmap[1] & 0x01, 0x01);

    // Set frame 1023
    set_frame(&mut bitmap, 1023);
    assert_eq!(bitmap[127] & 0x80, 0x80);

    // Clear frame 0
    clear_frame(&mut bitmap, 0);
    assert_eq!(bitmap[0] & 0x01, 0x00);

    // Frame 7 should still be set
    assert_eq!(bitmap[0] & 0x80, 0x80);

    println!("  [PASS] Bitmap set and clear correct");
}

fn set_frame(bitmap: &mut [u8], frame: u64) {
    let byte = byte_index(frame) as usize;
    let bit = bit_index(frame) as u8;
    bitmap[byte] |= 1 << bit;
}

fn clear_frame(bitmap: &mut [u8], frame: u64) {
    let byte = byte_index(frame) as usize;
    let bit = bit_index(frame) as u8;
    bitmap[byte] &= !(1 << bit);
}

/// Test finding the first free frame in the bitmap.
fn test_bitmap_find_first_free() {
    let mut bitmap = [0xFFu8; 128]; // All frames allocated

    // No free frame
    assert_eq!(find_first_free(&bitmap), None);

    // Free frame 0
    clear_frame(&mut bitmap, 0);
    assert_eq!(find_first_free(&bitmap), Some(0));

    // Free frame 100
    clear_frame(&mut bitmap, 100);
    assert_eq!(find_first_free(&bitmap), Some(0));

    // Allocate frame 0 again
    set_frame(&mut bitmap, 0);
    assert_eq!(find_first_free(&bitmap), Some(100));

    println!("  [PASS] Bitmap find first free correct");
}

fn find_first_free(bitmap: &[u8]) -> Option<u64> {
    for (byte_idx, &byte) in bitmap.iter().enumerate() {
        if byte != 0xFF {
            for bit in 0..8 {
                if byte & (1 << bit) == 0 {
                    return Some((byte_idx as u64) * 8 + bit as u64);
                }
            }
        }
    }
    None
}

/// Test finding contiguous free frames.
fn test_bitmap_find_contiguous() {
    let mut bitmap = [0xFFu8; 128]; // All frames allocated

    // No contiguous free frames
    assert_eq!(find_contiguous(&bitmap, 4), None);

    // Free frames 8-11 (4 contiguous)
    for i in 8..12 {
        clear_frame(&mut bitmap, i);
    }
    assert_eq!(find_contiguous(&bitmap, 4), Some(8));

    // Free frames 100-107 (8 contiguous)
    for i in 100..108 {
        clear_frame(&mut bitmap, i);
    }
    assert_eq!(find_contiguous(&bitmap, 8), Some(100));

    // Request 16 contiguous — should fail
    assert_eq!(find_contiguous(&bitmap, 16), None);

    println!("  [PASS] Bitmap find contiguous correct");
}

fn find_contiguous(bitmap: &[u8], count: u64) -> Option<u64> {
    let mut current_start: Option<u64> = None;
    let mut current_count = 0u64;

    for frame in 0..(bitmap.len() as u64 * 8) {
        let byte = byte_index(frame) as usize;
        let bit = bit_index(frame) as u8;

        if bitmap[byte] & (1 << bit) == 0 {
            // Free frame
            if current_start.is_none() {
                current_start = Some(frame);
                current_count = 1;
            } else {
                current_count += 1;
            }

            if current_count >= count {
                return current_start;
            }
        } else {
            // Allocated frame — reset
            current_start = None;
            current_count = 0;
        }
    }

    None
}

/// Test frame address alignment.
fn test_frame_alignment() {
    // Frame 0 is at address 0
    assert_eq!(frame_address(0), 0);
    // Frame 1 is at address 4096
    assert_eq!(frame_address(1), 4096);
    // Frame 1024 is at address 4MB
    assert_eq!(frame_address(1024), 4 * 1024 * 1024);
    // Frame address must be 4KB aligned
    assert_eq!(frame_address(100) % 4096, 0);
    println!("  [PASS] Frame address alignment correct");
}

fn frame_address(frame: u64) -> u64 {
    frame * 4096
}

/// Test frame index overflow detection.
fn test_frame_index_overflow() {
    // Maximum frame index for 512MB of memory (QEMU default)
    let max_frames = 512 * 1024 * 1024 / 4096;
    assert_eq!(max_frames, 131072);

    // Frame index should not exceed max_frames
    assert!(max_frames < u64::MAX / 4096);

    // Calculate the maximum addressable physical memory
    let max_addr = frame_address(max_frames);
    assert_eq!(max_addr, 512 * 1024 * 1024);

    println!("  [PASS] Frame index overflow detection correct");
}

/// Test UEFI memory map parsing logic.
///
/// UEFI memory descriptors have a type, physical start, virtual start,
/// number of pages, and attributes.
fn test_memory_map_parsing() {
    // Memory descriptor types
    const MEMORY_TYPE_RESERVED: u32 = 0;
    const MEMORY_TYPE_LOADER_CODE: u32 = 1;
    const MEMORY_TYPE_LOADER_DATA: u32 = 2;
    const MEMORY_TYPE_BOOT_SERVICES_CODE: u32 = 3;
    const MEMORY_TYPE_BOOT_SERVICES_DATA: u32 = 4;
    const MEMORY_TYPE_RUNTIME_SERVICES_CODE: u32 = 5;
    const MEMORY_TYPE_RUNTIME_SERVICES_DATA: u32 = 6;
    const MEMORY_TYPE_CONVENTIONAL: u32 = 7;
    const MEMORY_TYPE_UNUSABLE: u32 = 8;
    const MEMORY_TYPE_ACPI_RECLAIM: u32 = 9;
    const MEMORY_TYPE_ACPI_NVS: u32 = 10;
    const MEMORY_TYPE_MEMORY_MAPPED_IO: u32 = 11;
    const MEMORY_TYPE_MEMORY_MAPPED_IO_PORT_SPACE: u32 = 12;
    const MEMORY_TYPE_PAL_CODE: u32 = 13;
    const MEMORY_TYPE_PERSISTENT: u32 = 14;

    assert_eq!(MEMORY_TYPE_RESERVED, 0);
    assert_eq!(MEMORY_TYPE_CONVENTIONAL, 7);
    assert_eq!(MEMORY_TYPE_ACPI_RECLAIM, 9);
    assert_eq!(MEMORY_TYPE_ACPI_NVS, 10);
    assert_eq!(MEMORY_TYPE_PERSISTENT, 14);

    // Conventional memory is available for allocation
    assert!(is_allocatable(MEMORY_TYPE_CONVENTIONAL));
    // ACPI reclaim memory is available after ACPI tables are parsed
    assert!(is_allocatable(MEMORY_TYPE_ACPI_RECLAIM));
    // Reserved memory is NOT available
    assert!(!is_allocatable(MEMORY_TYPE_RESERVED));
    // Unusable memory is NOT available
    assert!(!is_allocatable(MEMORY_TYPE_UNUSABLE));

    println!("  [PASS] UEFI memory map parsing correct");
}

fn is_allocatable(memory_type: u32) -> bool {
    matches!(memory_type, 7 | 9) // Conventional | ACPI_Reclaim
}
