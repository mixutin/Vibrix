#![no_std]

#[cfg(test)]
extern crate std;

pub mod block;
pub mod config_policy;
pub mod cpu_topology;
pub mod credentials;
pub mod device;
#[path = "../../shared/gpt_identity.rs"]
pub mod gpt_identity;
pub mod ipv6;
pub mod klog;
pub mod local_ipc;
pub mod network_guard;
pub mod network_services;
pub mod nic;
pub mod ntp;
pub mod packet_filter;
pub mod path_policy;
pub mod per_cpu;
pub mod process;
pub mod process_syscalls;
pub mod resolver;
pub mod routing;
#[path = "../../shared/scsi.rs"]
pub mod scsi;
pub mod secure_random;
pub mod sync;
#[path = "../../shared/syscall_abi.rs"]
pub mod syscall_abi;
pub mod update_history;
pub mod update_policy;
pub mod update_staging;
pub mod usb_hid;
#[path = "../../shared/usb_mass_bulk.rs"]
pub mod usb_mass_bulk;
pub mod usb_storage;
pub mod user_image;
pub mod user_stack;
pub mod vfs;

/// QEMU-only caller supplies the real kernel's independent output paths.
/// A marker is emitted only after the corresponding behavior succeeds.
pub fn subsystem_self_test(mut report: impl FnMut(&str)) {
    vfs::self_test(&mut report).expect("bootstrap VFS self-test failed");
    klog::self_test().expect("structured kernel log self-test failed");
    report("VIBRIX: kernel structured log verified");
    local_ipc::self_test().expect("loopback/Unix datagram self-test failed");
    report("VIBRIX: kernel loopback and Unix datagrams verified");
    block::self_test().expect("block abstraction self-test failed");
    report("VIBRIX: kernel block abstraction verified");
    nic::self_test().expect("NIC abstraction self-test failed");
    report("VIBRIX: kernel NIC abstraction verified");
    ipv6::self_test().expect("IPv6/ICMPv6/NDP self-test failed");
    report("VIBRIX: kernel IPv6 ICMPv6 neighbor discovery verified");
    network_guard::self_test().expect("IPv4 ingress guard self-test failed");
    report("VIBRIX: kernel IPv4 ingress anti-spoofing guard verified");
    network_services::self_test().expect("network service policy self-test failed");
    report("VIBRIX: kernel network service default-off policy verified");
    packet_filter::self_test().expect("packet filter validation self-test failed");
    report("VIBRIX: kernel packet filter ruleset validation verified");
    sync::self_test().expect("synchronization primitives self-test failed");
    report("VIBRIX: kernel synchronization primitives verified");
    nic::ipv4_icmp_self_test().expect("IPv4/ICMP self-test failed");
    report("VIBRIX: kernel IPv4 ICMP echo verified");
    nic::udp_self_test().expect("UDP self-test failed");
    report("VIBRIX: kernel UDP datagram verified");
    nic::ethernet_arp_self_test().expect("Ethernet/ARP self-test failed");
    report("VIBRIX: kernel Ethernet ARP responder verified");
    credentials::self_test().expect("credential policy self-test failed");
    report("VIBRIX: kernel credential policy verified");
    usb_storage::self_test().expect("USB mass-storage descriptor policy self-test failed");
    report("VIBRIX: kernel USB mass-storage descriptor policy verified");
    #[cfg(target_os = "none")]
    {
        if secure_random::supported() {
            secure_random::self_test().expect("secure random RDSEED self-test failed");
            report("VIBRIX: kernel secure random RDSEED verified");
        } else {
            report("VIBRIX: kernel secure random unavailable on this CPU");
        }
    }
    nic::dhcp_self_test().expect("DHCPv4 client self-test failed");
    report("VIBRIX: kernel DHCPv4 client exchange verified");
    nic::dns_self_test().expect("DNS resolver self-test failed");
    report("VIBRIX: kernel DNS A resolver verified");
    resolver::self_test().expect("resolver config/cache self-test failed");
    report("VIBRIX: kernel resolver configuration and cache verified");
    ntp::self_test().expect("NTPv4 client discipline self-test failed");
    report("VIBRIX: kernel NTPv4 client discipline verified");
    nic::tcp_self_test().expect("TCP transport self-test failed");
    report("VIBRIX: kernel TCP active-open foundation verified");
    path_policy::self_test().expect("path visibility policy self-test failed");
    report("VIBRIX: kernel path visibility policy verified");
    process::self_test().expect("process lifecycle self-test failed");
    report("VIBRIX: kernel process lifecycle verified");
    report("VIBRIX: kernel process promises verified");
    config_policy::self_test().expect("portable configuration policy self-test failed");
    report("VIBRIX: kernel portable configuration policy verified");
    update_history::self_test().expect("update history self-test failed");
    report("VIBRIX: kernel update history and rollback selection verified");
    update_policy::self_test().expect("update policy model self-test failed");
    report("VIBRIX: kernel update policy model verified");
    update_staging::self_test().expect("transactional update staging self-test failed");
    report("VIBRIX: kernel transactional update staging verified");
}
