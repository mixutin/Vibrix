//! Transport-neutral early device/driver matching model.
//!
//! Discovery produces immutable identity records. A registry may nominate a
//! driver family, but nomination is not activation: no BAR, MMIO, DMA,
//! interrupts, config writes or ownership transfer happens here.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PciAddress {
    pub segment: u16,
    pub bus: u8,
    pub device: u8,
    pub function: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PciIdentity {
    pub address: PciAddress,
    pub vendor: u16,
    pub device_id: u16,
    pub class: u8,
    pub subclass: u8,
    pub programming_interface: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceIdentity {
    Pci(PciIdentity),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DriverKind {
    Xhci,
    Rtl8168,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MatchRule {
    PciVendorDevice {
        vendor: u16,
        device_id: u16,
    },
    PciClass {
        class: u8,
        subclass: u8,
        programming_interface: u8,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DriverDescriptor {
    pub kind: DriverKind,
    pub name: &'static str,
    pub rule: MatchRule,
}

pub const DRIVERS: &[DriverDescriptor] = &[
    DriverDescriptor {
        kind: DriverKind::Rtl8168,
        name: "rtl8168",
        rule: MatchRule::PciVendorDevice {
            vendor: 0x10ec,
            device_id: 0x8168,
        },
    },
    DriverDescriptor {
        kind: DriverKind::Xhci,
        name: "xhci",
        rule: MatchRule::PciClass {
            class: 0x0c,
            subclass: 0x03,
            programming_interface: 0x30,
        },
    },
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Candidate {
    pub identity: DeviceIdentity,
    pub driver: DriverKind,
    pub driver_name: &'static str,
}

pub fn candidate(identity: DeviceIdentity) -> Option<Candidate> {
    let descriptor = DRIVERS
        .iter()
        .find(|descriptor| matches_rule(identity, descriptor.rule))?;
    Some(Candidate {
        identity,
        driver: descriptor.kind,
        driver_name: descriptor.name,
    })
}

fn matches_rule(identity: DeviceIdentity, rule: MatchRule) -> bool {
    match (identity, rule) {
        (DeviceIdentity::Pci(device), MatchRule::PciVendorDevice { vendor, device_id }) => {
            device.vendor == vendor && device.device_id == device_id
        }
        (
            DeviceIdentity::Pci(device),
            MatchRule::PciClass {
                class,
                subclass,
                programming_interface,
            },
        ) => {
            device.class == class
                && device.subclass == subclass
                && device.programming_interface == programming_interface
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DiscoverySummary {
    pub devices: u32,
    pub driver_candidates: u32,
    pub xhci_candidates: u32,
    pub rtl8168_candidates: u32,
}

impl DiscoverySummary {
    pub fn observe(&mut self, identity: DeviceIdentity) -> Option<Candidate> {
        self.devices = self.devices.saturating_add(1);
        let found = candidate(identity)?;
        self.driver_candidates = self.driver_candidates.saturating_add(1);
        match found.driver {
            DriverKind::Xhci => {
                self.xhci_candidates = self.xhci_candidates.saturating_add(1);
            }
            DriverKind::Rtl8168 => {
                self.rtl8168_candidates = self.rtl8168_candidates.saturating_add(1);
            }
        }
        Some(found)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pci(
        vendor: u16,
        device_id: u16,
        class: u8,
        subclass: u8,
        programming_interface: u8,
    ) -> DeviceIdentity {
        DeviceIdentity::Pci(PciIdentity {
            address: PciAddress {
                segment: 0,
                bus: 2,
                device: 3,
                function: 0,
            },
            vendor,
            device_id,
            class,
            subclass,
            programming_interface,
        })
    }

    #[test]
    fn exact_vendor_device_precedes_unrelated_class_matches() {
        let found = candidate(pci(0x10ec, 0x8168, 0x02, 0x00, 0x00)).unwrap();
        assert_eq!(found.driver, DriverKind::Rtl8168);
        assert_eq!(found.driver_name, "rtl8168");
    }

    #[test]
    fn xhci_is_matched_by_standard_pci_class_triplet() {
        for vendor in [0x1234, 0x1022, 0x8086] {
            let found = candidate(pci(vendor, 0x9999, 0x0c, 0x03, 0x30)).unwrap();
            assert_eq!(found.driver, DriverKind::Xhci);
        }
        assert_eq!(candidate(pci(0x1234, 1, 0x0c, 0x03, 0x20)), None);
        assert_eq!(candidate(pci(0x1234, 1, 0x0c, 0x04, 0x30)), None);
    }

    #[test]
    fn unknown_devices_remain_discovered_but_unbound() {
        let mut summary = DiscoverySummary::default();
        assert_eq!(summary.observe(pci(0x1af4, 0x1000, 0x02, 0x00, 0)), None);
        assert_eq!(
            summary,
            DiscoverySummary {
                devices: 1,
                driver_candidates: 0,
                xhci_candidates: 0,
                rtl8168_candidates: 0,
            }
        );
    }

    #[test]
    fn summary_counts_candidates_without_claiming_activation() {
        let mut summary = DiscoverySummary::default();
        let xhci = summary
            .observe(pci(0x1022, 0x43ee, 0x0c, 0x03, 0x30))
            .unwrap();
        let nic = summary.observe(pci(0x10ec, 0x8168, 0x02, 0x00, 0)).unwrap();
        assert_eq!(xhci.driver, DriverKind::Xhci);
        assert_eq!(nic.driver, DriverKind::Rtl8168);
        assert_eq!(
            summary,
            DiscoverySummary {
                devices: 2,
                driver_candidates: 2,
                xhci_candidates: 1,
                rtl8168_candidates: 1,
            }
        );
    }

    #[test]
    fn registry_names_are_unique_and_rules_do_not_duplicate() {
        for (index, driver) in DRIVERS.iter().enumerate() {
            assert!(!driver.name.is_empty());
            for earlier in &DRIVERS[..index] {
                assert_ne!(earlier.name, driver.name);
                assert_ne!(earlier.rule, driver.rule);
            }
        }
    }
}
