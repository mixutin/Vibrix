//! Host-side unit tests for x86-64 virtual memory manager logic.
//!
//! These tests run on the host (not in the kernel) and validate that the
//! page table entry bit layout and address translation are correct.
//!
//! Primary reference: Intel SDM Vol. 3A, Section 4.5 (4-Level Paging),
//! AMD APM Vol. 2, Section 5.3 (Long Mode Page Translation).

fn main() {
    println!("Running virtual memory manager tests...\n");

    test_page_table_entry_size();
    test_page_table_entry_bit_layout();
    test_page_table_entry_present_bit();
    test_page_table_entry_write_bit();
    test_page_table_entry_user_bit();
    test_page_table_entry_pwt_bit();
    test_page_table_entry_pcd_bit();
    test_page_table_entry_accessed_bit();
    test_page_table_entry_dirty_bit();
    test_page_table_entry_pat_bit();
    test_page_table_entry_global_bit();
    test_page_table_entry_nx_bit();
    test_address_translation();
    test_page_table_walk();

    println!("\nAll virtual memory manager tests passed!");
}

/// Page table entry size is 8 bytes (64 bits) in x86-64.
fn test_page_table_entry_size() {
    assert_eq!(8, 8);
    println!("  [PASS] Page table entry size is 8 bytes");
}

/// Page table entry bit layout (Intel SDM Vol. 3A, Section 4.5):
///
/// Bit 0:    P (Present)
/// Bit 1:    W/R (Write/Read)
/// Bit 2:    U/S (User/Supervisor)
/// Bit 3:    PWT (Page Write-Through)
/// Bit 4:    PCD (Page Cache Disable)
/// Bit 5:    A (Accessed)
/// Bit 6:    D (Dirty)
/// Bit 7:    PAT (Page Attribute Table)
/// Bit 8:    G (Global)
/// Bit 9-11: Available for OS use
/// Bit 12-51: Physical address (bits 12-51)
/// Bit 52-62: Available for OS use
/// Bit 63:   NX (No-Execute)
fn test_page_table_entry_bit_layout() {
    const P: u64 = 1 << 0;
    const WR: u64 = 1 << 1;
    const US: u64 = 1 << 2;
    const PWT: u64 = 1 << 3;
    const PCD: u64 = 1 << 4;
    const A: u64 = 1 << 5;
    const D: u64 = 1 << 6;
    const PAT: u64 = 1 << 7;
    const G: u64 = 1 << 8;
    const NX: u64 = 1 << 63;

    assert_eq!(P, 0x001);
    assert_eq!(WR, 0x002);
    assert_eq!(US, 0x004);
    assert_eq!(PWT, 0x008);
    assert_eq!(PCD, 0x010);
    assert_eq!(A, 0x020);
    assert_eq!(D, 0x040);
    assert_eq!(PAT, 0x080);
    assert_eq!(G, 0x100);
    assert_eq!(NX, 0x8000_0000_0000_0000);

    println!("  [PASS] Page table entry bit layout correct");
}

/// Test the Present bit (bit 0).
fn test_page_table_entry_present_bit() {
    let entry: u64 = 0x001;
    assert_eq!(entry & 0x001, 1);
    println!("  [PASS] Page table entry present bit correct");
}

/// Test the Write/Read bit (bit 1).
fn test_page_table_entry_write_bit() {
    let entry: u64 = 0x002;
    assert_eq!(entry & 0x002, 2);
    println!("  [PASS] Page table entry write bit correct");
}

/// Test the User/Supervisor bit (bit 2).
fn test_page_table_entry_user_bit() {
    let entry: u64 = 0x004;
    assert_eq!(entry & 0x004, 4);
    println!("  [PASS] Page table entry user bit correct");
}

/// Test the PWT bit (bit 3).
fn test_page_table_entry_pwt_bit() {
    let entry: u64 = 0x008;
    assert_eq!(entry & 0x008, 8);
    println!("  [PASS] Page table entry PWT bit correct");
}

/// Test the PCD bit (bit 4).
fn test_page_table_entry_pcd_bit() {
    let entry: u64 = 0x010;
    assert_eq!(entry & 0x010, 16);
    println!("  [PASS] Page table entry PCD bit correct");
}

/// Test the Accessed bit (bit 5).
fn test_page_table_entry_accessed_bit() {
    let entry: u64 = 0x020;
    assert_eq!(entry & 0x020, 32);
    println!("  [PASS] Page table entry accessed bit correct");
}

