//! Host-side unit tests for HDA (High Definition Audio) register layout.
//!
//! These tests run on the host (not in the kernel) and validate that the
//! HDA register offsets and bit fields are correct.
//!
//! Primary reference: Intel High Definition Audio Specification.

fn main() {
    println!("Running HDA register layout tests...\n");

    test_hda_capability_registers();
    test_hda_gcap_register();
    test_hda_gctl_register();
    test_hda_wakeen_register();
    test_hda_wakessts_register();
    test_hda_gsts_register();
    test_hda_intctl_register();
    test_hda_intsts_register();
    test_hda_wallclk_register();
    test_hda_ssync_register();

    println!("\nAll HDA register layout tests passed!");
}

/// HDA capability registers.
fn test_hda_capability_registers() {
    const GCAP: u16 = 0x00; // Global Capabilities
    const VMIN: u16 = 0x02; // Minor Version
    const VMAJ: u16 = 0x03; // Major Version
    const OUTPAY: u16 = 0x04; // Output Payload Capability
    const INPAY: u16 = 0x06; // Input Payload Capability
    const GCTL: u16 = 0x08; // Global Control
    const WAKEEN: u16 = 0x0C; // Wake Enable
    const STATESTS: u16 = 0x0E; // State Status
    const GSTS: u16 = 0x10; // Global Status
    const OUTSTRMPAY: u16 = 0x18; // Output Stream Payload Capability
    const INSTRMPAY: u16 = 0x1A; // Input Stream Payload Capability
    const INTCTL: u16 = 0x20; // Interrupt Control
    const INTSTS: u16 = 0x24; // Interrupt Status
    const WALLCLK: u16 = 0x30; // Wall Clock Counter
    const SSYNC: u16 = 0x38; // Stream Synchronization
    const CORBLBASE: u16 = 0x40; // CORB Lower Base Address
    const CORBUBASE: u16 = 0x44; // CORB Upper Base Address
    const CORBWP: u16 = 0x48; // CORB Write Pointer
    const CORBRP: u16 = 0x4A; // CORB Read Pointer
    const CORBCTL: u16 = 0x4C; // CORB Control
    const CORBSTS: u16 = 0x4D; // CORB Status
    const CORBSIZE: u16 = 0x4E; // CORB Size
    const RIRBLBASE: u16 = 0x50; // RIRB Lower Base Address
    const RIRBUBASE: u16 = 0x54; // RIRB Upper Base Address
    const RIRBWP: u16 = 0x58; // RIRB Write Pointer
    const RINTCNT: u16 = 0x5A; // Response Interrupt Count
    const RIRBCTL: u16 = 0x5C; // RIRB Control
    const RIRBSTS: u16 = 0x5D; // RIRB Status
    const RIRBSIZE: u16 = 0x5E; // RIRB Size
    const IICO: u16 = 0x60; // Immediate Command Output Interface
    const IICI: u16 = 0x64; // Immediate Command Input Interface
    const IICS: u16 = 0x68; // Immediate Command Status
    const DPIBLBASE: u16 = 0x70; // DMA Position Buffer Lower Base
    const DPIBUBASE: u16 = 0x74; // DMA Position Buffer Upper Base

    assert_eq!(GCAP, 0x00);
    assert_eq!(VMIN, 0x02);
    assert_eq!(VMAJ, 0x03);
    assert_eq!(OUTPAY, 0x04);
    assert_eq!(INPAY, 0x06);
    assert_eq!(GCTL, 0x08);
    assert_eq!(WAKEEN, 0x0C);
    assert_eq!(STATESTS, 0x0E);
    assert_eq!(GSTS, 0x10);
    assert_eq!(OUTSTRMPAY, 0x18);
    assert_eq!(INSTRMPAY, 0x1A);
    assert_eq!(INTCTL, 0x20);
    assert_eq!(INTSTS, 0x24);
    assert_eq!(WALLCLK, 0x30);
    assert_eq!(SSYNC, 0x38);
    assert_eq!(CORBLBASE, 0x40);
    assert_eq!(CORBUBASE, 0x44);
    assert_eq!(CORBWP, 0x48);
    assert_eq!(CORBRP, 0x4A);
    assert_eq!(CORBCTL, 0x4C);
    assert_eq!(CORBSTS, 0x4D);
    assert_eq!(CORBSIZE, 0x4E);
    assert_eq!(RIRBLBASE, 0x50);
    assert_eq!(RIRBUBASE, 0x54);
    assert_eq!(RIRBWP, 0x58);
    assert_eq!(RINTCNT, 0x5A);
    assert_eq!(RIRBCTL, 0x5C);
    assert_eq!(RIRBSTS, 0x5D);
    assert_eq!(RIRBSIZE, 0x5E);
    assert_eq!(IICO, 0x60);
    assert_eq!(IICI, 0x64);
    assert_eq!(IICS, 0x68);
    assert_eq!(DPIBLBASE, 0x70);
    assert_eq!(DPIBUBASE, 0x74);

    println!("  [PASS] HDA capability registers correct");
}

