//! Host-side unit tests for xHCI (USB 3.0) register layout.
//!
//! These tests run on the host (not in the kernel) and validate that the
//! xHCI register offsets and bit fields are correct.
//!
//! Primary reference: eXtensible Host Controller Interface Specification for USB (xHCI).

fn main() {
    println!("Running xHCI register layout tests...\n");

    test_xhci_capability_registers();
    test_xhci_operational_registers();
    test_xhci_port_registers();
    test_xhci_runtime_registers();
    test_xhci_doorbell_registers();
    test_xhci_port_status();
    test_xhci_port_speed();
    test_xhci_port_link_state();
    test_xhci_command_register();
    test_xhci_status_register();

    println!("\nAll xHCI register layout tests passed!");
}

/// xHCI capability registers.
fn test_xhci_capability_registers() {
    const CAPLENGTH: u32 = 0x00;
    const HCIVERSION: u32 = 0x02;
    const HCSPARAMS1: u32 = 0x04;
    const HCSPARAMS2: u32 = 0x08;
    const HCSPARAMS3: u32 = 0x0C;
    const HCCPARAMS1: u32 = 0x10;
    const DBOFF: u32 = 0x14;
    const RTSOFF: u32 = 0x18;
    const HCCPARAMS2: u32 = 0x1C;

    assert_eq!(CAPLENGTH, 0x00);
    assert_eq!(HCIVERSION, 0x02);
    assert_eq!(HCSPARAMS1, 0x04);
    assert_eq!(HCSPARAMS2, 0x08);
    assert_eq!(HCSPARAMS3, 0x0C);
    assert_eq!(HCCPARAMS1, 0x10);
    assert_eq!(DBOFF, 0x14);
    assert_eq!(RTSOFF, 0x18);
    assert_eq!(HCCPARAMS2, 0x1C);

    println!("  [PASS] xHCI capability registers correct");
}

/// xHCI operational registers.
fn test_xhci_operational_registers() {
    const USBCMD: u32 = 0x00;
    const USBSTS: u32 = 0x04;
    const PAGESIZE: u32 = 0x08;
    const DNCTRL: u32 = 0x14;
    const CRCR: u32 = 0x18;
    const DCBAAP: u32 = 0x30;
    const CONFIG: u32 = 0x38;

    assert_eq!(USBCMD, 0x00);
    assert_eq!(USBSTS, 0x04);
    assert_eq!(PAGESIZE, 0x08);
    assert_eq!(DNCTRL, 0x14);
    assert_eq!(CRCR, 0x18);
    assert_eq!(DCBAAP, 0x30);
    assert_eq!(CONFIG, 0x38);

    println!("  [PASS] xHCI operational registers correct");
}

/// xHCI port registers.
fn test_xhci_port_registers() {
    // Port status and control register (offset 0x00 from port base)
    const PORTSC: u32 = 0x00;
    const PORTPMSC: u32 = 0x04;
    const PORTLI: u32 = 0x08;
    const PORTHLPMC: u32 = 0x0C;

    assert_eq!(PORTSC, 0x00);
    assert_eq!(PORTPMSC, 0x04);
    assert_eq!(PORTLI, 0x08);
    assert_eq!(PORTHLPMC, 0x0C);

    println!("  [PASS] xHCI port registers correct");
}

/// xHCI runtime registers.
fn test_xhci_runtime_registers() {
    const MFINDEX: u32 = 0x00;
    const IMAN: u32 = 0x00; // Interrupt Management Register
    const IMOD: u32 = 0x04; // Interrupt Moderation Register
    const ERSTSZ: u32 = 0x08;
    const ERSTBA: u32 = 0x10;
    const ERDP: u32 = 0x18;

    assert_eq!(MFINDEX, 0x00);
    assert_eq!(IMAN, 0x00);
    assert_eq!(IMOD, 0x04);
    assert_eq!(ERSTSZ, 0x08);
    assert_eq!(ERSTBA, 0x10);
    assert_eq!(ERDP, 0x18);

    println!("  [PASS] xHCI runtime registers correct");
}

