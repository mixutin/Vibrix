#![no_std]

#[cfg(test)]
extern crate std;

pub mod block;
pub mod config_policy;
pub mod cpu_topology;
pub mod nic;
pub mod process;
pub mod update_policy;

/// QEMU-only caller supplies the real kernel's independent output paths.
/// A marker is emitted only after the corresponding behavior succeeds.
pub fn subsystem_self_test(mut report: impl FnMut(&str)) {
    block::self_test().expect("block abstraction self-test failed");
    report("VIBRIX: kernel block abstraction verified");
    nic::self_test().expect("NIC abstraction self-test failed");
    report("VIBRIX: kernel NIC abstraction verified");
    process::self_test().expect("process lifecycle self-test failed");
    report("VIBRIX: kernel process lifecycle verified");
    config_policy::self_test().expect("portable configuration policy self-test failed");
    report("VIBRIX: kernel portable configuration policy verified");
    update_policy::self_test().expect("update policy model self-test failed");
    report("VIBRIX: kernel update policy model verified");
}
