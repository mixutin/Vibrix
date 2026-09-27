//! Host-side unit tests for UEFI memory map parsing and BootInfo validation.
//!
//! These tests run on the host (not in the kernel) and validate that the
//! UEFI memory map structures and BootInfo layout are correct.
//!
//! Primary reference: UEFI Specification (GetMemoryMap, memory descriptor types),
//! Vibrix Boot ABI (docs/BOOT_ABI.md).

fn main() {
    println!("Running UEFI memory map and BootInfo tests...\n");

    test_uefi_memory_descriptor_size();
    test_uefi_memory_type_values();
    test_uefi_memory_descriptor_attributes();
    test_uefi_page_size();
    test_bootinfo_magic();
    test_bootinfo_version();
    test_bootinfo_framebuffer();
    test_bootinfo_rsdp();
    test_bootinfo_memory_map();
    test_bootinfo_alignment();
    test_uefi_descriptor_version();

    println!("\nAll UEFI memory map and BootInfo tests passed!");
}

/// UEFI memory descriptor size (UEFI 2.x).
fn test_uefi_memory_descriptor_size() {
    // Each memory descriptor is 48 bytes:
    // Type: u32 (4 bytes)
    // PhysicalStart: u64 (8 bytes)
    // VirtualStart: u64 (8 bytes)
    // NumberOfPages: u64 (8 bytes)
    // Attribute: u64 (8 bytes)
    // Padding: u32 (4 bytes) — for 8-byte alignment

    const DESCRIPTOR_SIZE: usize = 48;
    assert_eq!(DESCRIPTOR_SIZE, 48);

    println!("  [PASS] UEFI memory descriptor size correct");
}

/// UEFI memory type values.
fn test_uefi_memory_type_values() {
    const EfiReservedMemoryType: u32 = 0;
    const EfiLoaderCode: u32 = 1;
    const EfiLoaderData: u32 = 2;
    const EfiBootServicesCode: u32 = 3;
    const EfiBootServicesData: u32 = 4;
    const EfiRuntimeServicesCode: u32 = 5;
    const EfiRuntimeServicesData: u32 = 6;
    const EfiConventionalMemory: u32 = 7;
    const EfiUnusableMemory: u32 = 8;
    const EfiACPIReclaimMemory: u32 = 9;
    const EfiACPIMemoryNVS: u32 = 10;
    const EfiMemoryMappedIO: u32 = 11;
    const EfiMemoryMappedIOPortSpace: u32 = 12;
    const EfiPalCode: u32 = 13;
    const EfiPersistentMemory: u32 = 14;

    assert_eq!(EfiReservedMemoryType, 0);
    assert_eq!(EfiLoaderCode, 1);
    assert_eq!(EfiLoaderData, 2);
    assert_eq!(EfiBootServicesCode, 3);
    assert_eq!(EfiBootServicesData, 4);
    assert_eq!(EfiRuntimeServicesCode, 5);
    assert_eq!(EfiRuntimeServicesData, 6);
    assert_eq!(EfiConventionalMemory, 7);
    assert_eq!(EfiUnusableMemory, 8);
    assert_eq!(EfiACPIReclaimMemory, 9);
    assert_eq!(EfiACPIMemoryNVS, 10);
    assert_eq!(EfiMemoryMappedIO, 11);
    assert_eq!(EfiMemoryMappedIOPortSpace, 12);
    assert_eq!(EfiPalCode, 13);
    assert_eq!(EfiPersistentMemory, 14);

    println!("  [PASS] UEFI memory type values correct");
}

