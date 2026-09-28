#[allow(dead_code)]
pub mod acpi;
pub mod acpi_runtime;
pub mod apic;
pub mod cpuid;
pub mod gdt;
pub mod idt;
pub mod irq;
pub mod pci;
mod pci_caps;
#[path = "../../../../shared/pci_interrupts/mod.rs"]
pub mod pci_interrupts;
#[cfg(not(feature = "panic-probe"))]
pub mod ps2;
#[cfg(not(feature = "panic-probe"))]
pub mod reset;
pub mod serial;
