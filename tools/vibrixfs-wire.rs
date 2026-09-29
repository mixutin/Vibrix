//! Host entry point for the shared VibrixFS v1 wire codec.

#[path = "../shared/vibrixfs_wire.rs"]
mod wire;

pub use wire::*;

#[cfg(not(test))]
fn main() {
    eprintln!("VibrixFS wire conformance helper: run with rustc --test");
}
