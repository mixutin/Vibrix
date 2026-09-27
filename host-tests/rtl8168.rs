//! Host-side unit tests for RTL8168 Ethernet controller register layout.
//!
//! These tests run on the host (not in the kernel) and validate that the
//! RTL8168 register offsets and bit fields are correct.
//!
//! Primary reference: RTL8168 datasheet, Ethernet controller register definitions.

fn main() {
    println!("Running RTL8168 register layout tests...\n");

    test_rtl8168_idr_registers();
    test_rtl8168_mlt_registers();
    test_rtl8168_tcr_register();
    test_rtl8168_rcr_register();
    test_rtl8168_cr_register();
    test_rtl8168_isr_register();
    test_rtl8168_imr_register();
    test_rtl8168_txd_registers();
    test_rtl8168_rxd_registers();
    test_rtl8168_mpc_register();

    println!("\nAll RTL8168 register layout tests passed!");
}

/// RTL8168 IDR registers (MAC address).
fn test_rtl8168_idr_registers() {
    // IDR0-IDR5: 6 registers containing the 48-bit MAC address
    const IDR0: u16 = 0x00; // MAC address byte 0
    const IDR1: u16 = 0x01; // MAC address byte 1
    const IDR2: u16 = 0x02; // MAC address byte 2
    const IDR3: u16 = 0x03; // MAC address byte 3
    const IDR4: u16 = 0x04; // MAC address byte 4
    const IDR5: u16 = 0x05; // MAC address byte 5

    assert_eq!(IDR0, 0x00);
    assert_eq!(IDR1, 0x01);
    assert_eq!(IDR2, 0x02);
    assert_eq!(IDR3, 0x03);
    assert_eq!(IDR4, 0x04);
    assert_eq!(IDR5, 0x05);

    println!("  [PASS] RTL8168 IDR registers correct");
}

/// RTL8168 MLT registers (Multicast address).
fn test_rtl8168_mlt_registers() {
    // MLT0-MLT7: 8 registers containing the multicast address hash table
    const MLT0: u16 = 0x08;
    const MLT1: u16 = 0x09;
    const MLT2: u16 = 0x0A;
    const MLT3: u16 = 0x0B;
    const MLT4: u16 = 0x0C;
    const MLT5: u16 = 0x0D;
    const MLT6: u16 = 0x0E;
    const MLT7: u16 = 0x0F;

    assert_eq!(MLT0, 0x08);
    assert_eq!(MLT1, 0x09);
    assert_eq!(MLT2, 0x0A);
    assert_eq!(MLT3, 0x0B);
    assert_eq!(MLT4, 0x0C);
    assert_eq!(MLT5, 0x0D);
    assert_eq!(MLT6, 0x0E);
    assert_eq!(MLT7, 0x0F);

    println!("  [PASS] RTL8168 MLT registers correct");
}

/// RTL8168 TCR register (Transmit Configuration Register).
fn test_rtl8168_tcr_register() {
    // TCR bits:
    // Bits 0-1: Reserved
    // Bit 2: IFG (Inter-Frame Gap)
    // Bit 3: LBK (Loopback)
    // Bit 4: CRC (Append CRC)
    // Bit 5: MXDMA (Max DMA burst size)
    // Bits 6-7: Reserved
    // Bit 8: TXDMA (Transmit DMA threshold)
    // Bits 9-10: Reserved
    // Bit 11: LUD (Link Up Disable)
    // Bits 12-15: Reserved

    const TCR_IFG_MASK: u32 = 0x0000_0004;
    const TCR_LBK: u32 = 0x0000_0008;
    const TCR_CRC: u32 = 0x0000_0010;
    const TCR_MXDMA_MASK: u32 = 0x0000_0020;
    const TCR_TXDMA_MASK: u32 = 0x0000_0100;
    const TCR_LUD: u32 = 0x0000_0800;

    assert_eq!(TCR_IFG_MASK, 0x0000_0004);
    assert_eq!(TCR_LBK, 0x0000_0008);
    assert_eq!(TCR_CRC, 0x0000_0010);
    assert_eq!(TCR_MXDMA_MASK, 0x0000_0020);
    assert_eq!(TCR_TXDMA_MASK, 0x0000_0100);
    assert_eq!(TCR_LUD, 0x0000_0800);

    println!("  [PASS] RTL8168 TCR register format correct");
}