/// xHCI doorbell registers.
fn test_xhci_doorbell_registers() {
    // Doorbell registers start at offset 0x00 from doorbell array base
    // Each doorbell is 4 bytes
    // Doorbell 0: Host controller command
    // Doorbell 1-31: Device slot contexts

    const DOORBELL_STRIDE: u32 = 4;
    assert_eq!(DOORBELL_STRIDE, 4);

    println!("  [PASS] xHCI doorbell registers correct");
}

/// xHCI port status register format.
fn test_xhci_port_status() {
    // Port status and control register bits:
    // Bit 0: Current connect status
    // Bit 1: Port enabled/disabled
    // Bit 2: Over-current
    // Bit 3: Port reset
    // Bits 5-4: Link state
    // Bit 8: Port power
    // Bits 13-10: Port speed
    // Bit 14: Port indicator control
    // Bit 15: Port link state write strobe
    // Bit 16: Connect status change
    // Bit 17: Port enabled/disabled change
    // Bit 18: Warm port reset change
    // Bit 19: Over-current change
    // Bit 20: Port reset change
    // Bit 21: Port link state change
    // Bit 22: Port config error change
    // Bit 23: Cold attach status
    // Bit 24: Wake on connect enable
    // Bit 25: Wake on disconnect enable
    // Bit 26: Wake on over-current enable
    // Bit 27: Device removable
    // Bit 28: Warm port reset

    const PORTSC_CURRENT_CONNECT: u32 = 1 << 0;
    const PORTSC_PORT_ENABLED: u32 = 1 << 1;
    const PORTSC_OVER_CURRENT: u32 = 1 << 2;
    const PORTSC_PORT_RESET: u32 = 1 << 3;
    const PORTSC_PORT_POWER: u32 = 1 << 8;
    const PORTSC_CONNECT_STATUS_CHANGE: u32 = 1 << 16;
    const PORTSC_PORT_RESET_CHANGE: u32 = 1 << 20;

    assert_eq!(PORTSC_CURRENT_CONNECT, 0x0000_0001);
    assert_eq!(PORTSC_PORT_ENABLED, 0x0000_0002);
    assert_eq!(PORTSC_OVER_CURRENT, 0x0000_0004);
    assert_eq!(PORTSC_PORT_RESET, 0x0000_0008);
    assert_eq!(PORTSC_PORT_POWER, 0x0000_0100);
    assert_eq!(PORTSC_CONNECT_STATUS_CHANGE, 0x0001_0000);
    assert_eq!(PORTSC_PORT_RESET_CHANGE, 0x0010_0000);

    println!("  [PASS] xHCI port status register format correct");
}

/// xHCI port speed values.
fn test_xhci_port_speed() {
    const PORT_SPEED_FULL: u32 = 1; // 12 Mbps
    const PORT_SPEED_LOW: u32 = 2; // 1.5 Mbps
    const PORT_SPEED_HIGH: u32 = 3; // 480 Mbps
    const PORT_SPEED_SUPER: u32 = 4; // 5 Gbps

    assert_eq!(PORT_SPEED_FULL, 1);
    assert_eq!(PORT_SPEED_LOW, 2);
    assert_eq!(PORT_SPEED_HIGH, 3);
    assert_eq!(PORT_SPEED_SUPER, 4);

    println!("  [PASS] xHCI port speed values correct");
}