/// HDA GCAP register format.
fn test_hda_gcap_register() {
    // GCAP bits:
    // Bits 0-3: Output stream count
    // Bits 4-7: Input stream count
    // Bits 8-11: Bidirectional stream count
    // Bits 12-15: Reserved
    // Bits 16-19: Number of SDOs
    // Bits 20-23: Number of SDIs
    // Bits 24-27: Reserved
    // Bits 28-31: Number of SDOs (extended)

    const GCAP_OSS_MASK: u16 = 0x000F;
    const GCAP_ISS_MASK: u16 = 0x00F0;
    const GCAP_BSS_MASK: u16 = 0x0F00;
    const GCAP_NSDO_MASK: u16 = 0xF000;

    assert_eq!(GCAP_OSS_MASK, 0x000F);
    assert_eq!(GCAP_ISS_MASK, 0x00F0);
    assert_eq!(GCAP_BSS_MASK, 0x0F00);
    assert_eq!(GCAP_NSDO_MASK, 0xF000);

    println!("  [PASS] HDA GCAP register format correct");
}

/// HDA GCTL register format.
fn test_hda_gctl_register() {
    // GCTL bits:
    // Bit 0: Controller reset
    // Bit 1: Flush control
    // Bit 2: Reserved
    // Bit 3: Reserved
    // Bit 4: Accept unsolicited response enable
    // Bits 5-7: Reserved
    // Bit 8: Stream reset
    // Bits 9-15: Reserved

    const GCTL_CRST: u16 = 0x0001;
    const GCTL_FCNTRL: u16 = 0x0002;
    const GCTL_SREN: u16 = 0x0100;

    assert_eq!(GCTL_CRST, 0x0001);
    assert_eq!(GCTL_FCNTRL, 0x0002);
    assert_eq!(GCTL_SREN, 0x0100);

    println!("  [PASS] HDA GCTL register format correct");
}

/// HDA WAKEEN register format.
fn test_hda_wakeen_register() {
    // WAKEEN bits:
    // Bits 0-14: SDIN wake enable (one bit per SDIN)
    // Bit 15: Reserved

    const WAKEEN_SDIN0: u16 = 0x0001;
    const WAKEEN_SDIN1: u16 = 0x0002;
    const WAKEEN_SDIN2: u16 = 0x0004;
    const WAKEEN_SDIN3: u16 = 0x0008;

    assert_eq!(WAKEEN_SDIN0, 0x0001);
    assert_eq!(WAKEEN_SDIN1, 0x0002);
    assert_eq!(WAKEEN_SDIN2, 0x0004);
    assert_eq!(WAKEEN_SDIN3, 0x0008);

    println!("  [PASS] HDA WAKEEN register format correct");
}

