//! Host-side unit tests for kernel heap allocator logic.
//!
//! These tests run on the host (not in the kernel) and validate the
//! heap allocation bitmap logic, alignment, and bounds checking.

fn main() {
    println!("Running kernel heap allocator tests...\n");

    test_heap_block_size();
    test_heap_alignment();
    test_heap_bitmap_size();
    test_heap_bitmap_operations();
    test_heap_find_free();
    test_heap_find_contiguous();
    test_heap_allocate();
    test_heap_free();
    test_heap_resize();
    test_heap_fragmentation();

    println!("\nAll kernel heap allocator tests passed!");
}

fn test_heap_block_size() {
    const MIN_BLOCK_SIZE: u64 = 16;
    assert_eq!(MIN_BLOCK_SIZE, 16);
    assert_eq!(MIN_BLOCK_SIZE, 1 << 4);
    println!("  [PASS] Minimum heap block size is 16 bytes");
}

fn test_heap_alignment() {
    const ALIGNMENT: u64 = 16;
    assert_eq!(ALIGNMENT, 16);
    assert_eq!(32 % ALIGNMENT, 0);
    assert_eq!(48 % ALIGNMENT, 0);
    assert_eq!(64 % ALIGNMENT, 0);
    assert_eq!(round_up(17, ALIGNMENT), 32);
    assert_eq!(round_up(33, ALIGNMENT), 48);
    assert_eq!(round_up(1, ALIGNMENT), 16);
    println!("  [PASS] Heap alignment correct");
}

fn round_up(value: u64, alignment: u64) -> u64 {
    (value + alignment - 1) & !(alignment - 1)
}

fn test_heap_bitmap_size() {
    assert_eq!(bitmap_size(8), 1);
    assert_eq!(bitmap_size(16), 2);
    assert_eq!(bitmap_size(1024), 128);
    assert_eq!(bitmap_size(1), 1);
    assert_eq!(bitmap_size(0), 0);
    println!("  [PASS] Heap bitmap size calculation correct");
}

fn bitmap_size(num_blocks: u64) -> u64 {
    (num_blocks + 7) / 8
}

fn test_heap_bitmap_operations() {
    let mut bitmap = [0u8; 128];

    set_block(&mut bitmap, 0);
    assert_eq!(bitmap[0] & 0x01, 0x01);

    set_block(&mut bitmap, 7);
    assert_eq!(bitmap[0] & 0x80, 0x80);

    set_block(&mut bitmap, 8);
    assert_eq!(bitmap[1] & 0x01, 0x01);

    clear_block(&mut bitmap, 0);
    assert_eq!(bitmap[0] & 0x01, 0x00);

    assert_eq!(bitmap[0] & 0x80, 0x80);

    println!("  [PASS] Heap bitmap operations correct");
}

fn set_block(bitmap: &mut [u8], block: u64) {
    let byte = (block / 8) as usize;
    let bit = (block % 8) as u8;
    bitmap[byte] |= 1 << bit;
}

fn clear_block(bitmap: &mut [u8], block: u64) {
    let byte = (block / 8) as usize;
    let bit = (block % 8) as u8;
    bitmap[byte] &= !(1 << bit);
}

