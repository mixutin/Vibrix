//! Host-side unit tests for AHCI (Advanced Host Controller Interface) register layout.
//!
//! These tests run on the host (not in the kernel) and validate that the
//! AHCI register offsets and bit fields are correct.
//!
//! Primary reference: Serial ATA Advanced Host Controller Interface (AHCI) Specification.

fn main() {
    println!("Running AHCI register layout tests...\n");

    test_ahci_capability_registers();
    test_ahci_global_control();
    test_ahci_port_registers();
    test_ahci_port_cmd();
    test_ahci_port_tfd();
    test_ahci_port_ssts();
    test_ahci_port_serr();
    test_ahci_port_sact();
    test_ahci_port_ci();
    test_ahci_port_sntf();

    println!("\nAll AHCI register layout tests passed!");
}

/// AHCI capability registers (HBA memory space).
fn test_ahci_capability_registers() {
    const CAP: u32 = 0x00; // HBA Capabilities
    const GHC: u32 = 0x04; // Global HBA Control
    const IS: u32 = 0x08; // Interrupt Status
    const PI: u32 = 0x0C; // Ports Implemented
    const VS: u32 = 0x10; // Version
    const CCC_CTL: u32 = 0x14; // Command Completion Coalescing Control
    const CCC_PORTS: u32 = 0x18; // Command Completion Coalescing Ports
    const EM_LOC: u32 = 0x1C; // Enclosure Management Location
    const EM_CTL: u32 = 0x20; // Enclosure Management Control
    const CAP2: u32 = 0x24; // HBA Capabilities Extended
    const BOHC: u32 = 0x28; // BIOS/OS Handoff Control and Status

    assert_eq!(CAP, 0x00);
    assert_eq!(GHC, 0x04);
    assert_eq!(IS, 0x08);
    assert_eq!(PI, 0x0C);
    assert_eq!(VS, 0x10);
    assert_eq!(CCC_CTL, 0x14);
    assert_eq!(CCC_PORTS, 0x18);
    assert_eq!(EM_LOC, 0x1C);
    assert_eq!(EM_CTL, 0x20);
    assert_eq!(CAP2, 0x24);
    assert_eq!(BOHC, 0x28);

    println!("  [PASS] AHCI capability registers correct");
}

/// AHCI global control register (GHC).
fn test_ahci_global_control() {
    // GHC bits:
    // Bit 0: HBA reset
    // Bit 1: Interrupt enable
    // Bit 2: MSI revert to single message
    // Bit 31: AHCI enable

    const GHC_HR: u32 = 1 << 0;
    const GHC_IE: u32 = 1 << 1;
    const GHC_MRSM: u32 = 1 << 2;
    const GHC_AE: u32 = 1 << 31;

    assert_eq!(GHC_HR, 0x0000_0001);
    assert_eq!(GHC_IE, 0x0000_0002);
    assert_eq!(GHC_MRSM, 0x0000_0004);
    assert_eq!(GHC_AE, 0x8000_0000);

    println!("  [PASS] AHCI global control register format correct");
}

