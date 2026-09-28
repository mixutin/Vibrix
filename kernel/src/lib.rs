#![no_std]

pub mod block;

/// QEMU-only caller supplies the real kernel's independent output paths.
/// A marker is emitted only after the corresponding behavior succeeds.
pub fn subsystem_self_test(mut report: impl FnMut(&str)) {
    block::self_test().expect("block abstraction self-test failed");
    report("VIBRIX: kernel block abstraction verified");
}
