//! x86-64 Interrupt Descriptor Table (IDT) and exception handlers.
//!
//! Clean-room implementation from the Intel 64 and IA-32 Architectures
//! Software Developer's Manual, Volume 3, Chapter 6 (Interrupt and
//! Exception Handling).

use core::arch::{asm, naked_asm};

/// A single 16-byte IDT entry (gate descriptor).
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct IdtEntry {
    offset_low: u16,
    selector: u16,
    ist: u8,
    type_attr: u8,
    offset_mid: u16,
    offset_high: u32,
    reserved: u32,
}

impl IdtEntry {
    pub const NULL: Self = Self::new(0, 0, 0, 0);

    pub const fn new(handler: u64, selector: u16, ist: u8, type_attr: u8) -> Self {
        Self {
            offset_low: (handler & 0xFFFF) as u16,
            selector,
            ist: ist & 0x07,
            type_attr,
            offset_mid: ((handler >> 16) & 0xFFFF) as u16,
            offset_high: ((handler >> 32) & 0xFFFF_FFFF) as u32,
            reserved: 0,
        }
    }

    pub const fn interrupt_gate(handler: u64, selector: u16) -> Self {
        Self::new(handler, selector, 0, 0x8E)
    }

    pub const fn trap_gate(handler: u64, selector: u16) -> Self {
        Self::new(handler, selector, 0, 0x8F)
    }

    pub const fn user_interrupt_gate(handler: u64, selector: u16) -> Self {
        Self::new(handler, selector, 0, 0xEE)
    }

    pub fn is_present(&self) -> bool {
        self.type_attr & 0x80 != 0
    }

    pub fn handler_address(&self) -> u64 {
        self.offset_low as u64
            | ((self.offset_mid as u64) << 16)
            | ((self.offset_high as u64) << 32)
    }
}

/// The Interrupt Descriptor Table (256 entries).
#[repr(C, align(16))]
pub struct Idt {
    entries: [IdtEntry; 256],
}

impl Idt {
    pub const fn new() -> Self {
        Self {
            entries: [IdtEntry::NULL; 256],
        }
    }

    pub fn set_handler(&mut self, vector: u8, entry: IdtEntry) {
        self.entries[vector as usize] = entry;
    }

    pub fn pointer(&self) -> IdtPointer {
        IdtPointer {
            limit: (core::mem::size_of::<Self>() - 1) as u16,
            base: self as *const _ as u64,
        }
    }
}

/// IDTR structure (loaded by LIDT).
#[repr(C, packed)]
pub struct IdtPointer {
    limit: u16,
    base: u64,
}

/// Exception handler function type.
pub type ExceptionHandler = extern "C" fn(frame: &ExceptionFrame, error_code: u64);

/// CPU exception frame.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ExceptionFrame {
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

/// Exception handler table.
static mut HANDLERS: [Option<ExceptionHandler>; 256] = [None; 256];

pub unsafe fn register_handler(vector: u8, handler: ExceptionHandler) {
    unsafe {
        HANDLERS[vector as usize] = Some(handler);
    }
}

pub unsafe fn load(idt: &Idt) {
    let ptr = idt.pointer();
    unsafe {
        asm!("lidt [{}]", in(reg) &ptr, options(nostack));
    }
}

pub unsafe fn init() {
    let mut idt = Idt::new();

    idt.set_handler(0, IdtEntry::interrupt_gate(exception_divide_by_zero as *const () as u64, 0x08));
    idt.set_handler(1, IdtEntry::interrupt_gate(exception_debug as *const () as u64, 0x08));
    idt.set_handler(2, IdtEntry::interrupt_gate(exception_nmi as *const () as u64, 0x08));
    idt.set_handler(3, IdtEntry::user_interrupt_gate(exception_breakpoint as *const () as u64, 0x08));
    idt.set_handler(4, IdtEntry::interrupt_gate(exception_overflow as *const () as u64, 0x08));
    idt.set_handler(5, IdtEntry::interrupt_gate(exception_bound_range as *const () as u64, 0x08));
    idt.set_handler(6, IdtEntry::interrupt_gate(exception_invalid_opcode as *const () as u64, 0x08));
    idt.set_handler(7, IdtEntry::interrupt_gate(exception_device_not_available as *const () as u64, 0x08));
    idt.set_handler(8, IdtEntry::interrupt_gate(exception_double_fault as *const () as u64, 0x08));
    idt.set_handler(10, IdtEntry::interrupt_gate(exception_invalid_tss as *const () as u64, 0x08));
    idt.set_handler(11, IdtEntry::interrupt_gate(exception_segment_not_present as *const () as u64, 0x08));
    idt.set_handler(12, IdtEntry::interrupt_gate(exception_stack_fault as *const () as u64, 0x08));
    idt.set_handler(13, IdtEntry::interrupt_gate(exception_general_protection as *const () as u64, 0x08));
    idt.set_handler(14, IdtEntry::interrupt_gate(exception_page_fault as *const () as u64, 0x08));
    idt.set_handler(16, IdtEntry::interrupt_gate(exception_x87_floating_point as *const () as u64, 0x08));
    idt.set_handler(17, IdtEntry::interrupt_gate(exception_alignment_check as *const () as u64, 0x08));
    idt.set_handler(18, IdtEntry::interrupt_gate(exception_machine_check as *const () as u64, 0x08));
    idt.set_handler(19, IdtEntry::interrupt_gate(exception_simd_floating_point as *const () as u64, 0x08));
    idt.set_handler(20, IdtEntry::interrupt_gate(exception_virtualization as *const () as u64, 0x08));
    idt.set_handler(21, IdtEntry::interrupt_gate(exception_control_protection as *const () as u64, 0x08));
    idt.set_handler(28, IdtEntry::interrupt_gate(exception_hypervisor_injection as *const () as u64, 0x08));
    idt.set_handler(29, IdtEntry::interrupt_gate(exception_vmm_communication as *const () as u64, 0x08));
    idt.set_handler(30, IdtEntry::interrupt_gate(exception_security as *const () as u64, 0x08));

    unsafe {
        load(&idt);
    }
}

