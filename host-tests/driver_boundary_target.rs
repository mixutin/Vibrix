#![cfg_attr(target_os = "none", no_std)]

#[path = "../kernel/src/device.rs"]
mod device;

pub fn aarch64_boundary_smoke() -> bool {
    let identity = device::DeviceIdentity::Pci(device::PciIdentity {
        address: device::PciAddress {
            segment: 0,
            bus: 0,
            device: 1,
            function: 0,
        },
        vendor: 0x1022,
        device_id: 0x43ee,
        class: 0x0c,
        subclass: 0x03,
        programming_interface: 0x30,
    });
    let mut binder = device::Binder::new();
    binder.bind_identity(identity).is_ok()
}

fn main() {}