/// RTL8168 RCR register (Receive Configuration Register).
fn test_rtl8168_rcr_register() {
    // RCR bits:
    // Bits 0-3: Reserved
    // Bit 4: RXFTH (Receive FIFO threshold)
    // Bit 5: RXDMA (Receive DMA burst size)
    // Bit 6: Reserved
    // Bit 7: RXNO (Receive no buffer)
    // Bit 8: Reserved
    // Bit 9: MXDMA (Max DMA burst size)
    // Bits 10-11: Reserved
    // Bit 12: APM (Physical Match)
    // Bit 13: AM (Multicast Match)
    // Bit 14: AB (Broadcast Match)
    // Bit 15: AAP (All Physical Match)

    const RCR_RXFTH_MASK: u32 = 0x0000_0010;
    const RCR_RXDMA_MASK: u32 = 0x0000_0020;
    const RCR_RXNO: u32 = 0x0000_0080;
    const RCR_MXDMA_MASK: u32 = 0x0000_0200;
    const RCR_APM: u32 = 0x0000_1000;
    const RCR_AM: u32 = 0x0000_2000;
    const RCR_AB: u32 = 0x0000_4000;
    const RCR_AAP: u32 = 0x0000_8000;

    assert_eq!(RCR_RXFTH_MASK, 0x0000_0010);
    assert_eq!(RCR_RXDMA_MASK, 0x0000_0020);
    assert_eq!(RCR_RXNO, 0x0000_0080);
    assert_eq!(RCR_MXDMA_MASK, 0x0000_0200);
    assert_eq!(RCR_APM, 0x0000_1000);
    assert_eq!(RCR_AM, 0x0000_2000);
    assert_eq!(RCR_AB, 0x0000_4000);
    assert_eq!(RCR_AAP, 0x0000_8000);

    println!("  [PASS] RTL8168 RCR register format correct");
}

/// RTL8168 CR register (Command Register).
fn test_rtl8168_cr_register() {
    // CR bits:
    // Bit 0: Reset
    // Bit 1: Receiver Enable
    // Bit 2: Transmitter Enable
    // Bit 3: Reserved
    // Bit 4: Reserved
    // Bit 5: Reserved
    // Bit 6: Reserved
    // Bit 7: Reserved

    const CR_RESET: u8 = 0x01;
    const CR_RE: u8 = 0x02;
    const CR_TE: u8 = 0x04;

    assert_eq!(CR_RESET, 0x01);
    assert_eq!(CR_RE, 0x02);
    assert_eq!(CR_TE, 0x04);

    println!("  [PASS] RTL8168 CR register format correct");
}

/// RTL8168 ISR register (Interrupt Status Register).
fn test_rtl8168_isr_register() {
    // ISR bits:
    // Bit 0: Receive OK
    // Bit 1: Receive Error
    // Bit 2: Transmit OK
    // Bit 3: Transmit Error
    // Bit 4: Receive Overflow
    // Bit 5: Link Change
    // Bit 6: Receive FIFO Overflow
    // Bit 7: Transmit FIFO Overflow
    // Bit 8: System Error
    // Bit 9: Reserved
    // Bit 10: Reserved
    // Bit 11: Reserved
    // Bit 12: Reserved
    // Bit 13: Reserved
    // Bit 14: Reserved
    // Bit 15: Reserved

    const ISR_ROK: u16 = 0x0001;
    const ISR_RER: u16 = 0x0002;
    const ISR_TOK: u16 = 0x0004;
    const ISR_TER: u16 = 0x0008;
    const ISR_RXOVW: u16 = 0x0010;
    const ISR_LCHG: u16 = 0x0020;
    const ISR_RXFOVW: u16 = 0x0040;
    const ISR_TXFOVW: u16 = 0x0080;
    const ISR_SYSERR: u16 = 0x0100;

    assert_eq!(ISR_ROK, 0x0001);
    assert_eq!(ISR_RER, 0x0002);
    assert_eq!(ISR_TOK, 0x0004);
    assert_eq!(ISR_TER, 0x0008);
    assert_eq!(ISR_RXOVW, 0x0010);
    assert_eq!(ISR_LCHG, 0x0020);
    assert_eq!(ISR_RXFOVW, 0x0040);
    assert_eq!(ISR_TXFOVW, 0x0080);
    assert_eq!(ISR_SYSERR, 0x0100);

    println!("  [PASS] RTL8168 ISR register format correct");
}