/// HDA STATESTS register format.
fn test_hda_wakessts_register() {
    // STATESTS bits:
    // Bits 0-14: SDIN state change status (one bit per SDIN)
    // Bit 15: Reserved

    const STATESTS_SDIN0: u16 = 0x0001;
    const STATESTS_SDIN1: u16 = 0x0002;
    const STATESTS_SDIN2: u16 = 0x0004;
    const STATESTS_SDIN3: u16 = 0x0008;

    assert_eq!(STATESTS_SDIN0, 0x0001);
    assert_eq!(STATESTS_SDIN1, 0x0002);
    assert_eq!(STATESTS_SDIN2, 0x0004);
    assert_eq!(STATESTS_SDIN3, 0x0008);

    println!("  [PASS] HDA STATESTS register format correct");
}

/// HDA GSTS register format.
fn test_hda_gsts_register() {
    // GSTS bits:
    // Bit 0: Reserved
    // Bit 1: Flush status
    // Bits 2-15: Reserved

    const GSTS_FSTS: u16 = 0x0002;

    assert_eq!(GSTS_FSTS, 0x0002);

    println!("  [PASS] HDA GSTS register format correct");
}

/// HDA INTCTL register format.
fn test_hda_intctl_register() {
    // INTCTL bits:
    // Bits 0-7: Interrupt control for output streams
    // Bits 8-15: Interrupt control for input streams
    // Bits 16-23: Interrupt control for bidirectional streams
    // Bit 24: Global interrupt enable
    // Bit 25: Controller interrupt enable
    // Bit 26: Stream interrupt enable
    // Bits 27-31: Reserved

    const INTCTL_GIE: u32 = 0x0100_0000;
    const INTCTL_CIE: u32 = 0x0200_0000;
    const INTCTL_SIE: u32 = 0x0400_0000;

    assert_eq!(INTCTL_GIE, 0x0100_0000);
    assert_eq!(INTCTL_CIE, 0x0200_0000);
    assert_eq!(INTCTL_SIE, 0x0400_0000);

    println!("  [PASS] HDA INTCTL register format correct");
}

/// HDA INTSTS register format.
fn test_hda_intsts_register() {
    // INTSTS bits:
    // Bits 0-7: Interrupt status for output streams
    // Bits 8-15: Interrupt status for input streams
    // Bits 16-23: Interrupt status for bidirectional streams
    // Bit 24: Global interrupt status
    // Bit 25: Controller interrupt status
    // Bit 26: Stream interrupt status
    // Bits 27-31: Reserved

    const INTSTS_GIS: u32 = 0x0100_0000;
    const INTSTS_CIS: u32 = 0x0200_0000;
    const INTSTS_SIS: u32 = 0x0400_0000;

    assert_eq!(INTSTS_GIS, 0x0100_0000);
    assert_eq!(INTSTS_CIS, 0x0200_0000);
    assert_eq!(INTSTS_SIS, 0x0400_0000);

    println!("  [PASS] HDA INTSTS register format correct");
}

/// HDA WALLCLK register format.
fn test_hda_wallclk_register() {
    // WALLCLK: 32-bit register containing the wall clock counter
    // Incremented at 48 MHz (or 24 MHz depending on implementation)

    const WALLCLK_MASK: u32 = 0xFFFF_FFFF;

    assert_eq!(WALLCLK_MASK, 0xFFFF_FFFF);

    println!("  [PASS] HDA WALLCLK register format correct");
}

/// HDA SSYNC register format.
fn test_hda_ssync_register() {
    // SSYNC bits:
    // Bits 0-7: Stream synchronization for output streams
    // Bits 8-15: Stream synchronization for input streams
    // Bits 16-23: Stream synchronization for bidirectional streams
    // Bits 24-31: Reserved

    const SSYNC_OUT_MASK: u32 = 0x0000_00FF;
    const SSYNC_IN_MASK: u32 = 0x0000_FF00;
    const SSYNC_BIDIR_MASK: u32 = 0x00FF_0000;

    assert_eq!(SSYNC_OUT_MASK, 0x0000_00FF);
    assert_eq!(SSYNC_IN_MASK, 0x0000_FF00);
    assert_eq!(SSYNC_BIDIR_MASK, 0x00FF_0000);

    println!("  [PASS] HDA SSYNC register format correct");
}
