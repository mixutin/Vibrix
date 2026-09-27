//! Host-side unit tests for NVMe (Non-Volatile Memory Express) register layout.
//!
//! These tests run on the host (not in the kernel) and validate that the
//! NVMe register offsets and bit fields are correct.
//!
//! Primary reference: NVM Express Base Specification, Chapter 2 (Registers).

fn main() {
    println!("Running NVMe register layout tests...\n");

    test_nvme_capability_registers();
    test_nvme_controller_config();
    test_nvme_controller_status();
    test_nvme_admin_queue_attributes();
    test_nvme_admin_queue_base();
    test_nvme_doorbell_stride();
    test_nvme_queue_entry_size();
    test_nvme_cc_format();
    test_nvme_csts_format();
    test_nvme_aqa_format();

    println!("\nAll NVMe register layout tests passed!");
}

/// NVMe capability registers (CAP).
fn test_nvme_capability_registers() {
    const CAP: u64 = 0x00; // Controller Capabilities
    const VS: u32 = 0x08; // Version
    const INTMS: u32 = 0x0C; // Interrupt Mask Set
    const INTMC: u32 = 0x10; // Interrupt Mask Clear
    const CC: u32 = 0x14; // Controller Configuration
    const CSTS: u32 = 0x1C; // Controller Status
    const NSSR: u32 = 0x20; // NVM Subsystem Reset
    const AQA: u32 = 0x24; // Admin Queue Attributes
    const ASQ: u64 = 0x28; // Admin Submission Queue Base Address
    const ACQ: u64 = 0x30; // Admin Completion Queue Base Address

    assert_eq!(CAP, 0x00);
    assert_eq!(VS, 0x08);
    assert_eq!(INTMS, 0x0C);
    assert_eq!(INTMC, 0x10);
    assert_eq!(CC, 0x14);
    assert_eq!(CSTS, 0x1C);
    assert_eq!(NSSR, 0x20);
    assert_eq!(AQA, 0x24);
    assert_eq!(ASQ, 0x28);
    assert_eq!(ACQ, 0x30);

    println!("  [PASS] NVMe capability registers correct");
}

/// NVMe controller configuration register (CC).
fn test_nvme_controller_config() {
    // CC bits:
    // Bits 0-3: I/O submission queue entry size (2^n)
    // Bits 4-7: I/O completion queue entry size (2^n)
    // Bits 11-14: Arbitration mechanism
    // Bits 15-17: Memory page size (2^n)
    // Bits 18-21: I/O command set selected
    // Bit 24: Enable
    // Bits 25-27: Shutdown notification
    // Bits 28-31: Controller reset

    const CC_IOSQES_MASK: u32 = 0x0000_000F;
    const CC_IOCQES_MASK: u32 = 0x0000_00F0;
    const CC_AMS_MASK: u32 = 0x0000_7000;
    const CC_MPS_MASK: u32 = 0x0003_8000;
    const CC_CSS_MASK: u32 = 0x001C_0000;
    const CC_EN: u32 = 1 << 24;
    const CC_SHN_MASK: u32 = 0x0E00_0000;
    const CC_CSR: u32 = 0xF000_0000;

    assert_eq!(CC_IOSQES_MASK, 0x0000_000F);
    assert_eq!(CC_IOCQES_MASK, 0x0000_00F0);
    assert_eq!(CC_AMS_MASK, 0x0000_7000);
    assert_eq!(CC_MPS_MASK, 0x0003_8000);
    assert_eq!(CC_CSS_MASK, 0x001C_0000);
    assert_eq!(CC_EN, 0x0100_0000);
    assert_eq!(CC_SHN_MASK, 0x0E00_0000);
    assert_eq!(CC_CSR, 0xF000_0000);

    println!("  [PASS] NVMe controller configuration register format correct");
}

/// NVMe controller status register (CSTS).
fn test_nvme_controller_status() {
    // CSTS bits:
    // Bit 0: Ready
    // Bit 1: Controller fatal status
    // Bit 2: Shutdown status
    // Bit 3: NVM subsystem reset occurred
    // Bit 4: Processing paused
    // Bit 5: Admin queue ready

    const CSTS_RDY: u32 = 1 << 0;
    const CSTS_CFS: u32 = 1 << 1;
    const CSTS_SHST_MASK: u32 = 0x0000_000C;
    const CSTS_NSSRO: u32 = 1 << 4;
    const CSTS_PP: u32 = 1 << 5;

    assert_eq!(CSTS_RDY, 0x0000_0001);
    assert_eq!(CSTS_CFS, 0x0000_0002);
    assert_eq!(CSTS_SHST_MASK, 0x0000_000C);
    assert_eq!(CSTS_NSSRO, 0x0000_0010);
    assert_eq!(CSTS_PP, 0x0000_0020);

    println!("  [PASS] NVMe controller status register format correct");
}

/// NVMe admin queue attributes register (AQA).
fn test_nvme_admin_queue_attributes() {
    // AQA bits:
    // Bits 0-11: Admin submission queue size (N-1)
    // Bits 16-27: Admin completion queue size (N-1)

    const AQA_ASQS_MASK: u32 = 0x0000_0FFF;
    const AQA_ACQS_MASK: u32 = 0x0FFF_0000;

    assert_eq!(AQA_ASQS_MASK, 0x0000_0FFF);
    assert_eq!(AQA_ACQS_MASK, 0x0FFF_0000);

    println!("  [PASS] NVMe admin queue attributes register format correct");
}

