#![cfg_attr(target_os = "none", no_std)]

#[path = "../shared/pci_interrupts/mod.rs"]
pub mod pci_interrupts;

#[cfg(all(not(test), not(target_os = "none")))]
fn main() {}
