#[allow(dead_code)]
pub mod acpi;
pub mod acpi_runtime;
pub mod cpuid;
pub mod gdt;
pub mod idt;
pub mod pci;
#[cfg(not(feature = "panic-probe"))]
pub mod ps2;
pub mod serial;