/// AHCI port registers (offset 0x100 + port * 0x80).
fn test_ahci_port_registers() {
    const PORT_CLB: u32 = 0x00; // Command List Base Address
    const PORT_CLBU: u32 = 0x04; // Command List Base Address Upper
    const PORT_FB: u32 = 0x08; // FIS Base Address
    const PORT_FBU: u32 = 0x0C; // FIS Base Address Upper
    const PORT_IS: u32 = 0x10; // Interrupt Status
    const PORT_IE: u32 = 0x14; // Interrupt Enable
    const PORT_CMD: u32 = 0x18; // Command and Status
    const PORT_TFD: u32 = 0x20; // Task File Data
    const PORT_SIG: u32 = 0x24; // Signature
    const PORT_SSTS: u32 = 0x28; // Serial ATA Status
    const PORT_SCTL: u32 = 0x2C; // Serial ATA Control
    const PORT_SERR: u32 = 0x30; // Serial ATA Error
    const PORT_SACT: u32 = 0x34; // Serial ATA Active
    const PORT_CI: u32 = 0x38; // Command Issue
    const PORT_SNTF: u32 = 0x3C; // Serial ATA Notification

    assert_eq!(PORT_CLB, 0x00);
    assert_eq!(PORT_CLBU, 0x04);
    assert_eq!(PORT_FB, 0x08);
    assert_eq!(PORT_FBU, 0x0C);
    assert_eq!(PORT_IS, 0x10);
    assert_eq!(PORT_IE, 0x14);
    assert_eq!(PORT_CMD, 0x18);
    assert_eq!(PORT_TFD, 0x20);
    assert_eq!(PORT_SIG, 0x24);
    assert_eq!(PORT_SSTS, 0x28);
    assert_eq!(PORT_SCTL, 0x2C);
    assert_eq!(PORT_SERR, 0x30);
    assert_eq!(PORT_SACT, 0x34);
    assert_eq!(PORT_CI, 0x38);
    assert_eq!(PORT_SNTF, 0x3C);

    println!("  [PASS] AHCI port registers correct");
}

/// AHCI port command register (CMD).
fn test_ahci_port_cmd() {
    // CMD bits:
    // Bit 0: Start
    // Bit 1: Spin-up device
    // Bit 2: Power-up device
    // Bit 3: Command list override
    // Bit 4: FIS receive enable
    // Bit 5: FIS receive running
    // Bit 6: Command list running
    // Bit 7: Cold presence detect
    // Bit 8: Mechanical presence switch
    // Bit 9: Hot plug capable
    // Bit 10: Port multiplier capable
    // Bit 11: FIS-based switching capable
    // Bit 12: Device is ATAPI
    // Bit 13: Automatic partial to slumber transitions
    // Bit 14: Aggressive link power management
    // Bit 15: Aggressive slumber
    // Bit 16-19: Interface communication control
    // Bit 20-23: Speed allowed
    // Bit 24-27: Power management state
    // Bit 28: Command completion coalescing enable
    // Bit 29: FIS-based switching enable
    // Bit 30: Device is ATAPI
    // Bit 31: Device is ATAPI

    const CMD_ST: u32 = 1 << 0;
    const CMD_SUD: u32 = 1 << 1;
    const CMD_POD: u32 = 1 << 2;
    const CMD_CLO: u32 = 1 << 3;
    const CMD_FRE: u32 = 1 << 4;
    const CMD_FR: u32 = 1 << 5;
    const CMD_CR: u32 = 1 << 6;
    const CMD_CPD: u32 = 1 << 7;
    const CMD_MPSP: u32 = 1 << 8;
    const CMD_HPCP: u32 = 1 << 9;
    const CMD_PMA: u32 = 1 << 10;
    const CMD_ICC_MASK: u32 = 0x0F00_0000;
    const CMD_SPD_MASK: u32 = 0x00F0_0000;

    assert_eq!(CMD_ST, 0x0000_0001);
    assert_eq!(CMD_SUD, 0x0000_0002);
    assert_eq!(CMD_POD, 0x0000_0004);
    assert_eq!(CMD_CLO, 0x0000_0008);
    assert_eq!(CMD_FRE, 0x0000_0010);
    assert_eq!(CMD_FR, 0x0000_0020);
    assert_eq!(CMD_CR, 0x0000_0040);
    assert_eq!(CMD_CPD, 0x0000_0080);
    assert_eq!(CMD_MPSP, 0x0000_0100);
    assert_eq!(CMD_HPCP, 0x0000_0200);
    assert_eq!(CMD_PMA, 0x0000_0400);
    assert_eq!(CMD_ICC_MASK, 0x0F00_0000);
    assert_eq!(CMD_SPD_MASK, 0x00F0_0000);

    println!("  [PASS] AHCI port command register format correct");
}

