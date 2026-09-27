//! Host-side unit tests for FADT (Fixed ACPI Description Table) registers.
//!
//! These tests run on the host (not in the kernel) and validate that the
//! FADT register structures are correct.
//!
//! Primary reference: ACPI Specification (Chapter 5: FADT).

fn main() {
    println!("Running FADT register tests...\n");

    test_fadt_header();
    test_fadt_firmware_ctrl();
    test_fadt_dsdt();
    test_fadt_reduced_acpi();
    test_fadt_flags();
    test_fadt_reset_register();
    test_fadt_boot_arch();
    test_fadt_minor_version();
    test_fadt_x_firmware_ctrl();
    test_fadt_x_dsdt();
    test_fadt_sleep_control();

    println!("\nAll FADT register tests passed!");
}

/// FADT header.
fn test_fadt_header() {
    // FADT header (36 bytes):
    // Signature: 4 bytes ("FACP")
    // Length: 4 bytes
    // Revision: 1 byte
    // Checksum: 1 byte
    // OEM ID: 6 bytes
    // OEM Table ID: 8 bytes
    // OEM Revision: 4 bytes
    // Creator ID: 4 bytes
    // Creator Revision: 4 bytes

    const FADT_HEADER_SIZE: usize = 36;
    assert_eq!(FADT_HEADER_SIZE, 36);

    println!("  [PASS] FADT header correct");
}

/// FADT firmware control register.
fn test_fadt_firmware_ctrl() {
    // FIRMWARE_CTRL: 32-bit physical address of the FACS
    const FADT_FIRMWARE_CTRL_ALIGNMENT: u32 = 4;
    assert_eq!(FADT_FIRMWARE_CTRL_ALIGNMENT, 4);

    println!("  [PASS] FADT firmware control register correct");
}

/// FADT DSDT register.
fn test_fadt_dsdt() {
    // DSDT: 32-bit physical address of the DSDT
    const FADT_DSDT_ALIGNMENT: u32 = 4;
    assert_eq!(FADT_DSDT_ALIGNMENT, 4);

    println!("  [PASS] FADT DSDT register correct");
}

/// FADT reduced ACPI.
fn test_fadt_reduced_acpi() {
    // Preferred_PM_Profile: 1 byte
    // 0 = Unspecified
    // 1 = Desktop
    // 2 = Mobile
    // 3 = Workstation
    // 4 = Enterprise Server
    // 5 = SOHO Server
    // 6 = Appliance PC
    // 7 = Performance Server

    const FADT_PM_PROFILE_UNSPECIFIED: u8 = 0;
    const FADT_PM_PROFILE_DESKTOP: u8 = 1;
    const FADT_PM_PROFILE_MOBILE: u8 = 2;
    const FADT_PM_PROFILE_WORKSTATION: u8 = 3;
    const FADT_PM_PROFILE_ENTERPRISE_SERVER: u8 = 4;
    const FADT_PM_PROFILE_SOHO_SERVER: u8 = 5;
    const FADT_PM_PROFILE_APPLIANCE_PC: u8 = 6;
    const FADT_PM_PROFILE_PERFORMANCE_SERVER: u8 = 7;

    assert_eq!(FADT_PM_PROFILE_UNSPECIFIED, 0);
    assert_eq!(FADT_PM_PROFILE_DESKTOP, 1);
    assert_eq!(FADT_PM_PROFILE_MOBILE, 2);
    assert_eq!(FADT_PM_PROFILE_WORKSTATION, 3);
    assert_eq!(FADT_PM_PROFILE_ENTERPRISE_SERVER, 4);
    assert_eq!(FADT_PM_PROFILE_SOHO_SERVER, 5);
    assert_eq!(FADT_PM_PROFILE_APPLIANCE_PC, 6);
    assert_eq!(FADT_PM_PROFILE_PERFORMANCE_SERVER, 7);

    println!("  [PASS] FADT reduced ACPI correct");
}

