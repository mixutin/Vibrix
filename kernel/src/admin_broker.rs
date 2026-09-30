//! Least-privilege administrative command broker policy.
//!
//! Administrative authority is represented as explicit action bits. Broker
//! grants can only be attenuated; no operation receives ambient authority merely
//! because some other administrative action was granted.

pub const AUTH_SERVICE: u32 = 1 << 0;
pub const AUTH_UPDATE: u32 = 1 << 1;
pub const AUTH_NETWORK: u32 = 1 << 2;
pub const AUTH_ACCOUNTS: u32 = 1 << 3;
pub const AUTH_STORAGE: u32 = 1 << 4;
pub const AUTH_ALL: u32 =
    AUTH_SERVICE | AUTH_UPDATE | AUTH_NETWORK | AUTH_ACCOUNTS | AUTH_STORAGE;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    InvalidAuthority,
    Expansion,
    Denied,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    ServiceControl,
    SystemUpdate,
    NetworkConfigure,
    AccountManage,
    StorageRepair,
}

impl Action {
    const fn required(self) -> u32 {
        match self {
            Self::ServiceControl => AUTH_SERVICE,
            Self::SystemUpdate => AUTH_UPDATE,
            Self::NetworkConfigure => AUTH_NETWORK,
            Self::AccountManage => AUTH_ACCOUNTS,
            Self::StorageRepair => AUTH_STORAGE,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Grant {
    authority: u32,
}

impl Grant {
    pub fn new(authority: u32) -> Result<Self, Error> {
        if authority & !AUTH_ALL != 0 {
            return Err(Error::InvalidAuthority);
        }
        Ok(Self { authority })
    }

    pub const fn authority(self) -> u32 {
        self.authority
    }

    pub fn permits(self, action: Action) -> bool {
        self.authority & action.required() == action.required()
    }

    pub fn attenuate(&mut self, authority: u32) -> Result<(), Error> {
        if authority & !AUTH_ALL != 0 {
            return Err(Error::InvalidAuthority);
        }
        if authority & !self.authority != 0 {
            return Err(Error::Expansion);
        }
        self.authority = authority;
        Ok(())
    }

    pub fn authorize(self, action: Action) -> Result<(), Error> {
        if self.permits(action) {
            Ok(())
        } else {
            Err(Error::Denied)
        }
    }
}

pub fn self_test() -> Result<(), Error> {
    let mut grant = Grant::new(AUTH_SERVICE | AUTH_UPDATE)?;
    grant.authorize(Action::ServiceControl)?;
    grant.authorize(Action::SystemUpdate)?;
    if grant.authorize(Action::NetworkConfigure) != Err(Error::Denied) {
        return Err(Error::Denied);
    }
    grant.attenuate(AUTH_SERVICE)?;
    if grant.authorize(Action::SystemUpdate) != Err(Error::Denied)
        || grant.attenuate(AUTH_ALL) != Err(Error::Expansion)
    {
        return Err(Error::Expansion);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_action_requires_only_its_explicit_authority() {
        let service = Grant::new(AUTH_SERVICE).unwrap();
        assert!(service.permits(Action::ServiceControl));
        assert!(!service.permits(Action::SystemUpdate));
        assert!(!service.permits(Action::NetworkConfigure));
        assert!(!service.permits(Action::AccountManage));
        assert!(!service.permits(Action::StorageRepair));

        let network = Grant::new(AUTH_NETWORK).unwrap();
        assert!(network.permits(Action::NetworkConfigure));
        assert!(!network.permits(Action::ServiceControl));
    }

    #[test]
    fn grants_can_only_be_attenuated() {
        let mut grant = Grant::new(AUTH_SERVICE | AUTH_UPDATE).unwrap();
        grant.attenuate(AUTH_SERVICE).unwrap();
        assert_eq!(grant.authority(), AUTH_SERVICE);
        assert_eq!(
            grant.attenuate(AUTH_SERVICE | AUTH_UPDATE),
            Err(Error::Expansion)
        );
        assert_eq!(grant.authority(), AUTH_SERVICE);
    }

    #[test]
    fn unknown_authority_bits_fail_closed() {
        assert_eq!(Grant::new(1 << 31), Err(Error::InvalidAuthority));
        let mut grant = Grant::new(AUTH_ALL).unwrap();
        assert_eq!(grant.attenuate(1 << 31), Err(Error::InvalidAuthority));
        assert_eq!(grant.authority(), AUTH_ALL);
    }

    #[test]
    fn production_self_test_passes() {
        self_test().unwrap();
    }
}