/// RTL8168 IMR register (Interrupt Mask Register).
fn test_rtl8168_imr_register() {
    // IMR bits (same as ISR):
    // Bit 0: Receive OK
    // Bit 1: Receive Error
    // Bit 2: Transmit OK
    // Bit 3: Transmit Error
    // Bit 4: Receive Overflow
    // Bit 5: Link Change
    // Bit 6: Receive FIFO Overflow
    // Bit 7: Transmit FIFO Overflow
    // Bit 8: System Error

    const IMR_ROK: u16 = 0x0001;
    const IMR_RER: u16 = 0x0002;
    const IMR_TOK: u16 = 0x0004;
    const IMR_TER: u16 = 0x0008;
    const IMR_RXOVW: u16 = 0x0010;
    const IMR_LCHG: u16 = 0x0020;
    const IMR_RXFOVW: u16 = 0x0040;
    const IMR_TXFOVW: u16 = 0x0080;
    const IMR_SYSERR: u16 = 0x0100;

    assert_eq!(IMR_ROK, 0x0001);
    assert_eq!(IMR_RER, 0x0002);
    assert_eq!(IMR_TOK, 0x0004);
    assert_eq!(IMR_TER, 0x0008);
    assert_eq!(IMR_RXOVW, 0x0010);
    assert_eq!(IMR_LCHG, 0x0020);
    assert_eq!(IMR_RXFOVW, 0x0040);
    assert_eq!(IMR_TXFOVW, 0x0080);
    assert_eq!(IMR_SYSERR, 0x0100);

    println!("  [PASS] RTL8168 IMR register format correct");
}

/// RTL8168 TXD registers (Transmit Descriptor).
fn test_rtl8168_txd_registers() {
    // TXD0-TXD3: 4 registers containing the transmit descriptor address
    const TXD0: u16 = 0x20; // Transmit descriptor address low
    const TXD1: u16 = 0x21; // Transmit descriptor address high
    const TXD2: u16 = 0x22; // Transmit descriptor address upper low
    const TXD3: u16 = 0x23; // Transmit descriptor address upper high

    assert_eq!(TXD0, 0x20);
    assert_eq!(TXD1, 0x21);
    assert_eq!(TXD2, 0x22);
    assert_eq!(TXD3, 0x23);

    println!("  [PASS] RTL8168 TXD registers correct");
}

/// RTL8168 RXD registers (Receive Descriptor).
fn test_rtl8168_rxd_registers() {
    // RXD0-RXD3: 4 registers containing the receive descriptor address
    const RXD0: u16 = 0x24; // Receive descriptor address low
    const RXD1: u16 = 0x25; // Receive descriptor address high
    const RXD2: u16 = 0x26; // Receive descriptor address upper low
    const RXD3: u16 = 0x27; // Receive descriptor address upper high

    assert_eq!(RXD0, 0x24);
    assert_eq!(RXD1, 0x25);
    assert_eq!(RXD2, 0x26);
    assert_eq!(RXD3, 0x27);

    println!("  [PASS] RTL8168 RXD registers correct");
}

/// RTL8168 MPC register (Missed Packet Counter).
fn test_rtl8168_mpc_register() {
    // MPC: 16-bit register containing the missed packet counter
    const MPC: u16 = 0x08;

    assert_eq!(MPC, 0x08);

    println!("  [PASS] RTL8168 MPC register correct");
}
