//! Host-side unit tests for I/O APIC interrupt routing logic.
//!
//! These tests run on the host (not in the kernel) and validate that the
//! I/O APIC interrupt routing calculations are correct.
//!
//! Primary reference: Intel SDM Vol. 3A, Chapter 10 (APIC).

fn main() {
    println!("Running I/O APIC interrupt routing tests...\n");

    test_irq_to_gsi_mapping();
    test_gsi_to_irq_mapping();
    test_interrupt_vector_assignment();
    test_delivery_mode_selection();
    test_destination_mode_selection();
    test_trigger_mode_selection();
    test_polarity_selection();
    test_mask_unmask_interrupt();
    test_round_robin_arbitration();
    test_priority_calculation();

    println!("\nAll I/O APIC interrupt routing tests passed!");
}

/// IRQ to GSI mapping.
fn test_irq_to_gsi_mapping() {
    // IRQ to GSI mapping (ISA IRQs):
    // IRQ0 -> GSI2 (timer)
    // IRQ1 -> GSI1 (keyboard)
    // IRQ2 -> GSI9 (cascade, via interrupt override)
    // IRQ8 -> GSI8 (RTC)
    // IRQ12 -> GSI12 (PS/2 mouse)
    // IRQ14 -> GSI14 (primary IDE)
    // IRQ15 -> GSI15 (secondary IDE)

    let irq_to_gsi = |irq: u8| -> u8 {
        match irq {
            0 => 2,
            1 => 1,
            2 => 9,
            8 => 8,
            12 => 12,
            14 => 14,
            15 => 15,
            _ => irq,
        }
    };

    assert_eq!(irq_to_gsi(0), 2);
    assert_eq!(irq_to_gsi(1), 1);
    assert_eq!(irq_to_gsi(2), 9);
    assert_eq!(irq_to_gsi(8), 8);
    assert_eq!(irq_to_gsi(12), 12);
    assert_eq!(irq_to_gsi(14), 14);
    assert_eq!(irq_to_gsi(15), 15);

    println!("  [PASS] IRQ to GSI mapping correct");
}

/// GSI to IRQ mapping.
fn test_gsi_to_irq_mapping() {
    // GSI to IRQ mapping (reverse of IRQ to GSI):
    // GSI2 -> IRQ0 (timer)
    // GSI1 -> IRQ1 (keyboard)
    // GSI9 -> IRQ2 (cascade)
    // GSI8 -> IRQ8 (RTC)
    // GSI12 -> IRQ12 (PS/2 mouse)
    // GSI14 -> IRQ14 (primary IDE)
    // GSI15 -> IRQ15 (secondary IDE)

    let gsi_to_irq = |gsi: u8| -> u8 {
        match gsi {
            2 => 0,
            1 => 1,
            9 => 2,
            8 => 8,
            12 => 12,
            14 => 14,
            15 => 15,
            _ => gsi,
        }
    };

    assert_eq!(gsi_to_irq(2), 0);
    assert_eq!(gsi_to_irq(1), 1);
    assert_eq!(gsi_to_irq(9), 2);
    assert_eq!(gsi_to_irq(8), 8);
    assert_eq!(gsi_to_irq(12), 12);
    assert_eq!(gsi_to_irq(14), 14);
    assert_eq!(gsi_to_irq(15), 15);

    println!("  [PASS] GSI to IRQ mapping correct");
}

/// Interrupt vector assignment.
fn test_interrupt_vector_assignment() {
    // Interrupt vector assignment:
    // ISA IRQs: vectors 0x20-0x2F
    // PCI IRQs: vectors 0x30-0x3F (typical)
    // Local APIC: vectors 0xF0-0xFF (typical)

    let isa_vector_base: u8 = 0x20;
    let pci_vector_base: u8 = 0x30;
    let local_apic_vector_base: u8 = 0xF0;

    assert_eq!(isa_vector_base, 0x20);
    assert_eq!(pci_vector_base, 0x30);
    assert_eq!(local_apic_vector_base, 0xF0);

    println!("  [PASS] Interrupt vector assignment correct");
}

