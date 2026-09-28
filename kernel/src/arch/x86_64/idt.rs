//! Minimal single-CPU x86-64 exception table for Vibrix.
//!
//! Installs synchronous exception gates plus timer/spurious IRQ vectors.
//! Explicit PCI interrupt probe builds also reserve permanent diagnostic vectors.
//! Hardware delivery stays disabled until native controller setup completes.
//! No privilege-transition IST policy or SMP IDT synchronization exists yet.

use core::{arch::asm, cell::UnsafeCell};

#[path = "../../../../shared/idt_layout.rs"]
mod layout;

use layout::{
    IdtGate, IdtPointer, IdtTable, PageFaultReason, VECTOR_BREAKPOINT, VECTOR_DOUBLE_FAULT,
    VECTOR_GENERAL_PROTECTION, VECTOR_PAGE_FAULT,
};

use crate::arch::x86_64::gdt::Gdt;
use crate::arch::x86_64::irq::{SPURIOUS_VECTOR, TIMER_VECTOR};

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

/// Install synchronous and reserved interrupt gates before any delivery.
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
        breakpoint_handler as *const () as usize as u64,
        Gdt::KERNEL_CODE_SELECTOR,
    );
    table.0[VECTOR_DOUBLE_FAULT] = IdtGate::interrupt(
        double_fault_handler as *const () as usize as u64,
        Gdt::KERNEL_CODE_SELECTOR,
    );
    table.0[VECTOR_GENERAL_PROTECTION] = IdtGate::interrupt(
        general_protection_handler as *const () as usize as u64,
        Gdt::KERNEL_CODE_SELECTOR,
    );
    table.0[VECTOR_PAGE_FAULT] = IdtGate::interrupt(
        page_fault_handler as *const () as usize as u64,
        Gdt::KERNEL_CODE_SELECTOR,
    );
    table.0[usize::from(TIMER_VECTOR)] = IdtGate::interrupt(
        timer_handler as *const () as usize as u64,
        Gdt::KERNEL_CODE_SELECTOR,
    );
    table.0[usize::from(SPURIOUS_VECTOR)] = IdtGate::interrupt(
        spurious_handler as *const () as usize as u64,
        Gdt::KERNEL_CODE_SELECTOR,
    );
    #[cfg(feature = "ring3-probe")]
    {
        table.0[super::ring3::PROBE_VECTOR] = IdtGate::user_interrupt(
            ring3_probe_handler as *const () as usize as u64,
            Gdt::KERNEL_CODE_SELECTOR,
        );
    }
    #[cfg(all(feature = "pci-irq-probe", not(feature = "panic-probe")))]
    {
        table.0[usize::from(super::pci_irq_probe::VECTOR)] = IdtGate::interrupt(
            edu_msi_handler as *const () as usize as u64,
            Gdt::KERNEL_CODE_SELECTOR,
        );
    }
    #[cfg(all(feature = "pci-msix-probe", not(feature = "panic-probe")))]
    {
        table.0[usize::from(super::pci_msix_probe::VECTOR)] = IdtGate::interrupt(
            ivshmem_msix_handler as *const () as usize as u64,
            Gdt::KERNEL_CODE_SELECTOR,
        );
    }

    let pointer = IdtPointer {
        limit: (core::mem::size_of::<IdtTable>() - 1) as u16,
        base: table as *mut IdtTable as u64,
    };
    // SAFETY: the IDTR points to persistent, initialized supervisor-mapped
    // kernel memory; the gate selector is the live ring-0 code descriptor.
    unsafe { asm!("lidt [{}]", in(reg) &pointer, options(readonly, nostack, preserves_flags)) };
}

extern "x86-interrupt" fn timer_handler(_frame: InterruptStackFrame) {
    crate::arch::x86_64::irq::record_timer_tick();
    // SAFETY: the timer vector is unmasked only after activate_pit_timer()
    // permanently maps the LAPIC page on this sole BSP. EOI happens before
    // any scheduler stack switch so the suspended handler owns no live LAPIC
    // in-service state while another kernel thread executes.
    unsafe { crate::arch::x86_64::apic::eoi() };
    crate::thread::on_timer_interrupt();
}

#[cfg(all(feature = "pci-irq-probe", not(feature = "panic-probe")))]
extern "x86-interrupt" fn edu_msi_handler(_frame: InterruptStackFrame) {
    super::pci_irq_probe::interrupt();
}

#[cfg(all(feature = "pci-msix-probe", not(feature = "panic-probe")))]
extern "x86-interrupt" fn ivshmem_msix_handler(_frame: InterruptStackFrame) {
    super::pci_msix_probe::interrupt();
}

extern "x86-interrupt" fn spurious_handler(_frame: InterruptStackFrame) {
    // Architectural spurious-vector interrupts do not require an EOI.
}

#[cfg(feature = "ring3-probe")]
extern "x86-interrupt" fn ring3_probe_handler(frame: InterruptStackFrame) -> ! {
    let kernel_rsp: u64;
    // SAFETY: read-only inspection of the handler's current CPL0 stack pointer.
    unsafe { asm!("mov {}, rsp", out(reg) kernel_rsp, options(nomem, nostack, preserves_flags)) };
    let selectors_ok =
        super::ring3::user_frame_selectors_valid(frame.code_segment, frame.stack_segment);
    let rsp0_ok = super::gdt::ring0_stack_contains(kernel_rsp);
    if selectors_ok && rsp0_ok {
        crate::debugcon::write("VIBRIX: kernel CPL3 trap reached via TSS RSP0\r\n");
        crate::println!(
            "kernel ring3 probe: cs={:#x} ss={:#x} user_rsp={:#x} kernel_rsp={:#x}",
            frame.code_segment,
            frame.stack_segment,
            frame.stack_pointer,
            kernel_rsp
        );
    } else {
        crate::debugcon::write("VIBRIX: kernel CPL3 trap validation failed\r\n");
        crate::println!(
            "kernel ring3 probe rejected: cs={:#x} ss={:#x} user_rsp={:#x} kernel_rsp={:#x}",
            frame.code_segment,
            frame.stack_segment,
            frame.stack_pointer,
            kernel_rsp
        );
    }
    loop {
        core::hint::spin_loop();
    }
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
