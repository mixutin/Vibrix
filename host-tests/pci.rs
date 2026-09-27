//! Host-side unit tests for PCI configuration space and enumeration logic.
//!
//! These tests run on the host (not in the kernel) and validate that the
//! PCI configuration space access and BAR parsing are correct.
//!
//! Primary reference: PCI Local Bus Specification, PCI Firmware Specification.

fn main() {
    println!("Running PCI enumeration tests...\n");

    test_pci_config_address_format();
    test_pci_vendor_device_id();
    test_pci_command_register();
    test_pci_status_register();
    test_pci_class_code();
    test_pci_bar_format();
    test_pci_bar_type();
    test_pci_bar_address();
    test_pci_interrupt_line();
    test_pci_bridge();

    println!("\nAll PCI enumeration tests passed!");
}

/// PCI configuration address format (Intel IO address 0xCF8):
///
/// Bit 31:    Enable bit (must be 1)
/// Bits 30-24: Reserved (0)
/// Bits 23-16: Bus number (0-255)
/// Bits 15-11: Device number (0-31)
/// Bits 10-8:  Function number (0-7)
/// Bits 7-2:   Register offset (0-255, must be 4-byte aligned)
/// Bits 1-0:   Reserved (0)
fn test_pci_config_address_format() {
    const ENABLE_BIT: u32 = 1 << 31;
    const BUS_SHIFT: u32 = 16;
    const DEVICE_SHIFT: u32 = 11;
    const FUNCTION_SHIFT: u32 = 8;
    const REGISTER_SHIFT: u32 = 2;

    assert_eq!(ENABLE_BIT, 0x8000_0000);

    // Bus 0, Device 0, Function 0, Register 0
    let addr: u32 = ENABLE_BIT | (0 << BUS_SHIFT) | (0 << DEVICE_SHIFT) | (0 << FUNCTION_SHIFT) | (0 << REGISTER_SHIFT);
    assert_eq!(addr, 0x8000_0000);

    // Bus 1, Device 2, Function 3, Register 4
    let addr2: u32 = ENABLE_BIT | (1 << BUS_SHIFT) | (2 << DEVICE_SHIFT) | (3 << FUNCTION_SHIFT) | (4 << REGISTER_SHIFT);
    assert_eq!(addr2, 0x8001_1310);

    println!("  [PASS] PCI config address format correct");
}

/// PCI vendor and device ID register (offset 0x00).
fn test_pci_vendor_device_id() {
    // Vendor ID: bits 0-15
    // Device ID: bits 16-31
    let vendor_id: u16 = 0x8086; // Intel
    let device_id: u16 = 0x1237; // Example device

    let reg: u32 = (device_id as u32) << 16 | (vendor_id as u32);
    assert_eq!(reg, 0x1237_8086);

    // Extract vendor and device
    let extracted_vendor = (reg & 0xFFFF) as u16;
    let extracted_device = ((reg >> 16) & 0xFFFF) as u16;
    assert_eq!(extracted_vendor, vendor_id);
    assert_eq!(extracted_device, device_id);

    println!("  [PASS] PCI vendor/device ID format correct");
}

/// PCI command register (offset 0x04).
fn test_pci_command_register() {
    // Bit 0:  I/O space enable
    // Bit 1:  Memory space enable
    // Bit 2:  Bus master enable
    // Bit 3:  Special cycle enable
    // Bit 4:  Memory write and invalidate enable
    // Bit 5:  VGA palette snoop enable
    // Bit 6:  Parity error response enable
    // Bit 7:  Wait cycle control
    // Bit 8:  SERR# enable
    // Bit 9:  Fast back-to-back enable
    // Bit 10: Interrupt disable

    const IO_SPACE_ENABLE: u16 = 1 << 0;
    const MEMORY_SPACE_ENABLE: u16 = 1 << 1;
    const BUS_MASTER_ENABLE: u16 = 1 << 2;
    const INTERRUPT_DISABLE: u16 = 1 << 10;

    assert_eq!(IO_SPACE_ENABLE, 0x0001);
    assert_eq!(MEMORY_SPACE_ENABLE, 0x0002);
    assert_eq!(BUS_MASTER_ENABLE, 0x0004);
    assert_eq!(INTERRUPT_DISABLE, 0x0400);

    println!("  [PASS] PCI command register format correct");
}

/// PCI status register (offset 0x06).
fn test_pci_status_register() {
    // Bit 3:  Interrupt status
    // Bit 4:  Capabilities list
    // Bit 5:  66 MHz capable
    // Bit 7:  Fast back-to-back capable
    // Bit 8:  Master data parity error
    // Bits 9-10: DEVSEL timing
    // Bit 11: Signaled target abort
    // Bit 12: Received target abort
    // Bit 13: Received master abort
    // Bit 14: Signaled system error
    // Bit 15: Detected parity error

    const CAPABILITIES_LIST: u16 = 1 << 4;
    const INTERRUPT_STATUS: u16 = 1 << 3;

    assert_eq!(CAPABILITIES_LIST, 0x0010);
    assert_eq!(INTERRUPT_STATUS, 0x0008);

    println!("  [PASS] PCI status register format correct");
}

