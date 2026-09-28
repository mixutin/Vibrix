#![cfg_attr(not(test), no_std)]

#[path = "../shared/pci_interrupts/mod.rs"]
pub mod pci_interrupts;
#[path = "../shared/pci_interrupts/msi.rs"]
pub mod pci_msi;
#[path = "../shared/pci_interrupts/msix.rs"]
pub mod pci_msix;