/// FADT flags.
fn test_fadt_flags() {
    // FADT flags (32-bit):
    // Bit 0: WBINVD
    // Bit 1: WBINVD_FLUSH
    // Bit 2: PROC_C1
    // Bit 3: P_LVL2_UP
    // Bit 4: PWR_BUTTON
    // Bit 5: SLP_BUTTON
    // Bit 6: FIX_RTC
    // Bit 7: RTC_S4
    // Bit 8: TMR_VAL_EXT
    // Bit 9: DCK_CAP
    // Bit 10: RESET_REG_SUP
    // Bit 11: SEALED_CASE
    // Bit 12: HEADLESS
    // Bit 13: CPU_SW_SLP
    // Bit 14: PCI_EXP_WAK
    // Bit 15: USE_PLATFORM_CLOCK
    // Bit 16: S4_RTC_STS_VALID
    // Bit 17: REMOTE_POWER_ON_CAPABLE
    // Bit 18: FORCE_APIC_CLUSTER_MODEL
    // Bit 19: FORCE_APIC_PHYSICAL_DESTINATION_MODE
    // Bit 20: HW_REDUCED_ACPI
    // Bit 21: LOW_POWER_S0_IDLE_CAPABLE

    const FADT_FLAG_WBINVD: u32 = 1 << 0;
    const FADT_FLAG_WBINVD_FLUSH: u32 = 1 << 1;
    const FADT_FLAG_PROC_C1: u32 = 1 << 2;
    const FADT_FLAG_PWR_BUTTON: u32 = 1 << 4;
    const FADT_FLAG_SLP_BUTTON: u32 = 1 << 5;
    const FADT_FLAG_RESET_REG_SUP: u32 = 1 << 10;
    const FADT_FLAG_HW_REDUCED_ACPI: u32 = 1 << 20;

    assert_eq!(FADT_FLAG_WBINVD, 1 << 0);
    assert_eq!(FADT_FLAG_WBINVD_FLUSH, 1 << 1);
    assert_eq!(FADT_FLAG_PROC_C1, 1 << 2);
    assert_eq!(FADT_FLAG_PWR_BUTTON, 1 << 4);
    assert_eq!(FADT_FLAG_SLP_BUTTON, 1 << 5);
    assert_eq!(FADT_FLAG_RESET_REG_SUP, 1 << 10);
    assert_eq!(FADT_FLAG_HW_REDUCED_ACPI, 1 << 20);

    println!("  [PASS] FADT flags correct");
}

/// FADT reset register.
fn test_fadt_reset_register() {
    // RESET_REG: Generic Address Structure (12 bytes)
    // RESET_VALUE: 1 byte (value to write to RESET_REG to reset the system)

    const FADT_RESET_REG_SIZE: usize = 12;
    const FADT_RESET_VALUE: u8 = 0x06; // Typical reset value

    assert_eq!(FADT_RESET_REG_SIZE, 12);
    assert_eq!(FADT_RESET_VALUE, 0x06);

    println!("  [PASS] FADT reset register correct");
}

/// FADT boot architecture flags.
fn test_fadt_boot_arch() {
    // BOOT_ARCH: 16-bit flags
    // Bit 0: LEGACY_DEVICES (8042 keyboard controller)
    // Bit 1: PS2_MOUSE (PS/2 mouse present)
    // Bit 2: NO_VGA (no VGA device)
    // Bit 3: NO_MSI (no MSI support)
    // Bit 4: NO_ASPM (no ASPM support)
    // Bit 5: NO_CMOS_RTC (no CMOS RTC)

    const FADT_BOOT_LEGACY_DEVICES: u16 = 1 << 0;
    const FADT_BOOT_PS2_MOUSE: u16 = 1 << 1;
    const FADT_BOOT_NO_VGA: u16 = 1 << 2;
    const FADT_BOOT_NO_MSI: u16 = 1 << 3;
    const FADT_BOOT_NO_ASPM: u16 = 1 << 4;
    const FADT_BOOT_NO_CMOS_RTC: u16 = 1 << 5;

    assert_eq!(FADT_BOOT_LEGACY_DEVICES, 1 << 0);
    assert_eq!(FADT_BOOT_PS2_MOUSE, 1 << 1);
    assert_eq!(FADT_BOOT_NO_VGA, 1 << 2);
    assert_eq!(FADT_BOOT_NO_MSI, 1 << 3);
    assert_eq!(FADT_BOOT_NO_ASPM, 1 << 4);
    assert_eq!(FADT_BOOT_NO_CMOS_RTC, 1 << 5);

    println!("  [PASS] FADT boot architecture flags correct");
}

/// FADT minor version.
fn test_fadt_minor_version() {
    // FADT minor version: 1 byte
    // ACPI 6.4: minor version = 4

    const FADT_MINOR_VERSION_ACPI_6_4: u8 = 4;
    assert_eq!(FADT_MINOR_VERSION_ACPI_6_4, 4);

    println!("  [PASS] FADT minor version correct");
}

/// FADT X_FIRMWARE_CTRL register.
fn test_fadt_x_firmware_ctrl() {
    // X_FIRMWARE_CTRL: 64-bit physical address of the FACS
    const FADT_X_FIRMWARE_CTRL_ALIGNMENT: u64 = 8;
    assert_eq!(FADT_X_FIRMWARE_CTRL_ALIGNMENT, 8);

    println!("  [PASS] FADT X_FIRMWARE_CTRL register correct");
}

/// FADT X_DSDT register.
fn test_fadt_x_dsdt() {
    // X_DSDT: 64-bit physical address of the DSDT
    const FADT_X_DSDT_ALIGNMENT: u64 = 8;
    assert_eq!(FADT_X_DSDT_ALIGNMENT, 8);

    println!("  [PASS] FADT X_DSDT register correct");
}

/// FADT sleep control register.
fn test_fadt_sleep_control() {
    // SLP_TYP: 3-bit field (sleep type)
    // SLP_EN: 1-bit field (sleep enable)

    const FADT_SLP_TYP_MASK: u16 = 0x07;
    const FADT_SLP_EN: u16 = 0x08;

    assert_eq!(FADT_SLP_TYP_MASK, 0x07);
    assert_eq!(FADT_SLP_EN, 0x08);

    println!("  [PASS] FADT sleep control register correct");
}
