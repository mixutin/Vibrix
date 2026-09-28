#![no_std]

#[cfg(test)]
extern crate std;

#[path = "mod.rs"]
pub mod vmm;

pub use vmm::*;
