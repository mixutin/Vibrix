//! Minimal single-CPU x86-64 exception table for Vibrix.
//!
//! The kernel still keeps interrupts disabled: this installs only
//! synchronous #BP, #DF, #GP and #PF gates. No APIC/IRQ/IST support yet.
//! Uses the nightly Rust x86-interrupt ABI for hardware-saved registers.

use core::{arch::asm, cell::UnsafeCell};

#[path = "../../../../shared/idt_layout.rs"]
mod layout;

use layout::{
    IdtGate, IdtPointer, IdtTable, PageFaultReason, VECTOR_BREAKPOINT, VECTOR_DOUBLE_FAULT,
    VECTOR_GENERAL_PROTECTION, VECTOR_PAGE_FAULT,
};

use crate::arch::x86_64::gdt::Gdt;

/// This is the CPU-pushed frame interpreted by the nightly x86-interrupt ABI.
/// All five machine words are valid for the current 64-bit kernel entry stack.
#[repr(C)]
#[derive(Clone, Copy)]
struct InterruptStackFrame {
    instruction_pointer: u64,
    code_segment: u64,
    cpu_flags: u64,
    stack_pointer: u64,
    stack_segment: u64,
}

struct PermanentIdt(UnsafeCell<IdtTable>);

// SAFETY: init() is called once with interrupts disabled on the BSP, prior
// to parallel execution. No software writes table memory after LIDT.
unsafe impl Sync for PermanentIdt {}

static IDT: PermanentIdt = PermanentIdt(UnsafeCell::new(IdtTable::EMPTY));

/// Install the table for synchronous traps. Do not STI until the IRQ model,
/// TSS privilege stacks and per-CPU IDT/locking are implemented.
///
/// # Safety
/// The caller is the sole boot CPU with interrupts disabled, has already
/// installed the permanent GDT/TSS, and has the IDT static storage mapped
/// for the kernel lifetime. This function is called only once.
pub unsafe fn init() {
    let table = IDT.0.get();
    // SAFETY: exclusive single-core bootstrap access before LIDT.
    let table = unsafe { &mut *table };
    table.0[VECTOR_BREAKPOINT] = IdtGate::interrupt(
        breakpoint_handler as usize as u64,
        Gdt::KERNEL_CODE_SELECTOR,
    );
    table.0[VECTOR_DOUBLE_FAULT] = IdtGate::interrupt(
        double_fault_handler as usize as u64,
        Gdt::KERNEL_CODE_SELECTOR,
    );
    table.0[VECTOR_GENERAL_PROTECTION] = IdtGate::interrupt(
        general_protection_handler as usize as u64,
        Gdt::KERNEL_CODE_SELECTOR,
    );
    table.0[VECTOR_PAGE_FAULT] = IdtGate::interrupt(
        page_fault_handler as usize as u64,
        Gdt::KERNEL_CODE_SELECTOR,
    );

    let pointer = IdtPointer {
        limit: (core::mem::size_of::<IdtTable>() - 1) as u16,
        base: table as *mut IdtTable as u64,
    };
    // SAFETY: the IDTR points to persistent, initialized supervisor-mapped
    // kernel memory; the gate selector is the live ring-0 code descriptor.
    unsafe { asm!("lidt [{}]", in(reg) &pointer, options(readonly, nostack, preserves_flags)) };
}

extern "x86-interrupt" fn breakpoint_handler(frame: InterruptStackFrame) {
    crate::debugcon::write("VIBRIX: kernel breakpoint exception handled\r\n");
    crate::println!("kernel #BP breakpoint rip={:#x}", frame.instruction_pointer);
}

extern "x86-interrupt" fn double_fault_handler(frame: InterruptStackFrame, error_code: u64) -> ! {
    crate::debugcon::write("VIBRIX: kernel double fault\r\n");
    crate::println!(
        "kernel #DF rip={:#x} error={:#x} (no IST yet)",
        frame.instruction_pointer,
        error_code
    );
    loop {
        core::hint::spin_loop();
    }
}

extern "x86-interrupt" fn general_protection_handler(
    frame: InterruptStackFrame,
    error_code: u64,
) -> ! {
    crate::debugcon::write("VIBRIX: kernel general protection fault\r\n");
    crate::println!(
        "kernel #GP rip={:#x} error={:#x}",
        frame.instruction_pointer,
        error_code
    );
    loop {
        core::hint::spin_loop();
    }
}

extern "x86-interrupt" fn page_fault_handler(frame: InterruptStackFrame, error_code: u64) -> ! {
    let cr2: u64;
    // SAFETY: CPL0 may read CR2. Do this first so debug printing cannot
    // overwrite the faulting linear address before it is captured.
    unsafe { asm!("mov {}, cr2", out(reg) cr2, options(nomem, nostack, preserves_flags)) };
    let reason = PageFaultReason::from_error_code(error_code);
    crate::debugcon::write("VIBRIX: kernel page fault diagnostic\r\n");
    crate::println!(
        "kernel #PF cr2={:#x} rip={:#x} error={:#x} present={} write={} user={} reserved={} exec={}",
        cr2,
        frame.instruction_pointer,
        error_code,
        reason.protection,
        reason.write,
        reason.user,
        reason.reserved_bit,
        reason.instruction_fetch
    );
    loop {
        core::hint::spin_loop();
    }
}
