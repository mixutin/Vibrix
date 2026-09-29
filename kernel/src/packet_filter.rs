//! Transactional validation gate for future packet-filter rulesets.
//!
//! No packet filtering is performed here. This module guarantees that a
//! malformed or over-capacity ruleset cannot replace the active validated
//! generation.

pub const MAX_RULES: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    Pass,
    Block,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Protocol {
    Any,
    Tcp,
    Udp,
    Icmp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rule {
    pub action: Action,
    pub protocol: Protocol,
    pub source_prefix: u32,
    pub source_prefix_len: u8,
    pub destination_prefix: u32,
    pub destination_prefix_len: u8,
    pub port_start: u16,
    pub port_end: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    TooManyRules,
    InvalidPrefix,
    NonCanonicalPrefix,
    InvalidPortRange,
    PortWithUnsupportedProtocol,
    EmptyRuleset,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValidatedRuleset {
    rules: [Rule; MAX_RULES],
    len: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActiveRuleset {
    generation: u64,
    ruleset: ValidatedRuleset,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FilterControl {
    active: Option<ActiveRuleset>,
    next_generation: u64,
}

const EMPTY_RULE: Rule = Rule {
    action: Action::Block,
    protocol: Protocol::Any,
    source_prefix: 0,
    source_prefix_len: 0,
    destination_prefix: 0,
    destination_prefix_len: 0,
    port_start: 0,
    port_end: 0,
};

fn canonical(prefix: u32, len: u8) -> bool {
    if len > 32 {
        return false;
    }
    if len == 0 {
        return prefix == 0;
    }
    let mask = u32::MAX << (32 - len);
    prefix & !mask == 0
}

impl ValidatedRuleset {
    pub fn validate(rules: &[Rule]) -> Result<Self, Error> {
        if rules.is_empty() {
            return Err(Error::EmptyRuleset);
        }
        if rules.len() > MAX_RULES {
            return Err(Error::TooManyRules);
        }
        let mut out = Self {
            rules: [EMPTY_RULE; MAX_RULES],
            len: rules.len() as u8,
        };
        for (index, rule) in rules.iter().copied().enumerate() {
            if rule.source_prefix_len > 32 || rule.destination_prefix_len > 32 {
                return Err(Error::InvalidPrefix);
            }
            if !canonical(rule.source_prefix, rule.source_prefix_len)
                || !canonical(rule.destination_prefix, rule.destination_prefix_len)
            {
                return Err(Error::NonCanonicalPrefix);
            }
            if rule.port_start > rule.port_end {
                return Err(Error::InvalidPortRange);
            }
            if matches!(rule.protocol, Protocol::Any | Protocol::Icmp)
                && (rule.port_start != 0 || rule.port_end != 0)
            {
                return Err(Error::PortWithUnsupportedProtocol);
            }
            out.rules[index] = rule;
        }
        Ok(out)
    }

    pub fn rules(&self) -> &[Rule] {
        &self.rules[..usize::from(self.len)]
    }
}

impl FilterControl {
    pub const fn new() -> Self {
        Self {
            active: None,
            next_generation: 1,
        }
    }

    pub const fn active(&self) -> Option<ActiveRuleset> {
        self.active
    }

    pub fn validate_and_activate(&mut self, rules: &[Rule]) -> Result<u64, Error> {
        let validated = ValidatedRuleset::validate(rules)?;
        let generation = self.next_generation;
        self.next_generation = self.next_generation.saturating_add(1);
        self.active = Some(ActiveRuleset {
            generation,
            ruleset: validated,
        });
        Ok(generation)
    }
}

impl Default for FilterControl {
    fn default() -> Self {
        Self::new()
    }
}

pub fn self_test() -> Result<(), Error> {
    let valid = [Rule {
        action: Action::Block,
        protocol: Protocol::Tcp,
        source_prefix: 0,
        source_prefix_len: 0,
        destination_prefix: 0xc0000200,
        destination_prefix_len: 24,
        port_start: 22,
        port_end: 22,
    }];
    let invalid = [Rule {
        destination_prefix: 0xc0000201,
        ..valid[0]
    }];

    let mut control = FilterControl::new();
    let first = control.validate_and_activate(&valid)?;
    let before = control.active();
    if control.validate_and_activate(&invalid) != Err(Error::NonCanonicalPrefix) {
        return Err(Error::NonCanonicalPrefix);
    }
    if control.active() != before || first != 1 {
        return Err(Error::InvalidPrefix);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_rule() -> Rule {
        Rule {
            action: Action::Pass,
            protocol: Protocol::Tcp,
            source_prefix: 0,
            source_prefix_len: 0,
            destination_prefix: 0xcb007100,
            destination_prefix_len: 24,
            port_start: 443,
            port_end: 443,
        }
    }

    #[test]
    fn invalid_rulesets_never_replace_active_generation() {
        let mut control = FilterControl::new();
        let rule = base_rule();
        assert_eq!(control.validate_and_activate(&[rule]), Ok(1));
        let before = control.active();

        let invalid = Rule {
            destination_prefix: 0xcb007101,
            ..rule
        };
        assert_eq!(
            control.validate_and_activate(&[invalid]),
            Err(Error::NonCanonicalPrefix)
        );
        assert_eq!(control.active(), before);
    }

    #[test]
    fn validates_capacity_prefixes_ports_and_protocols() {
        let rule = base_rule();
        assert!(ValidatedRuleset::validate(&[rule]).is_ok());

        let mut too_many = [EMPTY_RULE; MAX_RULES + 1];
        too_many.fill(rule);
        assert_eq!(
            ValidatedRuleset::validate(&too_many),
            Err(Error::TooManyRules)
        );

        assert_eq!(
            ValidatedRuleset::validate(&[Rule {
                source_prefix_len: 33,
                ..rule
            }]),
            Err(Error::InvalidPrefix)
        );
        assert_eq!(
            ValidatedRuleset::validate(&[Rule {
                port_start: 10,
                port_end: 9,
                ..rule
            }]),
            Err(Error::InvalidPortRange)
        );
        assert_eq!(
            ValidatedRuleset::validate(&[Rule {
                protocol: Protocol::Icmp,
                port_start: 1,
                port_end: 1,
                ..rule
            }]),
            Err(Error::PortWithUnsupportedProtocol)
        );
    }

    #[test]
    fn production_self_test_passes() {
        self_test().unwrap();
    }
}