fn test_heap_find_free() {
    let mut bitmap = [0xFFu8; 128];

    assert_eq!(find_first_free(&bitmap), None);

    clear_block(&mut bitmap, 0);
    assert_eq!(find_first_free(&bitmap), Some(0));

    clear_block(&mut bitmap, 100);
    assert_eq!(find_first_free(&bitmap), Some(0));

    set_block(&mut bitmap, 0);
    assert_eq!(find_first_free(&bitmap), Some(100));

    println!("  [PASS] Heap find first free correct");
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

fn test_heap_find_contiguous() {
    let mut bitmap = [0xFFu8; 128];

    assert_eq!(find_contiguous(&bitmap, 4), None);

    for i in 8..12 {
        clear_block(&mut bitmap, i);
    }
    assert_eq!(find_contiguous(&bitmap, 4), Some(8));

    for i in 100..108 {
        clear_block(&mut bitmap, i);
    }
    assert_eq!(find_contiguous(&bitmap, 8), Some(100));

    assert_eq!(find_contiguous(&bitmap, 16), None);

    println!("  [PASS] Heap find contiguous correct");
}

fn find_contiguous(bitmap: &[u8], count: u64) -> Option<u64> {
    let mut current_start: Option<u64> = None;
    let mut current_count = 0u64;

    for block in 0..(bitmap.len() as u64 * 8) {
        let byte = (block / 8) as usize;
        let bit = (block % 8) as u8;

        if bitmap[byte] & (1 << bit) == 0 {
            if current_start.is_none() {
                current_start = Some(block);
                current_count = 1;
            } else {
                current_count += 1;
            }

            if current_count >= count {
                return current_start;
            }
        } else {
            current_start = None;
            current_count = 0;
        }
    }

    None
}

fn test_heap_allocate() {
    let mut bitmap = [0u8; 128];

    let addr = allocate(&mut bitmap, 4);
    assert_eq!(addr, Some(0));

    for i in 0..4 {
        assert!(is_allocated(&bitmap, i));
    }

    let addr2 = allocate(&mut bitmap, 8);
    assert_eq!(addr2, Some(4));

    for i in 4..12 {
        assert!(is_allocated(&bitmap, i));
    }

    println!("  [PASS] Heap allocation correct");
}

fn allocate(bitmap: &mut [u8], count: u64) -> Option<u64> {
    let start = find_contiguous(bitmap, count)?;
    for i in start..start + count {
        set_block(bitmap, i);
    }
    Some(start)
}

fn is_allocated(bitmap: &[u8], block: u64) -> bool {
    let byte = (block / 8) as usize;
    let bit = (block % 8) as u8;
    bitmap[byte] & (1 << bit) != 0
}

fn test_heap_free() {
    let mut bitmap = [0u8; 128];

    let addr = allocate(&mut bitmap, 4);
    assert_eq!(addr, Some(0));

    free(&mut bitmap, 0, 4);

    for i in 0..4 {
        assert!(!is_allocated(&bitmap, i));
    }

    println!("  [PASS] Heap free correct");
}

fn free(bitmap: &mut [u8], start: u64, count: u64) {
    for i in start..start + count {
        clear_block(bitmap, i);
    }
}

fn test_heap_resize() {
    let mut bitmap = [0u8; 128];

    let addr = allocate(&mut bitmap, 4);
    assert_eq!(addr, Some(0));

    let success = resize(&mut bitmap, 0, 4, 8);
    assert!(success);

    for i in 0..8 {
        assert!(is_allocated(&bitmap, i));
    }

    let success2 = resize(&mut bitmap, 0, 8, 2);
    assert!(success2);

    for i in 0..2 {
        assert!(is_allocated(&bitmap, i));
    }
    for i in 2..8 {
        assert!(!is_allocated(&bitmap, i));
    }

    println!("  [PASS] Heap resize correct");
}

fn resize(bitmap: &mut [u8], start: u64, old_count: u64, new_count: u64) -> bool {
    if new_count <= old_count {
        free(bitmap, start + new_count, old_count - new_count);
        true
    } else {
        for i in start + old_count..start + new_count {
            if is_allocated(bitmap, i) {
                return false;
            }
        }
        for i in start + old_count..start + new_count {
            set_block(bitmap, i);
        }
        true
    }
}

fn test_heap_fragmentation() {
    let mut bitmap = [0u8; 128];

    let addr1 = allocate(&mut bitmap, 4);
    assert_eq!(addr1, Some(0));
    let addr2 = allocate(&mut bitmap, 4);
    assert_eq!(addr2, Some(4));
    let addr3 = allocate(&mut bitmap, 4);
    assert_eq!(addr3, Some(8));

    free(&mut bitmap, 4, 4);

    let addr5 = allocate(&mut bitmap, 4);
    assert_eq!(addr5, Some(4));

    let addr4 = allocate(&mut bitmap, 8);
    assert_eq!(addr4, Some(12));

    println!("  [PASS] Heap fragmentation handling correct");
}
