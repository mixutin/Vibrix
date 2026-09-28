//! x86-64 IDT gate and descriptor layout; production IDT imports this
//! exact source, and the host tests exercise its wire representation.
#![allow(dead_code)]

pub const VECTOR_BREAKPOINT: usize = 3;
pub const VECTOR_DOUBLE_FAULT: usize = 8;
pub const VECTOR_GENERAL_PROTECTION: usize = 13;
pub const VECTOR_PAGE_FAULT: usize = 14;
pub const PRESENT_INTERRUPT_GATE: u8 = 0x8e;
pub const PRESENT_USER_INTERRUPT_GATE: u8 = 0xee;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct IdtGate {
    offset_low: u16,
    selector: u16,
    ist: u8,
    options: u8,
    offset_mid: u16,
    offset_high: u32,
    reserved: u32,
}

impl IdtGate {
    pub const MISSING: Self = Self {
        offset_low: 0,
        selector: 0,
        ist: 0,
        options: 0,
        offset_mid: 0,
        offset_high: 0,
        reserved: 0,
    };

    pub const fn interrupt(handler: u64, code_selector: u16) -> Self {
        Self {
            offset_low: handler as u16,
            selector: code_selector,
            ist: 0, // No configured TSS IST stack yet.
            options: PRESENT_INTERRUPT_GATE,
            offset_mid: (handler >> 16) as u16,
            offset_high: (handler >> 32) as u32,
            reserved: 0,
        }
    }

    /// Interrupt gate callable from CPL3. This changes only the descriptor's
    /// DPL; the handler still enters the ring-0 code selector with IST=0.
    pub const fn user_interrupt(handler: u64, code_selector: u16) -> Self {
        Self {
            offset_low: handler as u16,
            selector: code_selector,
            ist: 0,
            options: PRESENT_USER_INTERRUPT_GATE,
            offset_mid: (handler >> 16) as u16,
            offset_high: (handler >> 32) as u32,
            reserved: 0,
        }
    }

    pub const fn handler_address(&self) -> u64 {
        self.offset_low as u64
            | ((self.offset_mid as u64) << 16)
            | ((self.offset_high as u64) << 32)
    }
}

#[repr(C, align(16))]
pub struct IdtTable(pub [IdtGate; 256]);

impl IdtTable {
    pub const EMPTY: Self = Self([IdtGate::MISSING; 256]);
}

#[repr(C, packed)]
pub struct IdtPointer {
    pub limit: u16,
    pub base: u64,
}

/// Decode the first five architecturally meaningful #PF error bits without
/// misinterpreting reserved/newer bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageFaultReason {
    pub protection: bool,
    pub write: bool,
    pub user: bool,
    pub reserved_bit: bool,
    pub instruction_fetch: bool,
}

impl PageFaultReason {
    pub const fn from_error_code(bits: u64) -> Self {
        Self {
            protection: bits & 1 != 0,
            write: bits & (1 << 1) != 0,
            user: bits & (1 << 2) != 0,
            reserved_bit: bits & (1 << 3) != 0,
            instruction_fetch: bits & (1 << 4) != 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{align_of, offset_of, size_of};

    #[test]
    fn x64_gate_and_idtr_layout() {
        assert_eq!(size_of::<IdtGate>(), 16);
        assert_eq!(align_of::<IdtGate>(), 4);
        assert_eq!(size_of::<IdtTable>(), 4096);
        assert_eq!(align_of::<IdtTable>(), 16);
        assert_eq!(size_of::<IdtPointer>(), 10);
        assert_eq!(offset_of!(IdtGate, options), 5);
        assert_eq!(offset_of!(IdtGate, offset_high), 8);
        assert_eq!(offset_of!(IdtGate, reserved), 12);
        assert_eq!(offset_of!(IdtPointer, base), 2);
    }

    #[test]
    fn gate_round_trips_full_64_bit_handler_and_kernel_selector() {
        let h = 0xffff_ffff_8000_0123_u64;
        let gate = IdtGate::interrupt(h, 0x08);
        assert_eq!(gate.handler_address(), h);
        assert_eq!(gate.selector, 0x08);
        assert_eq!(gate.ist, 0);
        assert_eq!(gate.options, PRESENT_INTERRUPT_GATE);
        assert_eq!(gate.reserved, 0);
        assert_eq!(IdtGate::MISSING.options, 0);
    }

    #[test]
    fn user_interrupt_gate_is_present_dpl3_and_uses_kernel_selector() {
        let gate = IdtGate::user_interrupt(0xffff_ffff_8000_4567, 0x08);
        assert_eq!(gate.options, PRESENT_USER_INTERRUPT_GATE);
        assert_eq!(gate.options >> 5 & 0x3, 3);
        assert_eq!(gate.selector, 0x08);
        assert_eq!(gate.ist, 0);
        assert_eq!(gate.handler_address(), 0xffff_ffff_8000_4567);
    }

    #[test]
    fn fault_error_code_classification() {
        assert_eq!(
            PageFaultReason::from_error_code(0),
            PageFaultReason {
                protection: false,
                write: false,
                user: false,
                reserved_bit: false,
                instruction_fetch: false,
            }
        );
        assert_eq!(
            PageFaultReason::from_error_code(0b1_1111),
            PageFaultReason {
                protection: true,
                write: true,
                user: true,
                reserved_bit: true,
                instruction_fetch: true,
            }
        );
    }
}