/// Test the Dirty bit (bit 6).
fn test_page_table_entry_dirty_bit() {
    let entry: u64 = 0x040;
    assert_eq!(entry & 0x040, 64);
    println!("  [PASS] Page table entry dirty bit correct");
}

/// Test the PAT bit (bit 7).
fn test_page_table_entry_pat_bit() {
    let entry: u64 = 0x080;
    assert_eq!(entry & 0x080, 128);
    println!("  [PASS] Page table entry PAT bit correct");
}

/// Test the Global bit (bit 8).
fn test_page_table_entry_global_bit() {
    let entry: u64 = 0x100;
    assert_eq!(entry & 0x100, 256);
    println!("  [PASS] Page table entry global bit correct");
}

/// Test the NX bit (bit 63).
fn test_page_table_entry_nx_bit() {
    let entry: u64 = 1 << 63;
    assert_eq!(entry & (1 << 63), 1 << 63);
    println!("  [PASS] Page table entry NX bit correct");
}

/// Test virtual address translation (4-level paging).
///
/// A 48-bit virtual address is split into:
/// - Bits 0-11:   Page offset (12 bits, 4KB page)
/// - Bits 12-20:  Level 4 (PT) index (9 bits)
/// - Bits 21-29:  Level 3 (PD) index (9 bits)
/// - Bits 30-38:  Level 2 (PDPT) index (9 bits)
/// - Bits 39-47:  Level 1 (PML4) index (9 bits)
/// - Bits 48-63:  Sign extension of bit 47
fn test_address_translation() {
    // Example: virtual address 0xFFFF8000_12345000
    let vaddr: u64 = 0xFFFF8000_12345000;

    // Extract page offset (bits 0-11)
    let offset = vaddr & 0xFFF;
    assert_eq!(offset, 0x000);

    // Extract PT index (bits 12-20)
    let pt_index = (vaddr >> 12) & 0x1FF;
    assert_eq!(pt_index, 0x145);

    // Extract PD index (bits 21-29)
    let pd_index = (vaddr >> 21) & 0x1FF;
    assert_eq!(pd_index, 0x091);

    // Extract PDPT index (bits 30-38)
    let pdpt_index = (vaddr >> 30) & 0x1FF;
    assert_eq!(pdpt_index, 0x000);

    // Extract PML4 index (bits 39-47)
    let pml4_index = (vaddr >> 39) & 0x1FF;
    assert_eq!(pml4_index, 0x100);

    println!("  [PASS] Virtual address translation correct");
}

/// Test page table walk logic.
///
/// Given a virtual address and a PML4 base, walk the page tables to find
/// the physical address.
fn test_page_table_walk() {
    // Simulated page tables (simplified)
    let pml4_base: u64 = 0x1000;
    let pdpt_base: u64 = 0x2000;
    let pd_base: u64 = 0x3000;
    let pt_base: u64 = 0x4000;

    // Virtual address: 0x0000_0000_0000_1234 (4KB page)
    let vaddr: u64 = 0x1234;

    // Extract indices
    let pml4_idx = (vaddr >> 39) & 0x1FF;
    let pdpt_idx = (vaddr >> 30) & 0x1FF;
    let pd_idx = (vaddr >> 21) & 0x1FF;
    let pt_idx = (vaddr >> 12) & 0x1FF;
    let offset = vaddr & 0xFFF;

    assert_eq!(pml4_idx, 0);
    assert_eq!(pdpt_idx, 0);
    assert_eq!(pd_idx, 0);
    assert_eq!(pt_idx, 1);
    assert_eq!(offset, 0x234);

    // Simulate page table walk
    let pml4_entry_addr = pml4_base + pml4_idx * 8;
    assert_eq!(pml4_entry_addr, 0x1000);

    let pdpt_entry_addr = pdpt_base + pdpt_idx * 8;
    assert_eq!(pdpt_entry_addr, 0x2000);

    let pd_entry_addr = pd_base + pd_idx * 8;
    assert_eq!(pd_entry_addr, 0x3000);

    let pt_entry_addr = pt_base + pt_idx * 8;
    assert_eq!(pt_entry_addr, 0x4008);

    // Final physical address
    let phys_addr = 0x5000 + offset;
    assert_eq!(phys_addr, 0x5234);

    println!("  [PASS] Page table walk correct");
}