macro_rules! exception_stub {
    ($name:ident, $vector:expr, $has_error_code:expr) => {
        #[unsafe(naked)]
        pub extern "C" fn $name() {
            naked_asm!(
                    "push rax\n\
                     push rcx\n\
                     push rdx\n\
                     push rbx\n\
                     push rbp\n\
                     push rsi\n\
                     push rdi\n\
                     push r8\n\
                     push r9\n\
                     push r10\n\
                     push r11\n\
                     push r12\n\
                     push r13\n\
                     push r14\n\
                     push r15\n\
                     mov rdi, rsp\n\
                     mov rsi, {errcode}\n\
                     call {handler}\n\
                     pop r15\n\
                     pop r14\n\
                     pop r13\n\
                     pop r12\n\
                     pop r11\n\
                     pop r10\n\
                     pop r9\n\
                     pop r8\n\
                     pop rdi\n\
                     pop rsi\n\
                     pop rbp\n\
                     pop rbx\n\
                     pop rdx\n\
                     pop rcx\n\
                     pop rax\n\
                     iretq",
                    errcode = const $has_error_code,
                    handler = sym exception_handler,
                );
        }
    };
}

exception_stub!(exception_divide_by_zero, 0, 0);
exception_stub!(exception_debug, 1, 0);
exception_stub!(exception_nmi, 2, 0);
exception_stub!(exception_breakpoint, 3, 0);
exception_stub!(exception_overflow, 4, 0);
exception_stub!(exception_bound_range, 5, 0);
exception_stub!(exception_invalid_opcode, 6, 0);
exception_stub!(exception_device_not_available, 7, 0);
exception_stub!(exception_double_fault, 8, 1);
exception_stub!(exception_invalid_tss, 10, 1);
exception_stub!(exception_segment_not_present, 11, 1);
exception_stub!(exception_stack_fault, 12, 1);
exception_stub!(exception_general_protection, 13, 1);
exception_stub!(exception_page_fault, 14, 1);
exception_stub!(exception_x87_floating_point, 16, 0);
exception_stub!(exception_alignment_check, 17, 1);
exception_stub!(exception_machine_check, 18, 0);
exception_stub!(exception_simd_floating_point, 19, 0);
exception_stub!(exception_virtualization, 20, 0);
exception_stub!(exception_control_protection, 21, 1);
exception_stub!(exception_hypervisor_injection, 28, 1);
exception_stub!(exception_vmm_communication, 29, 1);
exception_stub!(exception_security, 30, 1);

extern "C" fn exception_handler(_registers: *const u64, _error_code: u64) {
    loop {
        unsafe {
            asm!("hlt");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_idt_entry_size() {
        assert_eq!(core::mem::size_of::<IdtEntry>(), 16);
    }

    #[test]
    fn test_idt_size() {
        assert_eq!(core::mem::size_of::<Idt>(), 256 * 16);
    }

    #[test]
    fn test_idt_pointer_size() {
        assert_eq!(core::mem::size_of::<IdtPointer>(), 10);
    }

    #[test]
    fn test_null_entry() {
        let entry = IdtEntry::NULL;
        assert!(!entry.is_present());
        assert_eq!(entry.handler_address(), 0);
    }

    #[test]
    fn test_interrupt_gate() {
        let entry = IdtEntry::interrupt_gate(0xFFFFFFFF8000_0000, 0x08);
        assert!(entry.is_present());
        assert_eq!(entry.handler_address(), 0xFFFFFFFF8000_0000);
    }

    #[test]
    fn test_trap_gate() {
        let entry = IdtEntry::trap_gate(0xFFFFFFFF8000_0000, 0x08);
        assert!(entry.is_present());
        assert_eq!(entry.handler_address(), 0xFFFFFFFF8000_0000);
    }

    #[test]
    fn test_user_interrupt_gate() {
        let entry = IdtEntry::user_interrupt_gate(0xFFFFFFFF8000_0000, 0x08);
        assert!(entry.is_present());
        assert_eq!(entry.handler_address(), 0xFFFFFFFF8000_0000);
    }

    #[test]
    fn test_idt_set_handler() {
        let mut idt = Idt::new();
        let entry = IdtEntry::interrupt_gate(0xFFFFFFFF8000_0000, 0x08);
        idt.set_handler(14, entry);
        assert!(idt.entries[14].is_present());
        assert_eq!(idt.entries[14].handler_address(), 0xFFFFFFFF8000_0000);
    }
}