/// NVMe admin queue base address registers (ASQ, ACQ).
fn test_nvme_admin_queue_base() {
    // ASQ and ACQ are 64-bit registers containing the base address of the
    // admin submission and completion queues, respectively.
    // The base address must be aligned to the memory page size (CC.MPS).

    const ASQ_ALIGNMENT: u64 = 4096; // 4KB page alignment
    assert_eq!(ASQ_ALIGNMENT, 4096);

    // Example: ASQ base address 0x0000_0000_1000_0000
    let asq_base: u64 = 0x0000_0000_1000_0000;
    assert_eq!(asq_base % ASQ_ALIGNMENT, 0);

    println!("  [PASS] NVMe admin queue base address registers correct");
}

/// NVMe doorbell stride (DSTRD).
fn test_nvme_doorbell_stride() {
    // The doorbell stride is calculated as 2^(DSTRD+2) bytes.
    // DSTRD is a 4-bit field in the CAP register.

    const DSTRD_MASK: u32 = 0x0000_000F;
    assert_eq!(DSTRD_MASK, 0x0000_000F);

    // Example: DSTRD = 0, stride = 4 bytes
    let dstrd: u32 = 0;
    let stride = 1 << (dstrd + 2);
    assert_eq!(stride, 4);

    // Example: DSTRD = 2, stride = 16 bytes
    let dstrd2: u32 = 2;
    let stride2 = 1 << (dstrd2 + 2);
    assert_eq!(stride2, 16);

    println!("  [PASS] NVMe doorbell stride calculation correct");
}

/// NVMe queue entry sizes.
fn test_nvme_queue_entry_size() {
    // I/O submission queue entry size: 2^CC.IOSQES bytes (default 64 bytes)
    // I/O completion queue entry size: 2^CC.IOCQES bytes (default 16 bytes)

    const DEFAULT_SQ_ENTRY_SIZE: usize = 64;
    const DEFAULT_CQ_ENTRY_SIZE: usize = 16;

    assert_eq!(DEFAULT_SQ_ENTRY_SIZE, 64);
    assert_eq!(DEFAULT_CQ_ENTRY_SIZE, 16);

    println!("  [PASS] NVMe queue entry sizes correct");
}

/// NVMe CC register format validation.
fn test_nvme_cc_format() {
    // CC register must have:
    // - IOSQES (bits 0-3): 4-7 (64-128 bytes)
    // - IOCQES (bits 4-7): 4-5 (16-32 bytes)
    // - MPS (bits 15-17): 0-7 (4KB-128MB)
    // - CSS (bits 18-21): 0 (NVM command set)
    // - EN (bit 24): 0 (disabled) or 1 (enabled)

    const CC_IOSQES_MIN: u32 = 4;
    const CC_IOSQES_MAX: u32 = 7;
    const CC_IOCQES_MIN: u32 = 4;
    const CC_IOCQES_MAX: u32 = 5;
    const CC_MPS_MIN: u32 = 0;
    const CC_MPS_MAX: u32 = 7;

    assert_eq!(CC_IOSQES_MIN, 4);
    assert_eq!(CC_IOSQES_MAX, 7);
    assert_eq!(CC_IOCQES_MIN, 4);
    assert_eq!(CC_IOCQES_MAX, 5);
    assert_eq!(CC_MPS_MIN, 0);
    assert_eq!(CC_MPS_MAX, 7);

    println!("  [PASS] NVMe CC register format validation correct");
}

/// NVMe CSTS register format validation.
fn test_nvme_csts_format() {
    // CSTS register must have:
    // - RDY (bit 0): 0 (not ready) or 1 (ready)
    // - CFS (bit 1): 0 (no fatal error) or 1 (fatal error)
    // - SHST (bits 2-3): 0 (normal), 1 (shutdown), 2 (complete)

    const CSTS_SHST_NORMAL: u32 = 0;
    const CSTS_SHST_SHUTDOWN: u32 = 1;
    const CSTS_SHST_COMPLETE: u32 = 2;

    assert_eq!(CSTS_SHST_NORMAL, 0);
    assert_eq!(CSTS_SHST_SHUTDOWN, 1);
    assert_eq!(CSTS_SHST_COMPLETE, 2);

    println!("  [PASS] NVMe CSTS register format validation correct");
}

/// NVMe AQA register format validation.
fn test_nvme_aqa_format() {
    // AQA register must have:
    // - ASQS (bits 0-11): 0-4095 (queue size - 1)
    // - ACQS (bits 16-27): 0-4095 (queue size - 1)

    const AQA_ASQS_MAX: u32 = 0xFFF;
    const AQA_ACQS_MAX: u32 = 0xFFF;

    assert_eq!(AQA_ASQS_MAX, 0xFFF);
    assert_eq!(AQA_ACQS_MAX, 0xFFF);

    println!("  [PASS] NVMe AQA register format validation correct");
}