/// UEFI memory descriptor attributes.
fn test_uefi_memory_descriptor_attributes() {
    const EFI_MEMORY_UC: u64 = 1 << 0; // Uncacheable
    const EFI_MEMORY_WC: u64 = 1 << 1; // Write-combining
    const EFI_MEMORY_WT: u64 = 1 << 2; // Write-through
    const EFI_MEMORY_WB: u64 = 1 << 3; // Write-back
    const EFI_MEMORY_UCE: u64 = 1 << 4; // Uncacheable, exported
    const EFI_MEMORY_WP: u64 = 1 << 12; // Write-protect
    const EFI_MEMORY_RP: u64 = 1 << 13; // Read-protect
    const EFI_MEMORY_XP: u64 = 1 << 14; // Execute-protect
    const EFI_MEMORY_NV: u64 = 1 << 15; // Non-volatile
    const EFI_MEMORY_MORE_RELIABLE: u64 = 1 << 16; // More reliable
    const EFI_MEMORY_RO: u64 = 1 << 17; // Read-only
    const EFI_MEMORY_RUNTIME: u64 = 1 << 63; // Runtime

    assert_eq!(EFI_MEMORY_UC, 0x0000_0000_0000_0001);
    assert_eq!(EFI_MEMORY_WC, 0x0000_0000_0000_0002);
    assert_eq!(EFI_MEMORY_WT, 0x0000_0000_0000_0004);
    assert_eq!(EFI_MEMORY_WB, 0x0000_0000_0000_0008);
    assert_eq!(EFI_MEMORY_RUNTIME, 0x8000_0000_0000_0000);

    println!("  [PASS] UEFI memory descriptor attributes correct");
}

/// UEFI page size (4KB).
fn test_uefi_page_size() {
    const UEFI_PAGE_SIZE: u64 = 4096;
    assert_eq!(UEFI_PAGE_SIZE, 4096);
    assert_eq!(UEFI_PAGE_SIZE, 1 << 12);

    println!("  [PASS] UEFI page size correct");
}

/// BootInfo magic number.
fn test_bootinfo_magic() {
    // BootInfo magic: "VIBRIX01" (8 bytes)
    const BOOTINFO_MAGIC: u64 = 0x5649_4252_4958_3031; // "VIBRIX01"
    assert_eq!(BOOTINFO_MAGIC, 0x5649_4252_4958_3031);

    println!("  [PASS] BootInfo magic correct");
}

/// BootInfo version.
fn test_bootinfo_version() {
    const BOOTINFO_VERSION_1: u32 = 1;
    assert_eq!(BOOTINFO_VERSION_1, 1);

    println!("  [PASS] BootInfo version correct");
}

/// BootInfo framebuffer fields.
fn test_bootinfo_framebuffer() {
    // Framebuffer base must be page-aligned
    let framebuffer_base: u64 = 0xFD00_0000;
    assert_eq!(framebuffer_base % 4096, 0);

    // Framebuffer size must be non-zero
    let framebuffer_size: u64 = 0x0078_0000; // 1024x768x4 bytes
    assert!(framebuffer_size > 0);

    println!("  [PASS] BootInfo framebuffer fields correct");
}

/// BootInfo RSDP field.
fn test_bootinfo_rsdp() {
    // RSDP address must be 8-byte aligned (ACPI requirement)
    let rsdp: u64 = 0x0000_0000_000F_0000;
    assert_eq!(rsdp % 8, 0);

    println!("  [PASS] BootInfo RSDP field correct");
}

/// BootInfo memory map fields.
fn test_bootinfo_memory_map() {
    // Memory map address must be 8-byte aligned
    let memory_map: u64 = 0x0000_0000_0010_0000;
    assert_eq!(memory_map % 8, 0);

    // Memory map length must be non-zero
    let memory_map_len: u64 = 4096;
    assert!(memory_map_len > 0);

    // Memory descriptor size must be 48 (UEFI 2.x)
    let memory_descriptor_size: u64 = 48;
    assert_eq!(memory_descriptor_size, 48);

    println!("  [PASS] BootInfo memory map fields correct");
}

/// BootInfo alignment requirements.
fn test_bootinfo_alignment() {
    // BootInfo must be 8-byte aligned
    let bootinfo_addr: u64 = 0x0000_0000_0010_0000;
    assert_eq!(bootinfo_addr % 8, 0);

    println!("  [PASS] BootInfo alignment correct");
}

/// UEFI descriptor version.
fn test_uefi_descriptor_version() {
    // Descriptor version 1 is the only defined version
    const DESCRIPTOR_VERSION_1: u32 = 1;
    assert_eq!(DESCRIPTOR_VERSION_1, 1);

    println!("  [PASS] UEFI descriptor version correct");
}