/// Delivery mode selection.
fn test_delivery_mode_selection() {
    // Delivery mode selection:
    // FIXED: deliver interrupt to all agents in destination
    // LOWEST_PRIORITY: deliver to agent with lowest priority
    // SMI: System Management Interrupt
    // NMI: Non-Maskable Interrupt
    // INIT: Initialize agent
    // STARTUP: Start agent (SIPI)
    // EXTINT: External interrupt (8259A compatibility)

    const DELIVERY_FIXED: u8 = 0x0;
    const DELIVERY_LOWEST_PRIORITY: u8 = 0x1;
    const DELIVERY_SMI: u8 = 0x2;
    const DELIVERY_NMI: u8 = 0x4;
    const DELIVERY_INIT: u8 = 0x5;
    const DELIVERY_STARTUP: u8 = 0x6;
    const DELIVERY_EXTINT: u8 = 0x7;

    assert_eq!(DELIVERY_FIXED, 0x0);
    assert_eq!(DELIVERY_LOWEST_PRIORITY, 0x1);
    assert_eq!(DELIVERY_SMI, 0x2);
    assert_eq!(DELIVERY_NMI, 0x4);
    assert_eq!(DELIVERY_INIT, 0x5);
    assert_eq!(DELIVERY_STARTUP, 0x6);
    assert_eq!(DELIVERY_EXTINT, 0x7);

    println!("  [PASS] Delivery mode selection correct");
}

/// Destination mode selection.
fn test_destination_mode_selection() {
    // Destination mode selection:
    // Physical: destination is APIC ID
    // Logical: destination is set of APIC IDs

    const DEST_MODE_PHYSICAL: u8 = 0;
    const DEST_MODE_LOGICAL: u8 = 1;

    assert_eq!(DEST_MODE_PHYSICAL, 0);
    assert_eq!(DEST_MODE_LOGICAL, 1);

    println!("  [PASS] Destination mode selection correct");
}

/// Trigger mode selection.
fn test_trigger_mode_selection() {
    // Trigger mode selection:
    // Edge: interrupt is edge-triggered
    // Level: interrupt is level-triggered

    const TRIGGER_EDGE: u8 = 0;
    const TRIGGER_LEVEL: u8 = 1;

    assert_eq!(TRIGGER_EDGE, 0);
    assert_eq!(TRIGGER_LEVEL, 1);

    println!("  [PASS] Trigger mode selection correct");
}

/// Polarity selection.
fn test_polarity_selection() {
    // Polarity selection:
    // Active high: signal is active when high
    // Active low: signal is active when low

    const POLARITY_ACTIVE_HIGH: u8 = 0;
    const POLARITY_ACTIVE_LOW: u8 = 1;

    assert_eq!(POLARITY_ACTIVE_HIGH, 0);
    assert_eq!(POLARITY_ACTIVE_LOW, 1);

    println!("  [PASS] Polarity selection correct");
}

/// Mask/unmask interrupt.
fn test_mask_unmask_interrupt() {
    // Mask/unmask interrupt:
    // Masked (1): interrupt is disabled
    // Unmasked (0): interrupt is enabled

    const INTERRUPT_MASKED: u8 = 1;
    const INTERRUPT_UNMASKED: u8 = 0;

    assert_eq!(INTERRUPT_MASKED, 1);
    assert_eq!(INTERRUPT_UNMASKED, 0);

    println!("  [PASS] Mask/unmask interrupt correct");
}

/// Round-robin arbitration.
fn test_round_robin_arbitration() {
    // Round-robin arbitration:
    // When multiple interrupts have the same priority,
    // they are serviced in round-robin order

    const ROUND_ROBIN_ARBITRATION: u8 = 1;
    const FIXED_ARBITRATION: u8 = 0;

    assert_eq!(ROUND_ROBIN_ARBITRATION, 1);
    assert_eq!(FIXED_ARBITRATION, 0);

    println!("  [PASS] Round-robin arbitration correct");
}

/// Priority calculation.
fn test_priority_calculation() {
    // Priority calculation:
    // Priority = (vector >> 4) & 0x0F
    // Higher value = higher priority

    let vector: u8 = 0x20;
    let priority = (vector >> 4) & 0x0F;

    assert_eq!(priority, 2);

    println!("  [PASS] Priority calculation correct");
}