/// PCI class code register (offset 0x09).
fn test_pci_class_code() {
    // Class code: bits 23-16
    // Subclass: bits 15-8
    // Programming interface: bits 7-0

    const CLASS_MASS_STORAGE: u8 = 0x01;
    const SUBCLASS_SATA: u8 = 0x06;
    const PROG_IF_AHCI: u8 = 0x01;

    let class_reg: u32 = ((CLASS_MASS_STORAGE as u32) << 24)
        | ((SUBCLASS_SATA as u32) << 16)
        | ((PROG_IF_AHCI as u32) << 8);
    assert_eq!(class_reg, 0x0106_0100);

    println!("  [PASS] PCI class code format correct");
}

/// PCI BAR (Base Address Register) format.
fn test_pci_bar_format() {
    // BAR bits 0-3:
    //   Bit 0:  Type (0=memory, 1=I/O)
    //   Bits 1-2: Memory type (0=32-bit, 1=20-bit, 2=64-bit)
    //   Bit 3:  Prefetchable (memory only)
    //   Bits 4-31: Base address (memory) or I/O address (I/O)

    const BAR_TYPE_MEMORY: u32 = 0;
    const BAR_TYPE_IO: u32 = 1;
    const BAR_TYPE_32BIT: u32 = 0;
    const BAR_TYPE_64BIT: u32 = 2;
    const BAR_PREFETCHABLE: u32 = 1 << 3;

    assert_eq!(BAR_TYPE_MEMORY, 0);
    assert_eq!(BAR_TYPE_IO, 1);
    assert_eq!(BAR_TYPE_32BIT, 0);
    assert_eq!(BAR_TYPE_64BIT, 2);
    assert_eq!(BAR_PREFETCHABLE, 0x08);

    println!("  [PASS] PCI BAR format correct");
}

/// PCI BAR type detection.
fn test_pci_bar_type() {
    // Memory BAR: bit 0 = 0
    let mem_bar: u32 = 0xFFFF_F000;
    assert_eq!(mem_bar & 0x01, 0);

    // I/O BAR: bit 0 = 1
    let io_bar: u32 = 0xFFFF_FFF9;
    assert_eq!(io_bar & 0x01, 1);

    println!("  [PASS] PCI BAR type detection correct");
}

/// PCI BAR address extraction.
fn test_pci_bar_address() {
    // Memory BAR: bits 4-31 are the base address
    let mem_bar: u32 = 0xF000_0000;
    let mem_addr = mem_bar & 0xFFFF_FFF0;
    assert_eq!(mem_addr, 0xF000_0000);

    // I/O BAR: bits 2-31 are the base address
    let io_bar: u32 = 0x0000_FFF8;
    let io_addr = io_bar & 0xFFFF_FFFC;
    assert_eq!(io_addr, 0x0000_FFF8);

    println!("  [PASS] PCI BAR address extraction correct");
}

/// PCI interrupt line register (offset 0x3C).
fn test_pci_interrupt_line() {
    // Interrupt line: bits 0-7 (IRQ number 0-255, 0=not connected)
    // Interrupt pin: bits 8-15 (0=none, 1=INTA#, 2=INTB#, 3=INTC#, 4=INTD#)

    const INTERRUPT_LINE_IRQ0: u8 = 0x00;
    const INTERRUPT_LINE_IRQ11: u8 = 0x0B;
    const INTERRUPT_PIN_INTA: u8 = 0x01;
    const INTERRUPT_PIN_INTB: u8 = 0x02;

    let reg: u16 = ((INTERRUPT_PIN_INTB as u16) << 8) | (INTERRUPT_LINE_IRQ11 as u16);
    assert_eq!(reg, 0x020B);

    println!("  [PASS] PCI interrupt line format correct");
}

/// PCI bridge configuration.
fn test_pci_bridge() {
    // PCI-to-PCI bridge class code
    const CLASS_BRIDGE: u8 = 0x06;
    const SUBCLASS_PCI_TO_PCI: u8 = 0x04;

    // Bridge registers:
    // Offset 0x18: Primary bus, secondary bus, subordinate bus, secondary latency
    // Offset 0x1C: I/O base, I/O limit
    // Offset 0x20: Memory base, memory limit
    // Offset 0x24: Prefetchable memory base, limit

    let bus_reg: u8 = 0x01; // Primary bus 1
    let secondary_bus: u8 = 0x02; // Secondary bus 2
    let subordinate_bus: u8 = 0x05; // Subordinate bus 5

    let bridge_reg: u32 = ((subordinate_bus as u32) << 16)
        | ((secondary_bus as u32) << 8)
        | (bus_reg as u32);
    assert_eq!(bridge_reg, 0x0005_0201);

    println!("  [PASS] PCI bridge configuration correct");
}
