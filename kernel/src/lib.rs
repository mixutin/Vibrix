#![no_std]

#[cfg(test)]
extern crate std;

pub mod block;
pub mod config_policy;
pub mod cpu_topology;
pub mod nic;
pub mod per_cpu;
pub mod process;
pub mod process_syscalls;
pub mod secure_random;
pub mod update_policy;
pub mod user_image;
pub mod user_stack;
pub mod vfs;

/// QEMU-only caller supplies the real kernel's independent output paths.
/// A marker is emitted only after the corresponding behavior succeeds.
pub fn subsystem_self_test(mut report: impl FnMut(&str)) {
    vfs::self_test(&mut report).expect("bootstrap VFS self-test failed");
    block::self_test().expect("block abstraction self-test failed");
    report("VIBRIX: kernel block abstraction verified");
    nic::self_test().expect("NIC abstraction self-test failed");
    report("VIBRIX: kernel NIC abstraction verified");
    nic::ipv4_icmp_self_test().expect("IPv4/ICMP self-test failed");
    report("VIBRIX: kernel IPv4 ICMP echo verified");
    nic::udp_self_test().expect("UDP self-test failed");
    report("VIBRIX: kernel UDP datagram verified");
    nic::ethernet_arp_self_test().expect("Ethernet/ARP self-test failed");
    report("VIBRIX: kernel Ethernet ARP responder verified");
    #[cfg(target_os = "none")]
    {
        secure_random::self_test().expect("secure random RDSEED self-test failed");
        report("VIBRIX: kernel secure random RDSEED verified");
    }
    process::self_test().expect("process lifecycle self-test failed");
    report("VIBRIX: kernel process lifecycle verified");
    config_policy::self_test().expect("portable configuration policy self-test failed");
    report("VIBRIX: kernel portable configuration policy verified");
    update_policy::self_test().expect("update policy model self-test failed");
    report("VIBRIX: kernel update policy model verified");
}
