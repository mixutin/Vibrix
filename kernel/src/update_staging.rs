//! Fixed-capacity transactional update staging policy.
//!
//! This module models when a candidate generation becomes eligible for the
//! existing trial-boot policy. It does not write storage or verify signatures.
//! Integration must persist each returned record atomically on the boot USB.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Phase {
    Manifest,
    Payload,
    Verified,
    Durable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Record {
    pub transaction: u64,
    pub generation: u64,
    pub phase: Phase,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    InvalidGeneration,
    NotNewer,
    Pending,
    NoPending,
    WrongTransaction,
    WrongPhase,
    SequenceExhausted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Stager {
    known_good_generation: u64,
    next_transaction: u64,
    pending: Option<Record>,
}

impl Stager {
    pub fn new(known_good_generation: u64) -> Result<Self, Error> {
        if known_good_generation == 0 {
            return Err(Error::InvalidGeneration);
        }
        Ok(Self {
            known_good_generation,
            next_transaction: 1,
            pending: None,
        })
    }

    pub const fn pending(&self) -> Option<Record> {
        self.pending
    }

    pub fn begin(&mut self, generation: u64) -> Result<Record, Error> {
        if generation == 0 {
            return Err(Error::InvalidGeneration);
        }
        if generation <= self.known_good_generation {
            return Err(Error::NotNewer);
        }
        if self.pending.is_some() {
            return Err(Error::Pending);
        }
        let transaction = self.next_transaction;
        self.next_transaction = self
            .next_transaction
            .checked_add(1)
            .ok_or(Error::SequenceExhausted)?;
        let record = Record {
            transaction,
            generation,
            phase: Phase::Manifest,
        };
        self.pending = Some(record);
        Ok(record)
    }

    fn advance(&mut self, transaction: u64, expected: Phase, next: Phase) -> Result<Record, Error> {
        let mut record = self.pending.ok_or(Error::NoPending)?;
        if record.transaction != transaction {
            return Err(Error::WrongTransaction);
        }
        if record.phase != expected {
            return Err(Error::WrongPhase);
        }
        record.phase = next;
        self.pending = Some(record);
        Ok(record)
    }

    pub fn payload_written(&mut self, transaction: u64) -> Result<Record, Error> {
        self.advance(transaction, Phase::Manifest, Phase::Payload)
    }

    pub fn verified(&mut self, transaction: u64) -> Result<Record, Error> {
        self.advance(transaction, Phase::Payload, Phase::Verified)
    }

    pub fn durable(&mut self, transaction: u64) -> Result<Record, Error> {
        self.advance(transaction, Phase::Verified, Phase::Durable)
    }

    /// Hand a fully durable candidate to the separate trial-boot policy.
    ///
    /// This does not promote the generation to known-good.
    pub fn take_durable(&mut self, transaction: u64) -> Result<u64, Error> {
        let record = self.pending.ok_or(Error::NoPending)?;
        if record.transaction != transaction {
            return Err(Error::WrongTransaction);
        }
        if record.phase != Phase::Durable {
            return Err(Error::WrongPhase);
        }
        self.pending = None;
        Ok(record.generation)
    }

    /// Reconstruct staging state after an interruption.
    ///
    /// Incomplete records are deliberately discarded. Only a record whose
    /// payload was independently verified and durably flushed survives.
    pub fn recover(
        known_good_generation: u64,
        last_transaction: u64,
        persisted: Option<Record>,
    ) -> Result<Self, Error> {
        if known_good_generation == 0 {
            return Err(Error::InvalidGeneration);
        }
        let next_transaction = last_transaction
            .checked_add(1)
            .ok_or(Error::SequenceExhausted)?;
        let pending = match persisted {
            Some(record)
                if record.transaction == last_transaction
                    && record.generation > known_good_generation
                    && record.phase == Phase::Durable =>
            {
                Some(record)
            }
            _ => None,
        };
        Ok(Self {
            known_good_generation,
            next_transaction,
            pending,
        })
    }
}

pub fn self_test() -> Result<(), Error> {
    let mut stager = Stager::new(10)?;
    let manifest = stager.begin(11)?;
    let payload = stager.payload_written(manifest.transaction)?;
    let verified = stager.verified(payload.transaction)?;
    let durable = stager.durable(verified.transaction)?;

    for record in [manifest, payload, verified] {
        let recovered = Stager::recover(10, record.transaction, Some(record))?;
        if recovered.pending().is_some() {
            return Err(Error::WrongPhase);
        }
    }

    let mut recovered = Stager::recover(10, durable.transaction, Some(durable))?;
    if recovered.pending() != Some(durable)
        || recovered.take_durable(durable.transaction)? != durable.generation
        || recovered.pending().is_some()
    {
        return Err(Error::WrongPhase);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_only_advance_in_order_and_fail_atomically() {
        let mut stager = Stager::new(7).unwrap();
        assert_eq!(stager.begin(7), Err(Error::NotNewer));
        let manifest = stager.begin(8).unwrap();
        let before = stager;
        assert_eq!(
            stager.verified(manifest.transaction),
            Err(Error::WrongPhase)
        );
        assert_eq!(stager, before);
        assert_eq!(
            stager.payload_written(manifest.transaction + 1),
            Err(Error::WrongTransaction)
        );
        assert_eq!(stager, before);

        let payload = stager.payload_written(manifest.transaction).unwrap();
        let verified = stager.verified(payload.transaction).unwrap();
        let durable = stager.durable(verified.transaction).unwrap();
        assert_eq!(stager.take_durable(durable.transaction), Ok(8));
        assert_eq!(stager.pending(), None);
    }

    #[test]
    fn every_incomplete_interruption_recovers_to_known_good_only() {
        let mut stager = Stager::new(20).unwrap();
        let manifest = stager.begin(21).unwrap();
        let payload = stager.payload_written(manifest.transaction).unwrap();
        let verified = stager.verified(payload.transaction).unwrap();

        for record in [manifest, payload, verified] {
            let recovered = Stager::recover(20, record.transaction, Some(record)).unwrap();
            assert_eq!(recovered.pending(), None);
        }
    }

    #[test]
    fn crash_cut_matrix_never_exposes_an_incomplete_generation() {
        const KNOWN_GOOD: u64 = 40;
        const CANDIDATE: u64 = 41;

        let mut stager = Stager::new(KNOWN_GOOD).unwrap();
        let manifest = stager.begin(CANDIDATE).unwrap();
        let payload = stager.payload_written(manifest.transaction).unwrap();
        let verified = stager.verified(payload.transaction).unwrap();
        let durable = stager.durable(verified.transaction).unwrap();

        // Model a power cut after each persistence point, including before the
        // first staging record becomes durable. Every non-Durable phase must
        // disappear during recovery.
        for persisted in [None, Some(manifest), Some(payload), Some(verified)] {
            let recovered =
                Stager::recover(KNOWN_GOOD, manifest.transaction, persisted).unwrap();
            assert_eq!(recovered.pending(), None);
        }

        // A cut after the Durable record preserves exactly the candidate and
        // still requires the separate trial-boot policy to consume it.
        let mut recovered =
            Stager::recover(KNOWN_GOOD, durable.transaction, Some(durable)).unwrap();
        assert_eq!(recovered.pending(), Some(durable));
        assert_eq!(
            recovered.take_durable(durable.transaction),
            Ok(CANDIDATE)
        );
        assert_eq!(recovered.pending(), None);

        // Torn/stale metadata cannot accidentally resurrect an older or
        // mismatched transaction as a candidate.
        for record in [
            Record {
                transaction: durable.transaction.saturating_sub(1),
                ..durable
            },
            Record {
                generation: KNOWN_GOOD,
                ..durable
            },
            Record {
                generation: KNOWN_GOOD - 1,
                ..durable
            },
        ] {
            let recovered =
                Stager::recover(KNOWN_GOOD, durable.transaction, Some(record)).unwrap();
            assert_eq!(recovered.pending(), None);
        }
    }

    #[test]
    fn interruption_recovery_advances_transaction_ids_without_reuse() {
        let recovered = Stager::recover(10, 77, None).unwrap();
        let mut recovered = recovered;
        let next = recovered.begin(11).unwrap();
        assert_eq!(next.transaction, 78);

        assert_eq!(
            Stager::recover(10, u64::MAX, None),
            Err(Error::SequenceExhausted)
        );
    }

    #[test]
    fn only_durable_candidate_survives_interruption() {
        let mut stager = Stager::new(20).unwrap();
        let manifest = stager.begin(21).unwrap();
        let payload = stager.payload_written(manifest.transaction).unwrap();
        let verified = stager.verified(payload.transaction).unwrap();
        let durable = stager.durable(verified.transaction).unwrap();

        let recovered = Stager::recover(20, durable.transaction, Some(durable)).unwrap();
        assert_eq!(recovered.pending(), Some(durable));
    }

    #[test]
    fn corrupt_or_stale_recovery_records_fail_closed() {
        let stale = Record {
            transaction: 4,
            generation: 11,
            phase: Phase::Durable,
        };
        assert_eq!(Stager::recover(10, 5, Some(stale)).unwrap().pending(), None);

        let rollback = Record {
            transaction: 5,
            generation: 9,
            phase: Phase::Durable,
        };
        assert_eq!(
            Stager::recover(10, 5, Some(rollback)).unwrap().pending(),
            None
        );
    }

    #[test]
    fn production_self_test_passes() {
        self_test().unwrap();
    }
}
