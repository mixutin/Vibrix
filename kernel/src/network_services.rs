//! Deny-by-default startup policy for network-facing services.
//!
//! This is a control-plane primitive. A service may start only after an
//! explicit enable decision has been recorded for that exact service.

pub const SERVICE_COUNT: usize = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Invariant,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Service {
    Ssh = 0,
    Resolver = 1,
    Ntp = 2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Policy {
    enabled: [bool; SERVICE_COUNT],
}

impl Policy {
    /// Fresh boots/configurations start with every network-facing service off.
    pub const fn new() -> Self {
        Self {
            enabled: [false; SERVICE_COUNT],
        }
    }

    pub fn set_enabled(&mut self, service: Service, enabled: bool) {
        self.enabled[service as usize] = enabled;
    }

    /// Startup gates must consult this before binding/listening.
    pub fn may_start(&self, service: Service) -> bool {
        self.enabled[service as usize]
    }
}

impl Default for Policy {
    fn default() -> Self {
        Self::new()
    }
}

pub fn self_test() -> Result<(), Error> {
    let mut policy = Policy::new();
    for service in [Service::Ssh, Service::Resolver, Service::Ntp] {
        if policy.may_start(service) {
            return Err(Error::Invariant);
        }
    }

    policy.set_enabled(Service::Resolver, true);
    if !policy.may_start(Service::Resolver)
        || policy.may_start(Service::Ssh)
        || policy.may_start(Service::Ntp)
    {
        return Err(Error::Invariant);
    }

    policy.set_enabled(Service::Resolver, false);
    if policy.may_start(Service::Resolver) {
        return Err(Error::Invariant);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_services_are_disabled_by_default() {
        let policy = Policy::new();
        assert!(!policy.may_start(Service::Ssh));
        assert!(!policy.may_start(Service::Resolver));
        assert!(!policy.may_start(Service::Ntp));
    }

    #[test]
    fn enablement_is_exact_and_reversible() {
        let mut policy = Policy::new();
        policy.set_enabled(Service::Ssh, true);
        assert!(policy.may_start(Service::Ssh));
        assert!(!policy.may_start(Service::Resolver));
        assert!(!policy.may_start(Service::Ntp));

        policy.set_enabled(Service::Ssh, false);
        assert!(!policy.may_start(Service::Ssh));
    }

    #[test]
    fn production_self_test_passes() {
        self_test().unwrap();
    }
}