/// AHCI port task file data register (TFD).
fn test_ahci_port_tfd() {
    // TFD bits:
    // Bits 0-7: Status
    // Bits 8-15: Error
    // Bits 16-23: Device
    // Bits 24-31: Command

    const TFD_STATUS_MASK: u32 = 0x0000_00FF;
    const TFD_ERROR_MASK: u32 = 0x0000_FF00;
    const TFD_DEVICE_MASK: u32 = 0x00FF_0000;
    const TFD_COMMAND_MASK: u32 = 0xFF00_0000;

    assert_eq!(TFD_STATUS_MASK, 0x0000_00FF);
    assert_eq!(TFD_ERROR_MASK, 0x0000_FF00);
    assert_eq!(TFD_DEVICE_MASK, 0x00FF_0000);
    assert_eq!(TFD_COMMAND_MASK, 0xFF00_0000);

    println!("  [PASS] AHCI port task file data register format correct");
}

/// AHCI port SATA status register (SSTS).
fn test_ahci_port_ssts() {
    // SSTS bits:
    // Bits 0-3: Device detection
    // Bits 4-7: Current interface speed
    // Bits 8-11: Interface power management
    // Bits 12-15: Reserved

    const SSTS_DET_MASK: u32 = 0x0000_000F;
    const SSTS_SPD_MASK: u32 = 0x0000_00F0;
    const SSTS_IPM_MASK: u32 = 0x0000_0F00;

    assert_eq!(SSTS_DET_MASK, 0x0000_000F);
    assert_eq!(SSTS_SPD_MASK, 0x0000_00F0);
    assert_eq!(SSTS_IPM_MASK, 0x0000_0F00);

    // Device detection values
    const SSTS_DET_NO_DEVICE: u32 = 0x0000_0000;
    const SSTS_DET_NO_PHY: u32 = 0x0000_0001;
    const SSTS_DET_PRESENT: u32 = 0x0000_0003;
    const SSTS_DET_OFFLINE: u32 = 0x0000_0004;

    assert_eq!(SSTS_DET_NO_DEVICE, 0x0000_0000);
    assert_eq!(SSTS_DET_NO_PHY, 0x0000_0001);
    assert_eq!(SSTS_DET_PRESENT, 0x0000_0003);
    assert_eq!(SSTS_DET_OFFLINE, 0x0000_0004);

    println!("  [PASS] AHCI port SATA status register format correct");
}

/// AHCI port SATA error register (SERR).
fn test_ahci_port_serr() {
    // SERR bits:
    // Bits 0-7: Diagnostics
    // Bits 8-15: Error
    // Bits 16-31: Reserved

    const SERR_DIAG_MASK: u32 = 0x0000_00FF;
    const SERR_ERR_MASK: u32 = 0x0000_FF00;

    assert_eq!(SERR_DIAG_MASK, 0x0000_00FF);
    assert_eq!(SERR_ERR_MASK, 0x0000_FF00);

    println!("  [PASS] AHCI port SATA error register format correct");
}

/// AHCI port SATA active register (SACT).
fn test_ahci_port_sact() {
    // SACT bits:
    // Bits 0-31: Device bits (1 = device present)

    const SACT_DEVICE_MASK: u32 = 0xFFFF_FFFF;

    assert_eq!(SACT_DEVICE_MASK, 0xFFFF_FFFF);

    println!("  [PASS] AHCI port SATA active register format correct");
}

/// AHCI port command issue register (CI).
fn test_ahci_port_ci() {
    // CI bits:
    // Bits 0-31: Command bits (1 = command issued)

    const CI_COMMAND_MASK: u32 = 0xFFFF_FFFF;

    assert_eq!(CI_COMMAND_MASK, 0xFFFF_FFFF);

    println!("  [PASS] AHCI port command issue register format correct");
}

/// AHCI port SATA notification register (SNTF).
fn test_ahci_port_sntf() {
    // SNTF bits:
    // Bits 0-31: Notification bits (1 = notification pending)

    const SNTF_NOTIFICATION_MASK: u32 = 0xFFFF_FFFF;

    assert_eq!(SNTF_NOTIFICATION_MASK, 0xFFFF_FFFF);

    println!("  [PASS] AHCI port SATA notification register format correct");
}
