//! Fixed-capacity update history and explicit rollback selection policy.

pub const HISTORY_CAPACITY: usize = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Generation {
    pub id: u64,
    pub security_epoch: u64,
    pub healthy: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    InvalidGeneration,
    DuplicateGeneration,
    HistoryFull,
    NotFound,
    Unhealthy,
    SecurityFloor,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct History {
    entries: [Option<Generation>; HISTORY_CAPACITY],
    len: usize,
    selected: u64,
    security_floor: u64,
}

impl History {
    pub fn new(initial: Generation, security_floor: u64) -> Result<Self, Error> {
        if initial.id == 0 {
            return Err(Error::InvalidGeneration);
        }
        if initial.security_epoch < security_floor {
            return Err(Error::SecurityFloor);
        }
        let mut entries = [None; HISTORY_CAPACITY];
        entries[0] = Some(initial);
        Ok(Self {
            entries,
            len: 1,
            selected: initial.id,
            security_floor,
        })
    }

    pub const fn selected(&self) -> u64 {
        self.selected
    }

    pub fn entries(&self) -> impl Iterator<Item = Generation> + '_ {
        self.entries[..self.len].iter().flatten().copied()
    }

    pub fn record(&mut self, generation: Generation) -> Result<(), Error> {
        if generation.id == 0 {
            return Err(Error::InvalidGeneration);
        }
        if generation.security_epoch < self.security_floor {
            return Err(Error::SecurityFloor);
        }
        if self.entries().any(|entry| entry.id == generation.id) {
            return Err(Error::DuplicateGeneration);
        }
        if self.len == HISTORY_CAPACITY {
            return Err(Error::HistoryFull);
        }
        self.entries[self.len] = Some(generation);
        self.len += 1;
        Ok(())
    }

    /// Select a previously recorded healthy generation without mutating
    /// history. Failure leaves the current selection unchanged.
    pub fn select_rollback(&mut self, generation_id: u64) -> Result<(), Error> {
        let candidate = self
            .entries()
            .find(|entry| entry.id == generation_id)
            .ok_or(Error::NotFound)?;
        if !candidate.healthy {
            return Err(Error::Unhealthy);
        }
        if candidate.security_epoch < self.security_floor {
            return Err(Error::SecurityFloor);
        }
        self.selected = candidate.id;
        Ok(())
    }
}

pub fn self_test() -> Result<(), Error> {
    let initial = Generation {
        id: 10,
        security_epoch: 2,
        healthy: true,
    };
    let mut history = History::new(initial, 2)?;
    history.record(Generation {
        id: 11,
        security_epoch: 2,
        healthy: true,
    })?;
    history.record(Generation {
        id: 12,
        security_epoch: 3,
        healthy: false,
    })?;
    history.select_rollback(11)?;
    let before = history.selected();
    if history.select_rollback(12) != Err(Error::Unhealthy) || history.selected() != before {
        return Err(Error::Unhealthy);
    }
    if history.select_rollback(99) != Err(Error::NotFound) || history.selected() != before {
        return Err(Error::NotFound);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn initial() -> Generation {
        Generation {
            id: 1,
            security_epoch: 1,
            healthy: true,
        }
    }

    #[test]
    fn history_records_unique_generations_and_preserves_order() {
        let mut history = History::new(initial(), 1).unwrap();
        history
            .record(Generation {
                id: 2,
                security_epoch: 1,
                healthy: true,
            })
            .unwrap();
        history
            .record(Generation {
                id: 3,
                security_epoch: 2,
                healthy: false,
            })
            .unwrap();
        let ids: std::vec::Vec<_> = history.entries().map(|entry| entry.id).collect();
        assert_eq!(ids, [1, 2, 3]);
        assert_eq!(
            history.record(Generation {
                id: 2,
                security_epoch: 1,
                healthy: true,
            }),
            Err(Error::DuplicateGeneration)
        );
    }

    #[test]
    fn rollback_selection_is_fail_closed_and_atomic() {
        let mut history = History::new(initial(), 1).unwrap();
        history
            .record(Generation {
                id: 2,
                security_epoch: 1,
                healthy: true,
            })
            .unwrap();
        history
            .record(Generation {
                id: 3,
                security_epoch: 1,
                healthy: false,
            })
            .unwrap();

        history.select_rollback(2).unwrap();
        assert_eq!(history.selected(), 2);
        assert_eq!(history.select_rollback(3), Err(Error::Unhealthy));
        assert_eq!(history.selected(), 2);
        assert_eq!(history.select_rollback(99), Err(Error::NotFound));
        assert_eq!(history.selected(), 2);
    }

    #[test]
    fn security_floor_applies_to_recorded_and_selected_generations() {
        assert_eq!(
            History::new(
                Generation {
                    id: 1,
                    security_epoch: 1,
                    healthy: true,
                },
                2,
            ),
            Err(Error::SecurityFloor)
        );
        let mut history = History::new(
            Generation {
                id: 2,
                security_epoch: 2,
                healthy: true,
            },
            2,
        )
        .unwrap();
        assert_eq!(
            history.record(Generation {
                id: 3,
                security_epoch: 1,
                healthy: true,
            }),
            Err(Error::SecurityFloor)
        );
    }

    #[test]
    fn production_self_test_passes() {
        self_test().unwrap();
    }
}
