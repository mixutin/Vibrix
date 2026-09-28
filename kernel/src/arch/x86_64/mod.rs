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
#[cfg(all(feature = "pci-irq-probe", not(feature = "panic-probe")))]
mod pci_irq_probe;
#[cfg(all(
    any(feature = "pci-irq-probe", feature = "pci-msix-probe"),
    not(feature = "panic-probe")
))]
#[path = "../../../../shared/pci_interrupts/msi.rs"]
mod pci_msi;
#[cfg(all(feature = "pci-msix-probe", not(feature = "panic-probe")))]
#[path = "../../../../shared/pci_interrupts/msix.rs"]
mod pci_msix;
#[cfg(all(feature = "pci-msix-probe", not(feature = "panic-probe")))]
mod pci_msix_probe;
#[cfg(not(feature = "panic-probe"))]
pub mod ps2;
#[cfg(not(feature = "panic-probe"))]
pub mod reset;
#[cfg(any(feature = "ring3-probe", feature = "address-space-probe"))]
pub mod ring3;
pub mod serial;
