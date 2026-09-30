//! Fixed-capacity IPv4 packet-filter policy and transactional ruleset control.
//!
//! Rules are validated before publication. The bounded dataplane applies
//! first-match policy, supports an explicit default action, and remembers
//! accepted TCP/UDP flows so reverse traffic can pass without a second rule.
//! There is no allocation, NAT, fragment reassembly, timeout clock or NIC hook.

pub const MAX_RULES: usize = 32;
pub const MAX_STATES: usize = 16;

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
pub enum DefaultPolicy {
    Pass,
    Block,
}

impl DefaultPolicy {
    const fn action(self) -> Action {
        match self {
            Self::Pass => Action::Pass,
            Self::Block => Action::Block,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Packet {
    pub protocol: Protocol,
    pub source: u32,
    pub destination: u32,
    pub source_port: u16,
    pub destination_port: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Flow {
    protocol: Protocol,
    source: u32,
    destination: u32,
    source_port: u16,
    destination_port: u16,
}

impl Flow {
    const fn from_packet(packet: Packet) -> Self {
        Self {
            protocol: packet.protocol,
            source: packet.source,
            destination: packet.destination,
            source_port: packet.source_port,
            destination_port: packet.destination_port,
        }
    }

    fn matches(self, packet: Packet) -> bool {
        self.protocol == packet.protocol
            && ((self.source == packet.source
                && self.destination == packet.destination
                && self.source_port == packet.source_port
                && self.destination_port == packet.destination_port)
                || (self.source == packet.destination
                    && self.destination == packet.source
                    && self.source_port == packet.destination_port
                    && self.destination_port == packet.source_port))
    }
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StatefulFilter {
    control: FilterControl,
    default_policy: DefaultPolicy,
    states: [Option<Flow>; MAX_STATES],
    next_state: usize,
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

fn prefix_matches(address: u32, prefix: u32, len: u8) -> bool {
    if len == 0 {
        return true;
    }
    let mask = u32::MAX << (32 - len);
    address & mask == prefix
}

fn rule_matches(rule: Rule, packet: Packet) -> bool {
    if rule.protocol != Protocol::Any && rule.protocol != packet.protocol {
        return false;
    }
    if !prefix_matches(packet.source, rule.source_prefix, rule.source_prefix_len)
        || !prefix_matches(
            packet.destination,
            rule.destination_prefix,
            rule.destination_prefix_len,
        )
    {
        return false;
    }
    match packet.protocol {
        Protocol::Tcp | Protocol::Udp if rule.protocol != Protocol::Any => {
            (rule.port_start..=rule.port_end).contains(&packet.destination_port)
        }
        _ => true,
    }
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

impl StatefulFilter {
    pub const fn new(default_policy: DefaultPolicy) -> Self {
        Self {
            control: FilterControl::new(),
            default_policy,
            states: [None; MAX_STATES],
            next_state: 0,
        }
    }

    pub const fn default_policy(&self) -> DefaultPolicy {
        self.default_policy
    }

    pub fn set_default_policy(&mut self, policy: DefaultPolicy) {
        self.default_policy = policy;
    }

    pub const fn active(&self) -> Option<ActiveRuleset> {
        self.control.active()
    }

    /// Validate and publish a ruleset atomically. Successful replacement
    /// invalidates remembered flow state so old policy cannot authorize traffic
    /// under the new generation. Failed validation preserves both.
    pub fn validate_and_activate(&mut self, rules: &[Rule]) -> Result<u64, Error> {
        let generation = self.control.validate_and_activate(rules)?;
        self.states = [None; MAX_STATES];
        self.next_state = 0;
        Ok(generation)
    }

    pub fn state_count(&self) -> usize {
        self.states.iter().filter(|state| state.is_some()).count()
    }

    fn established(&self, packet: Packet) -> bool {
        matches!(packet.protocol, Protocol::Tcp | Protocol::Udp)
            && self.states.iter().flatten().any(|flow| flow.matches(packet))
    }

    fn remember(&mut self, packet: Packet) {
        if !matches!(packet.protocol, Protocol::Tcp | Protocol::Udp) {
            return;
        }
        if self.established(packet) {
            return;
        }
        if let Some(slot) = self.states.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(Flow::from_packet(packet));
            return;
        }
        self.states[self.next_state] = Some(Flow::from_packet(packet));
        self.next_state = (self.next_state + 1) % MAX_STATES;
    }

    /// Evaluate one packet. Existing TCP/UDP state is checked first; otherwise
    /// the active ruleset is first-match and the explicit default policy is used
    /// when no rule matches.
    pub fn evaluate(&mut self, packet: Packet) -> Action {
        if self.established(packet) {
            return Action::Pass;
        }

        let action = self
            .control
            .active
            .and_then(|active| {
                active
                    .ruleset
                    .rules()
                    .iter()
                    .copied()
                    .find(|rule| rule_matches(*rule, packet))
                    .map(|rule| rule.action)
            })
            .unwrap_or_else(|| self.default_policy.action());

        if action == Action::Pass {
            self.remember(packet);
        }
        action
    }
}

impl Default for StatefulFilter {
    fn default() -> Self {
        Self::new(DefaultPolicy::Block)
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

    let allow_https = [Rule {
        action: Action::Pass,
        protocol: Protocol::Tcp,
        source_prefix: 0,
        source_prefix_len: 0,
        destination_prefix: 0xcb007100,
        destination_prefix_len: 24,
        port_start: 443,
        port_end: 443,
    }];
    let request = Packet {
        protocol: Protocol::Tcp,
        source: 0xc000020a,
        destination: 0xcb007107,
        source_port: 49152,
        destination_port: 443,
    };
    let reply = Packet {
        protocol: Protocol::Tcp,
        source: request.destination,
        destination: request.source,
        source_port: request.destination_port,
        destination_port: request.source_port,
    };
    let mut filter = StatefulFilter::new(DefaultPolicy::Block);
    filter.validate_and_activate(&allow_https)?;
    if filter.evaluate(request) != Action::Pass
        || filter.evaluate(reply) != Action::Pass
        || filter.state_count() != 1
    {
        return Err(Error::InvalidPortRange);
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
    fn default_deny_first_match_and_stateful_reverse_flow_are_enforced() {
        let mut filter = StatefulFilter::new(DefaultPolicy::Block);
        let rules = [
            Rule {
                action: Action::Block,
                protocol: Protocol::Tcp,
                source_prefix: 0xc0000200,
                source_prefix_len: 24,
                destination_prefix: 0xcb007100,
                destination_prefix_len: 24,
                port_start: 22,
                port_end: 22,
            },
            base_rule(),
        ];
        filter.validate_and_activate(&rules).unwrap();

        let ssh = Packet {
            protocol: Protocol::Tcp,
            source: 0xc000020a,
            destination: 0xcb007107,
            source_port: 50000,
            destination_port: 22,
        };
        assert_eq!(filter.evaluate(ssh), Action::Block);

        let https = Packet {
            destination_port: 443,
            ..ssh
        };
        assert_eq!(filter.evaluate(https), Action::Pass);
        assert_eq!(filter.state_count(), 1);

        let reply = Packet {
            protocol: Protocol::Tcp,
            source: https.destination,
            destination: https.source,
            source_port: 443,
            destination_port: https.source_port,
        };
        assert_eq!(filter.evaluate(reply), Action::Pass);

        let unrelated = Packet {
            protocol: Protocol::Udp,
            source: 0xc000020a,
            destination: 0xcb007107,
            source_port: 50001,
            destination_port: 53,
        };
        assert_eq!(filter.evaluate(unrelated), Action::Block);
    }

    #[test]
    fn successful_policy_replacement_flushes_state_but_failed_update_does_not() {
        let mut filter = StatefulFilter::new(DefaultPolicy::Block);
        let allow = [base_rule()];
        filter.validate_and_activate(&allow).unwrap();
        let packet = Packet {
            protocol: Protocol::Tcp,
            source: 0xc000020a,
            destination: 0xcb007107,
            source_port: 50000,
            destination_port: 443,
        };
        assert_eq!(filter.evaluate(packet), Action::Pass);
        assert_eq!(filter.state_count(), 1);

        let invalid = [Rule {
            destination_prefix: 0xcb007101,
            ..base_rule()
        }];
        assert_eq!(
            filter.validate_and_activate(&invalid),
            Err(Error::NonCanonicalPrefix)
        );
        assert_eq!(filter.state_count(), 1);

        let block_all = [Rule {
            action: Action::Block,
            protocol: Protocol::Any,
            source_prefix: 0,
            source_prefix_len: 0,
            destination_prefix: 0,
            destination_prefix_len: 0,
            port_start: 0,
            port_end: 0,
        }];
        filter.validate_and_activate(&block_all).unwrap();
        assert_eq!(filter.state_count(), 0);
        assert_eq!(filter.evaluate(packet), Action::Block);
    }

    #[test]
    fn production_self_test_passes() {
        self_test().unwrap();
    }
}
