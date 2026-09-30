//! Monotonic per-process path visibility/access allow-list policy.
//!
//! The policy is allocation-free and fail-closed. A process begins unrestricted.
//! Its first restriction installs a bounded absolute-prefix allow-list; later
//! restrictions may only remove entries or rights. Prefix matching is component
//! aware, so "/tmp" does not authorize "/tmp2".

pub const MAX_PATH_RULES: usize = 8;
pub const MAX_PATH_BYTES: usize = 64;

pub const ACCESS_READ: u8 = 1 << 0;
pub const ACCESS_WRITE: u8 = 1 << 1;
pub const ACCESS_ALL: u8 = ACCESS_READ | ACCESS_WRITE;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    TooManyRules,
    InvalidPath,
    InvalidRights,
    Expansion,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rule {
    path: [u8; MAX_PATH_BYTES],
    len: u8,
    rights: u8,
}

impl Rule {
    pub const EMPTY: Self = Self {
        path: [0; MAX_PATH_BYTES],
        len: 0,
        rights: 0,
    };

    pub fn new(path: &[u8], rights: u8) -> Result<Self, Error> {
        if path.is_empty()
            || path[0] != b'/'
            || path.len() > MAX_PATH_BYTES
            || rights == 0
            || rights & !ACCESS_ALL != 0
            || path.contains(&0)
            || (path.len() > 1 && path.ends_with(b"/"))
        {
            return Err(if rights == 0 || rights & !ACCESS_ALL != 0 {
                Error::InvalidRights
            } else {
                Error::InvalidPath
            });
        }
        let mut rule = Self::EMPTY;
        rule.path[..path.len()].copy_from_slice(path);
        rule.len = path.len() as u8;
        rule.rights = rights;
        Ok(rule)
    }

    pub fn path(&self) -> &[u8] {
        &self.path[..usize::from(self.len)]
    }

    pub const fn rights(&self) -> u8 {
        self.rights
    }

    fn matches(&self, path: &[u8]) -> bool {
        let prefix = self.path();
        if prefix == b"/" {
            return path.starts_with(b"/");
        }
        path == prefix
            || (path.len() > prefix.len()
                && path.starts_with(prefix)
                && path[prefix.len()] == b'/')
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Policy {
    rules: [Rule; MAX_PATH_RULES],
    len: u8,
    restricted: bool,
}

impl Policy {
    pub const fn unrestricted() -> Self {
        Self {
            rules: [Rule::EMPTY; MAX_PATH_RULES],
            len: 0,
            restricted: false,
        }
    }

    pub const fn restricted(&self) -> bool {
        self.restricted
    }

    pub fn rules(&self) -> &[Rule] {
        &self.rules[..usize::from(self.len)]
    }

    pub fn permits(&self, path: &[u8], rights: u8) -> bool {
        if path.is_empty()
            || path[0] != b'/'
            || rights == 0
            || rights & !ACCESS_ALL != 0
        {
            return false;
        }
        if !self.restricted {
            return true;
        }
        self.rules()
            .iter()
            .any(|rule| rule.rights() & rights == rights && rule.matches(path))
    }

    pub fn restrict(&mut self, rules: &[Rule]) -> Result<(), Error> {
        if rules.len() > MAX_PATH_RULES {
            return Err(Error::TooManyRules);
        }
        if self.restricted {
            for candidate in rules {
                let authorized = self.rules().iter().any(|current| {
                    current.matches(candidate.path())
                        && current.rights() & candidate.rights() == candidate.rights()
                });
                if !authorized {
                    return Err(Error::Expansion);
                }
            }
        }

        let mut next = [Rule::EMPTY; MAX_PATH_RULES];
        next[..rules.len()].copy_from_slice(rules);
        self.rules = next;
        self.len = rules.len() as u8;
        self.restricted = true;
        Ok(())
    }
}

pub fn self_test() -> Result<(), Error> {
    let tmp = Rule::new(b"/tmp", ACCESS_READ | ACCESS_WRITE)?;
    let etc = Rule::new(b"/etc", ACCESS_READ)?;
    let mut policy = Policy::unrestricted();
    policy.restrict(&[tmp, etc])?;
    if !policy.permits(b"/tmp/file", ACCESS_WRITE)
        || !policy.permits(b"/etc/config", ACCESS_READ)
        || policy.permits(b"/etc/config", ACCESS_WRITE)
        || policy.permits(b"/tmp2", ACCESS_READ)
    {
        return Err(Error::Expansion);
    }
    let read_tmp = Rule::new(b"/tmp", ACCESS_READ)?;
    policy.restrict(&[read_tmp])?;
    if policy.permits(b"/tmp/file", ACCESS_WRITE)
        || policy.restrict(&[tmp]) != Err(Error::Expansion)
    {
        return Err(Error::Expansion);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_aware_prefixes_do_not_alias() {
        let mut policy = Policy::unrestricted();
        policy
            .restrict(&[Rule::new(b"/tmp", ACCESS_READ).unwrap()])
            .unwrap();
        assert!(policy.permits(b"/tmp", ACCESS_READ));
        assert!(policy.permits(b"/tmp/a", ACCESS_READ));
        assert!(!policy.permits(b"/tmp2", ACCESS_READ));
        assert!(!policy.permits(b"/tm", ACCESS_READ));
    }

    #[test]
    fn restrictions_only_shrink() {
        let mut policy = Policy::unrestricted();
        let broad = Rule::new(b"/home/user", ACCESS_ALL).unwrap();
        policy.restrict(&[broad]).unwrap();
        let narrow = Rule::new(b"/home/user/docs", ACCESS_READ).unwrap();
        policy.restrict(&[narrow]).unwrap();
        assert!(policy.permits(b"/home/user/docs/a", ACCESS_READ));
        assert!(!policy.permits(b"/home/user/docs/a", ACCESS_WRITE));
        assert_eq!(policy.restrict(&[broad]), Err(Error::Expansion));
    }

    #[test]
    fn invalid_rules_fail_closed() {
        assert_eq!(Rule::new(b"relative", ACCESS_READ), Err(Error::InvalidPath));
        assert_eq!(Rule::new(b"/tmp/", ACCESS_READ), Err(Error::InvalidPath));
        assert_eq!(Rule::new(b"/tmp", 0), Err(Error::InvalidRights));
        assert_eq!(Rule::new(b"/tmp", 0x80), Err(Error::InvalidRights));
    }

    #[test]
    fn production_self_test_passes() {
        self_test().unwrap();
    }
}