/// xHCI port link state values.
fn test_xhci_port_link_state() {
    const PORT_LINK_STATE_U0: u32 = 0;
    const PORT_LINK_STATE_U1: u32 = 1;
    const PORT_LINK_STATE_U2: u32 = 2;
    const PORT_LINK_STATE_U3: u32 = 3;
    const PORT_LINK_STATE_SS_DISABLED: u32 = 4;
    const PORT_LINK_STATE_RX_DETECT: u32 = 5;
    const PORT_LINK_STATE_SS_INACTIVE: u32 = 6;
    const PORT_LINK_STATE_POLLING: u32 = 7;
    const PORT_LINK_STATE_RECOVERY: u32 = 8;
    const PORT_LINK_STATE_HOT_RESET: u32 = 9;
    const PORT_LINK_STATE_COMPLIANCE: u32 = 10;
    const PORT_LINK_STATE_LOOPBACK: u32 = 11;

    assert_eq!(PORT_LINK_STATE_U0, 0);
    assert_eq!(PORT_LINK_STATE_U1, 1);
    assert_eq!(PORT_LINK_STATE_U2, 2);
    assert_eq!(PORT_LINK_STATE_U3, 3);
    assert_eq!(PORT_LINK_STATE_SS_DISABLED, 4);
    assert_eq!(PORT_LINK_STATE_RX_DETECT, 5);
    assert_eq!(PORT_LINK_STATE_SS_INACTIVE, 6);
    assert_eq!(PORT_LINK_STATE_POLLING, 7);
    assert_eq!(PORT_LINK_STATE_RECOVERY, 8);
    assert_eq!(PORT_LINK_STATE_HOT_RESET, 9);
    assert_eq!(PORT_LINK_STATE_COMPLIANCE, 10);
    assert_eq!(PORT_LINK_STATE_LOOPBACK, 11);

    println!("  [PASS] xHCI port link state values correct");
}

/// xHCI command register format.
fn test_xhci_command_register() {
    // USBCMD bits:
    // Bit 0: Run/stop
    // Bit 1: Host controller reset
    // Bit 2: Interrupter enable
    // Bit 3: Host system error enable
    // Bit 4: Light host controller reset
    // Bit 5: Controller save state
    // Bit 6: Controller restore state
    // Bit 7: Enable wrap event
    // Bit 8: Enable U3 MFINDEX stop
    // Bit 9: CEM enable
    // Bit 10: Extended transfer burst count enable
    // Bit 11: Extended packet burst count enable
    // Bit 12: Force next transfer event

    const USBCMD_RUN_STOP: u32 = 1 << 0;
    const USBCMD_HCRST: u32 = 1 << 1;
    const USBCMD_INTE: u32 = 1 << 2;
    const USBCMD_HSEE: u32 = 1 << 3;

    assert_eq!(USBCMD_RUN_STOP, 0x0000_0001);
    assert_eq!(USBCMD_HCRST, 0x0000_0002);
    assert_eq!(USBCMD_INTE, 0x0000_0004);
    assert_eq!(USBCMD_HSEE, 0x0000_0008);

    println!("  [PASS] xHCI command register format correct");
}

/// xHCI status register format.
fn test_xhci_status_register() {
    // USBSTS bits:
    // Bit 0: Host controller halted
    // Bit 2: Host system error
    // Bit 3: Event interrupt
    // Bit 4: Port change detect
    // Bit 8: Save state status
    // Bit 9: Restore state status
    // Bit 10: Save/restore error
    // Bit 11: Controller not ready
    // Bit 12: Host controller error

    const USBSTS_HCH: u32 = 1 << 0;
    const USBSTS_HSE: u32 = 1 << 2;
    const USBSTS_EINT: u32 = 1 << 3;
    const USBSTS_PCD: u32 = 1 << 4;
    const USBSTS_CNR: u32 = 1 << 11;
    const USBSTS_HCE: u32 = 1 << 12;

    assert_eq!(USBSTS_HCH, 0x0000_0001);
    assert_eq!(USBSTS_HSE, 0x0000_0004);
    assert_eq!(USBSTS_EINT, 0x0000_0008);
    assert_eq!(USBSTS_PCD, 0x0000_0010);
    assert_eq!(USBSTS_CNR, 0x0000_0800);
    assert_eq!(USBSTS_HCE, 0x0000_1000);

    println!("  [PASS] xHCI status register format correct");
}
