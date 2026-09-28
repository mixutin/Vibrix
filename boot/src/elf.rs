//! Shared ELF64 validation used by both firmware loader and kernel userspace loader.

#[path = "../../shared/elf.rs"]
mod shared;

pub use shared::*;
