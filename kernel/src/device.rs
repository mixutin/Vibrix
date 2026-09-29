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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Binding {
    pub identity: DeviceIdentity,
    pub driver: DriverKind,
    pub driver_name: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BindError {
    NoDriver,
    AlreadyBound,
    Full,
}

pub const MAX_EARLY_BINDINGS: usize = 16;

pub struct Binder {
    bindings: [Option<Binding>; MAX_EARLY_BINDINGS],
    used: usize,
}

impl Binder {
    pub const fn new() -> Self {
        Self {
            bindings: [None; MAX_EARLY_BINDINGS],
            used: 0,
        }
    }

    pub fn bind_identity(&mut self, identity: DeviceIdentity) -> Result<Binding, BindError> {
        let candidate = candidate(identity).ok_or(BindError::NoDriver)?;
        self.bind(candidate)
    }

    pub fn bind(&mut self, candidate: Candidate) -> Result<Binding, BindError> {
        if self.bindings[..self.used]
            .iter()
            .flatten()
            .any(|binding| binding.identity == candidate.identity)
        {
            return Err(BindError::AlreadyBound);
        }
        if self.used == self.bindings.len() {
            return Err(BindError::Full);
        }
        let binding = Binding {
            identity: candidate.identity,
            driver: candidate.driver,
            driver_name: candidate.driver_name,
        };
        self.bindings[self.used] = Some(binding);
        self.used += 1;
        Ok(binding)
    }

    pub fn len(&self) -> usize {
        self.used
    }

    pub fn count_driver(&self, driver: DriverKind) -> usize {
        self.bindings[..self.used]
            .iter()
            .flatten()
            .filter(|binding| binding.driver == driver)
            .count()
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompatibilityClass {
    DriverCandidate,
    MissingDriver,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompatibilityQuirk {
    XhciContext32Only,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompatibilityObservation {
    pub class: CompatibilityClass,
    pub driver_name: Option<&'static str>,
    pub quirk: Option<CompatibilityQuirk>,
}

pub fn compatibility(identity: DeviceIdentity) -> CompatibilityObservation {
    match candidate(identity) {
        Some(found) => CompatibilityObservation {
            class: CompatibilityClass::DriverCandidate,
            driver_name: Some(found.driver_name),
            quirk: match found.driver {
                DriverKind::Xhci => Some(CompatibilityQuirk::XhciContext32Only),
                DriverKind::Rtl8168 => None,
            },
        },
        None => CompatibilityObservation {
            class: CompatibilityClass::MissingDriver,
            driver_name: None,
            quirk: None,
        },
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CompatibilitySummary {
    pub discovered: u32,
    pub driver_candidates: u32,
    pub missing_driver: u32,
    pub limited_by_known_quirk: u32,
}

impl CompatibilitySummary {
    pub fn observe(&mut self, identity: DeviceIdentity) -> CompatibilityObservation {
        self.discovered = self.discovered.saturating_add(1);
        let observation = compatibility(identity);
        match observation.class {
            CompatibilityClass::DriverCandidate => {
                self.driver_candidates = self.driver_candidates.saturating_add(1);
            }
            CompatibilityClass::MissingDriver => {
                self.missing_driver = self.missing_driver.saturating_add(1);
            }
        }
        if observation.quirk.is_some() {
            self.limited_by_known_quirk = self.limited_by_known_quirk.saturating_add(1);
        }
        observation
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DiagnosticSummary {
    pub discovered: u32,
    pub candidates: u32,
    pub bound: u32,
    pub missing_driver: u32,
    pub binding_failures: u32,
}

impl DiagnosticSummary {
    pub fn new(discovery: DiscoverySummary, binder: &Binder, binding_failures: u32) -> Self {
        let bound = u32::try_from(binder.len()).unwrap_or(u32::MAX);
        Self {
            discovered: discovery.devices,
            candidates: discovery.driver_candidates,
            bound,
            missing_driver: discovery
                .devices
                .saturating_sub(discovery.driver_candidates),
            binding_failures: binding_failures
                .max(discovery.driver_candidates.saturating_sub(bound)),
        }
    }

    pub const fn healthy(self) -> bool {
        self.binding_failures == 0
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
    fn binder_establishes_exclusive_device_to_driver_ownership() {
        let identity = pci(0x1022, 0x43ee, 0x0c, 0x03, 0x30);
        let mut binder = Binder::new();
        let binding = binder.bind_identity(identity).unwrap();
        assert_eq!(binding.driver, DriverKind::Xhci);
        assert_eq!(binding.identity, identity);
        assert_eq!(binder.len(), 1);
        assert_eq!(binder.count_driver(DriverKind::Xhci), 1);
        assert_eq!(binder.bind_identity(identity), Err(BindError::AlreadyBound));
    }

    #[test]
    fn binder_rejects_unknown_devices_and_capacity_overflow() {
        let mut binder = Binder::new();
        assert_eq!(
            binder.bind_identity(pci(0x1af4, 0x1000, 0x02, 0, 0)),
            Err(BindError::NoDriver)
        );
        for function in 0..MAX_EARLY_BINDINGS {
            let identity = DeviceIdentity::Pci(PciIdentity {
                address: PciAddress {
                    segment: 0,
                    bus: 0,
                    device: (function / 8) as u8,
                    function: (function % 8) as u8,
                },
                vendor: 0x1234,
                device_id: function as u16,
                class: 0x0c,
                subclass: 0x03,
                programming_interface: 0x30,
            });
            binder.bind_identity(identity).unwrap();
        }
        assert_eq!(binder.len(), MAX_EARLY_BINDINGS);
        assert_eq!(
            binder.bind_identity(DeviceIdentity::Pci(PciIdentity {
                address: PciAddress {
                    segment: 0,
                    bus: 0,
                    device: 3,
                    function: 0,
                },
                vendor: 0x1234,
                device_id: 0xffff,
                class: 0x0c,
                subclass: 0x03,
                programming_interface: 0x30,
            })),
            Err(BindError::Full)
        );
    }

    #[test]
    fn compatibility_reporting_distinguishes_candidates_missing_drivers_and_limits() {
        let xhci = pci(0x1234, 0x11e8, 0x0c, 0x03, 0x30);
        let rtl = pci(0x10ec, 0x8168, 0x02, 0x00, 0x00);
        let unknown = pci(0x1af4, 0x1000, 0x02, 0x00, 0x00);

        assert_eq!(
            compatibility(xhci),
            CompatibilityObservation {
                class: CompatibilityClass::DriverCandidate,
                driver_name: Some("xhci"),
                quirk: Some(CompatibilityQuirk::XhciContext32Only),
            }
        );
        assert_eq!(
            compatibility(rtl),
            CompatibilityObservation {
                class: CompatibilityClass::DriverCandidate,
                driver_name: Some("rtl8168"),
                quirk: None,
            }
        );
        assert_eq!(
            compatibility(unknown),
            CompatibilityObservation {
                class: CompatibilityClass::MissingDriver,
                driver_name: None,
                quirk: None,
            }
        );

        let mut summary = CompatibilitySummary::default();
        summary.observe(xhci);
        summary.observe(rtl);
        summary.observe(unknown);
        assert_eq!(
            summary,
            CompatibilitySummary {
                discovered: 3,
                driver_candidates: 2,
                missing_driver: 1,
                limited_by_known_quirk: 1,
            }
        );
    }

    #[test]
    fn diagnostics_distinguish_missing_drivers_from_binding_failures() {
        let mut discovery = DiscoverySummary::default();
        let xhci_identity = pci(0x1022, 0x43ee, 0x0c, 0x03, 0x30);
        let unknown_identity = pci(0x1af4, 0x1000, 0x02, 0x00, 0x00);
        discovery.observe(xhci_identity);
        discovery.observe(unknown_identity);

        let mut binder = Binder::new();
        binder.bind_identity(xhci_identity).unwrap();
        assert_eq!(
            DiagnosticSummary::new(discovery, &binder, 0),
            DiagnosticSummary {
                discovered: 2,
                candidates: 1,
                bound: 1,
                missing_driver: 1,
                binding_failures: 0,
            }
        );
        assert!(DiagnosticSummary::new(discovery, &binder, 0).healthy());

        let failed = DiagnosticSummary::new(discovery, &Binder::new(), 1);
        assert_eq!(failed.bound, 0);
        assert_eq!(failed.missing_driver, 1);
        assert_eq!(failed.binding_failures, 1);
        assert!(!failed.healthy());
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
