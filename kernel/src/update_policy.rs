//! Executable update/rollback POLICY MODEL, not a verifier or disk updater.
//! Evidence and health values model external checks; they are not credentials.

pub const MAX_TRIAL_BOOTS: u8 = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    InvalidRelease,
    SecurityFloor,
    NotNewer,
    Prerequisite,
    PendingTrial,
    SequenceExhausted,
    StaleTicket,
    Unhealthy,
    Invariant,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Release {
    generation: u64,
    security_epoch: u64,
}

impl Release {
    pub fn new(generation: u64, security_epoch: u64) -> Result<Self, Error> {
        if generation == 0 {
            return Err(Error::InvalidRelease);
        }
        Ok(Self {
            generation,
            security_epoch,
        })
    }

    pub const fn generation(self) -> u64 {
        self.generation
    }

    pub const fn security_epoch(self) -> u64 {
        self.security_epoch
    }
}

/// Assertions supplied to the model, NOT proof of signature validation or I/O.
/// Real integration must obtain these from independent verified subsystems.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Evidence {
    pub signature_verified: bool,
    pub payload_hashes_verified: bool,
    pub usb_identity_matched: bool,
    pub durable_stage_complete: bool,
    pub schemas_compatible: bool,
}

impl Evidence {
    fn complete(self) -> bool {
        self.signature_verified
            && self.payload_hashes_verified
            && self.usb_identity_matched
            && self.durable_stage_complete
            && self.schemas_compatible
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Health {
    pub root_mounted: bool,
    pub userspace_ready: bool,
    pub storage_flush_ok: bool,
}

impl Health {
    fn complete(self) -> bool {
        self.root_mounted && self.userspace_ready && self.storage_flush_ok
    }
}

/// Binds confirmation to a unique staging trial and its latest boot attempt.
/// This sequence guard is not a cryptographic authorization token.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BootTicket {
    generation: u64,
    trial_id: u64,
    attempt: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BootChoice {
    KnownGood(Release),
    Trial(Release, BootTicket),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Trial {
    release: Release,
    id: u64,
    remaining: u8,
    last_ticket: Option<BootTicket>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UpdatePolicy {
    known_good: Release,
    security_floor: u64,
    last_trial_id: u64,
    trial: Option<Trial>,
}

impl UpdatePolicy {
    pub fn new(known_good: Release, security_floor: u64) -> Result<Self, Error> {
        if known_good.security_epoch < security_floor {
            return Err(Error::SecurityFloor);
        }
        Ok(Self {
            known_good,
            security_floor,
            last_trial_id: 0,
            trial: None,
        })
    }

    pub const fn known_good(&self) -> Release {
        self.known_good
    }

    pub fn stage(&mut self, candidate: Release, evidence: Evidence) -> Result<(), Error> {
        if self.trial.is_some() {
            return Err(Error::PendingTrial);
        }
        if candidate.security_epoch < self.security_floor {
            return Err(Error::SecurityFloor);
        }
        if candidate.generation <= self.known_good.generation {
            return Err(Error::NotNewer);
        }
        if !evidence.complete() {
            return Err(Error::Prerequisite);
        }
        let id = self
            .last_trial_id
            .checked_add(1)
            .ok_or(Error::SequenceExhausted)?;
        self.trial = Some(Trial {
            release: candidate,
            id,
            remaining: MAX_TRIAL_BOOTS,
            last_ticket: None,
        });
        self.last_trial_id = id;
        Ok(())
    }

    /// Models one boot boundary. Integration MUST durably commit the resulting
    /// decremented state before executing the selected generation. A reset
    /// without confirmation consumes an attempt; no counter wraps or resets.
    pub fn begin_boot(&mut self) -> BootChoice {
        if let Some(trial) = self.trial.as_mut() {
            if trial.remaining == 0 {
                self.trial = None;
            } else {
                trial.remaining -= 1;
                let ticket = BootTicket {
                    generation: trial.release.generation,
                    trial_id: trial.id,
                    attempt: MAX_TRIAL_BOOTS - trial.remaining,
                };
                trial.last_ticket = Some(ticket);
                return BootChoice::Trial(trial.release, ticket);
            }
        }
        BootChoice::KnownGood(self.known_good)
    }

    /// A real health acknowledgement must be authenticated and persisted by
    /// the trusted boot manager. This model performs neither operation.
    pub fn confirm(&mut self, ticket: BootTicket, health: Health) -> Result<(), Error> {
        let trial = self.trial.ok_or(Error::StaleTicket)?;
        if trial.last_ticket != Some(ticket) {
            return Err(Error::StaleTicket);
        }
        if !health.complete() {
            return Err(Error::Unhealthy);
        }
        self.known_good = trial.release;
        self.trial = None;
        Ok(())
    }
}

fn complete_evidence() -> Evidence {
    Evidence {
        signature_verified: true,
        payload_hashes_verified: true,
        usb_identity_matched: true,
        durable_stage_complete: true,
        schemas_compatible: true,
    }
}

fn complete_health() -> Health {
    Health {
        root_mounted: true,
        userspace_ready: true,
        storage_flush_ok: true,
    }
}

/// Simulated facts only. Executes transitions, not installation or reboot.
pub fn self_test() -> Result<(), Error> {
    let old = Release::new(1, 1)?;
    let new = Release::new(2, 1)?;
    let mut policy = UpdatePolicy::new(old, 1)?;
    policy.stage(new, complete_evidence())?;
    for _ in 0..MAX_TRIAL_BOOTS {
        if !matches!(policy.begin_boot(), BootChoice::Trial(release, _) if release == new) {
            return Err(Error::Invariant);
        }
    }
    if policy.begin_boot() != BootChoice::KnownGood(old) {
        return Err(Error::Invariant);
    }
    policy.stage(new, complete_evidence())?;
    let BootChoice::Trial(_, ticket) = policy.begin_boot() else {
        return Err(Error::Invariant);
    };
    policy.confirm(ticket, complete_health())?;
    if policy.begin_boot() != BootChoice::KnownGood(new) {
        return Err(Error::Invariant);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> UpdatePolicy {
        UpdatePolicy::new(Release::new(10, 2).unwrap(), 2).unwrap()
    }

    fn ticket(policy: &mut UpdatePolicy) -> BootTicket {
        let BootChoice::Trial(_, ticket) = policy.begin_boot() else {
            panic!("expected trial boot")
        };
        ticket
    }

    #[test]
    fn rollback_and_healthy_confirmation() {
        assert_eq!(self_test(), Ok(()));
    }

    #[test]
    fn all_prerequisites_required_without_partial_mutation() {
        for bits in 0..32u8 {
            let mut policy = policy();
            let before = policy;
            let evidence = Evidence {
                signature_verified: bits & 1 != 0,
                payload_hashes_verified: bits & 2 != 0,
                usb_identity_matched: bits & 4 != 0,
                durable_stage_complete: bits & 8 != 0,
                schemas_compatible: bits & 16 != 0,
            };
            let result = policy.stage(Release::new(11, 2).unwrap(), evidence);
            if bits == 31 {
                assert_eq!(result, Ok(()));
            } else {
                assert_eq!(result, Err(Error::Prerequisite));
                assert_eq!(policy, before);
            }
        }
    }

    #[test]
    fn generations_and_security_floor_are_checked() {
        assert_eq!(Release::new(0, 1), Err(Error::InvalidRelease));
        let mut policy = policy();
        let evidence = complete_evidence();
        for generation in [9, 10] {
            let candidate = Release::new(generation, 2).unwrap();
            assert_eq!(policy.stage(candidate, evidence), Err(Error::NotNewer));
        }
        let old_epoch = Release::new(11, 1).unwrap();
        assert_eq!(policy.stage(old_epoch, evidence), Err(Error::SecurityFloor));
        assert_eq!(UpdatePolicy::new(old_epoch, 2), Err(Error::SecurityFloor));
    }

    #[test]
    fn stale_and_unhealthy_confirmations_preserve_known_good() {
        let mut policy = policy();
        let old = policy.known_good();
        let candidate = Release::new(11, 2).unwrap();
        policy.stage(candidate, complete_evidence()).unwrap();
        let first = ticket(&mut policy);
        let second = ticket(&mut policy);
        assert_eq!(
            policy.confirm(first, complete_health()),
            Err(Error::StaleTicket)
        );
        assert_eq!(
            policy.confirm(second, Health::default()),
            Err(Error::Unhealthy)
        );
        assert_eq!(policy.known_good(), old);
        policy.confirm(second, complete_health()).unwrap();
        assert_eq!(policy.known_good().generation(), 11);
        assert_eq!(
            policy.confirm(second, complete_health()),
            Err(Error::StaleTicket)
        );
    }

    #[test]
    fn pending_trial_cannot_be_replaced_or_extended() {
        let mut policy = policy();
        let old = policy.known_good();
        let candidate = Release::new(11, 2).unwrap();
        policy.stage(candidate, complete_evidence()).unwrap();
        for _ in 0..MAX_TRIAL_BOOTS {
            assert_eq!(
                policy.stage(candidate, complete_evidence()),
                Err(Error::PendingTrial)
            );
            assert!(matches!(policy.begin_boot(), BootChoice::Trial(_, _)));
        }
        for _ in 0..10 {
            assert_eq!(policy.begin_boot(), BootChoice::KnownGood(old));
        }
    }

    #[test]
    fn restaging_same_release_invalidates_previous_trial_tickets() {
        let mut policy = policy();
        let candidate = Release::new(11, 2).unwrap();
        policy.stage(candidate, complete_evidence()).unwrap();
        let stale = ticket(&mut policy);
        for _ in 0..MAX_TRIAL_BOOTS {
            policy.begin_boot();
        }
        policy.stage(candidate, complete_evidence()).unwrap();
        let current = ticket(&mut policy);
        assert_ne!(stale, current);
        assert_eq!(
            policy.confirm(stale, complete_health()),
            Err(Error::StaleTicket)
        );
        policy.confirm(current, complete_health()).unwrap();
    }

    #[test]
    fn trial_sequence_never_wraps() {
        let mut policy = policy();
        policy.last_trial_id = u64::MAX;
        let before = policy;
        let candidate = Release::new(11, 2).unwrap();
        assert_eq!(
            policy.stage(candidate, complete_evidence()),
            Err(Error::SequenceExhausted)
        );
        assert_eq!(policy, before);
    }
}
