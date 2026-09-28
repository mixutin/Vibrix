//! Fixed-capacity process identity and lifecycle model for early M5.
//!
//! This module owns PIDs, parent/child relationships and the Running -> Zombie
//! -> reaped transition. It is deliberately independent of the scheduler,
//! address-space owner and syscall dispatcher so those can be connected without
//! weakening lifetime rules. No heap allocation or unsafe Rust is used.

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Pid(u32);

impl Pid {
    pub const INIT: Self = Self(1);

    pub const fn get(self) -> u32 {
        self.0
    }

    pub const fn from_raw(raw: u64) -> Option<Self> {
        if raw == 0 || raw > u32::MAX as u64 {
            None
        } else {
            Some(Self(raw as u32))
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum State {
    Running,
    Zombie(i32),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Process {
    pub pid: Pid,
    pub parent: Option<Pid>,
    pub state: State,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaitTarget {
    Any,
    Pid(Pid),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaitObservation {
    Pending,
    Ready { pid: Pid, status: i32 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaitResult {
    Pending,
    Reaped { pid: Pid, status: i32 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Capacity,
    PidExhausted,
    NotFound,
    ParentNotRunning,
    AlreadyExited,
    NoChild,
}

pub struct Table<const N: usize> {
    slots: [Option<Process>; N],
    next_pid: u32,
}

impl<const N: usize> Default for Table<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> Table<N> {
    pub const fn new() -> Self {
        Self {
            slots: [None; N],
            next_pid: Pid::INIT.get(),
        }
    }

    pub fn len(&self) -> usize {
        self.slots.iter().filter(|slot| slot.is_some()).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get(&self, pid: Pid) -> Option<Process> {
        self.slots
            .iter()
            .flatten()
            .copied()
            .find(|process| process.pid == pid)
    }

    fn index_of(&self, pid: Pid) -> Option<usize> {
        self.slots
            .iter()
            .position(|slot| slot.is_some_and(|process| process.pid == pid))
    }

    fn allocate_pid(&mut self) -> Result<Pid, Error> {
        if self.next_pid == 0 {
            return Err(Error::PidExhausted);
        }
        let pid = Pid(self.next_pid);
        self.next_pid = self.next_pid.checked_add(1).unwrap_or(0);
        Ok(pid)
    }

    pub fn spawn_init(&mut self) -> Result<Pid, Error> {
        if self.get(Pid::INIT).is_some() || self.next_pid != Pid::INIT.get() {
            return Err(Error::ParentNotRunning);
        }
        self.spawn(None)
    }

    pub fn spawn_child(&mut self, parent: Pid) -> Result<Pid, Error> {
        match self.get(parent) {
            Some(Process {
                state: State::Running,
                ..
            }) => self.spawn(Some(parent)),
            Some(_) => Err(Error::ParentNotRunning),
            None => Err(Error::NotFound),
        }
    }

    fn spawn(&mut self, parent: Option<Pid>) -> Result<Pid, Error> {
        let slot = self
            .slots
            .iter()
            .position(Option::is_none)
            .ok_or(Error::Capacity)?;
        let pid = self.allocate_pid()?;
        self.slots[slot] = Some(Process {
            pid,
            parent,
            state: State::Running,
        });
        Ok(pid)
    }

    pub fn exit(&mut self, pid: Pid, status: i32) -> Result<(), Error> {
        let index = self.index_of(pid).ok_or(Error::NotFound)?;
        let process = self.slots[index].expect("located process slot");
        if matches!(process.state, State::Zombie(_)) {
            return Err(Error::AlreadyExited);
        }

        let init_running = pid != Pid::INIT
            && self
                .get(Pid::INIT)
                .is_some_and(|init| init.state == State::Running);
        let adopt = init_running.then_some(Pid::INIT);
        for child in self.slots.iter_mut().flatten() {
            if child.parent == Some(pid) {
                child.parent = adopt;
            }
        }

        self.slots[index]
            .as_mut()
            .expect("located process slot")
            .state = State::Zombie(status);
        Ok(())
    }

    /// Observe wait readiness without consuming a zombie.
    ///
    /// Syscall code can validate/copy the status value to userspace first and
    /// only then call reap(). This prevents a bad userspace pointer from
    /// irreversibly losing a child exit status.
    pub fn observe_wait(
        &self,
        parent: Pid,
        target: WaitTarget,
    ) -> Result<WaitObservation, Error> {
        match self.get(parent) {
            Some(Process {
                state: State::Running,
                ..
            }) => {}
            Some(_) => return Err(Error::ParentNotRunning),
            None => return Err(Error::NotFound),
        }

        let mut matching_child = false;
        for process in self.slots.iter().flatten() {
            if process.parent != Some(parent) {
                continue;
            }
            let selected = match target {
                WaitTarget::Any => true,
                WaitTarget::Pid(pid) => process.pid == pid,
            };
            if !selected {
                continue;
            }
            matching_child = true;
            if let State::Zombie(status) = process.state {
                return Ok(WaitObservation::Ready {
                    pid: process.pid,
                    status,
                });
            }
        }

        if matching_child {
            Ok(WaitObservation::Pending)
        } else {
            Err(Error::NoChild)
        }
    }

    /// Consume one exact zombie child after any fallible status copy-out has
    /// already succeeded.
    pub fn reap(&mut self, parent: Pid, pid: Pid) -> Result<i32, Error> {
        let index = self.index_of(pid).ok_or(Error::NoChild)?;
        let process = self.slots[index].ok_or(Error::NoChild)?;
        if process.parent != Some(parent) {
            return Err(Error::NoChild);
        }
        let State::Zombie(status) = process.state else {
            return Err(Error::AlreadyExited);
        };
        self.slots[index] = None;
        Ok(status)
    }

    pub fn wait(&mut self, parent: Pid, target: WaitTarget) -> Result<WaitResult, Error> {
        match self.observe_wait(parent, target)? {
            WaitObservation::Pending => Ok(WaitResult::Pending),
            WaitObservation::Ready { pid, status } => {
                let reaped = self.reap(parent, pid)?;
                debug_assert_eq!(reaped, status);
                Ok(WaitResult::Reaped { pid, status })
            }
        }
    }
}

/// Production-linked behavior proof used by the normal post-firmware kernel
/// self-test. It exercises identity allocation, zombie retention, reaping,
/// parent validation and orphan adoption without touching scheduler state.
pub fn self_test() -> Result<(), Error> {
    let mut table = Table::<4>::new();
    let init = table.spawn_init()?;
    if init != Pid::INIT {
        return Err(Error::NotFound);
    }
    let first = table.spawn_child(init)?;
    let second = table.spawn_child(init)?;
    table.exit(first, 7)?;
    if table.wait(init, WaitTarget::Any)?
        != (WaitResult::Reaped {
            pid: first,
            status: 7,
        })
    {
        return Err(Error::NotFound);
    }

    let replacement = table.spawn_child(init)?;
    if replacement == first {
        return Err(Error::PidExhausted);
    }

    let grandchild = table.spawn_child(second)?;
    table.exit(second, 9)?;
    if table.get(grandchild).and_then(|p| p.parent) != Some(init) {
        return Err(Error::ParentNotRunning);
    }
    if table.wait(init, WaitTarget::Pid(second))?
        != (WaitResult::Reaped {
            pid: second,
            status: 9,
        })
    {
        return Err(Error::NotFound);
    }
    if table.wait(init, WaitTarget::Pid(grandchild))? != WaitResult::Pending {
        return Err(Error::NoChild);
    }
    if table.len() != 3 {
        return Err(Error::Capacity);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_and_children_receive_monotonic_pids() {
        let mut table = Table::<4>::new();
        let init = table.spawn_init().unwrap();
        let a = table.spawn_child(init).unwrap();
        let b = table.spawn_child(init).unwrap();
        assert_eq!(init.get(), 1);
        assert_eq!(a.get(), 2);
        assert_eq!(b.get(), 3);
        assert_eq!(table.len(), 3);
    }

    #[test]
    fn exited_process_stays_zombie_until_parent_reaps_it() {
        let mut table = Table::<3>::new();
        let init = table.spawn_init().unwrap();
        let child = table.spawn_child(init).unwrap();
        table.exit(child, -12).unwrap();
        assert_eq!(table.get(child).unwrap().state, State::Zombie(-12));
        assert_eq!(
            table.wait(init, WaitTarget::Pid(child)).unwrap(),
            WaitResult::Reaped {
                pid: child,
                status: -12
            }
        );
        assert_eq!(table.get(child), None);
        assert_eq!(
            table.wait(init, WaitTarget::Pid(child)),
            Err(Error::NoChild)
        );
    }

    #[test]
    fn running_children_make_wait_pending_without_mutation() {
        let mut table = Table::<3>::new();
        let init = table.spawn_init().unwrap();
        let child = table.spawn_child(init).unwrap();
        assert_eq!(
            table.wait(init, WaitTarget::Pid(child)).unwrap(),
            WaitResult::Pending
        );
        assert_eq!(table.get(child).unwrap().state, State::Running);
    }

    #[test]
    fn reaped_slot_reuses_storage_but_never_pid() {
        let mut table = Table::<2>::new();
        let init = table.spawn_init().unwrap();
        let first = table.spawn_child(init).unwrap();
        table.exit(first, 0).unwrap();
        table.wait(init, WaitTarget::Any).unwrap();
        let second = table.spawn_child(init).unwrap();
        assert_ne!(first, second);
        assert_eq!(second.get(), first.get() + 1);
        assert_eq!(table.len(), 2);
    }

    #[test]
    fn children_of_exiting_process_are_adopted_by_live_init() {
        let mut table = Table::<4>::new();
        let init = table.spawn_init().unwrap();
        let parent = table.spawn_child(init).unwrap();
        let child = table.spawn_child(parent).unwrap();
        table.exit(parent, 3).unwrap();
        assert_eq!(table.get(child).unwrap().parent, Some(init));
        assert_eq!(
            table.wait(init, WaitTarget::Pid(parent)).unwrap(),
            WaitResult::Reaped {
                pid: parent,
                status: 3
            }
        );
    }

    #[test]
    fn capacity_and_parent_state_fail_before_allocation() {
        let mut table = Table::<2>::new();
        let init = table.spawn_init().unwrap();
        let child = table.spawn_child(init).unwrap();
        assert_eq!(table.spawn_child(init), Err(Error::Capacity));
        table.exit(child, 0).unwrap();
        assert_eq!(table.spawn_child(child), Err(Error::ParentNotRunning));
        table.wait(init, WaitTarget::Any).unwrap();
        let next = table.spawn_child(init).unwrap();
        assert_eq!(next.get(), 3);
    }

    #[test]
    fn wait_rejects_non_children_and_zombie_parent() {
        let mut table = Table::<4>::new();
        let init = table.spawn_init().unwrap();
        let a = table.spawn_child(init).unwrap();
        let b = table.spawn_child(init).unwrap();
        let grandchild = table.spawn_child(a).unwrap();
        assert_eq!(
            table.wait(b, WaitTarget::Pid(grandchild)),
            Err(Error::NoChild)
        );
        table.exit(a, 1).unwrap();
        assert_eq!(
            table.wait(a, WaitTarget::Pid(grandchild)),
            Err(Error::ParentNotRunning)
        );
    }

    #[test]
    fn wait_observation_does_not_consume_zombie_until_explicit_reap() {
        let mut table = Table::<3>::new();
        let init = table.spawn_init().unwrap();
        let child = table.spawn_child(init).unwrap();
        table.exit(child, 41).unwrap();

        assert_eq!(
            table.observe_wait(init, WaitTarget::Pid(child)).unwrap(),
            WaitObservation::Ready {
                pid: child,
                status: 41
            }
        );
        assert_eq!(table.get(child).unwrap().state, State::Zombie(41));
        assert_eq!(table.reap(init, child), Ok(41));
        assert_eq!(table.get(child), None);
    }

    #[test]
    fn failed_or_premature_reap_preserves_process_state() {
        let mut table = Table::<4>::new();
        let init = table.spawn_init().unwrap();
        let running = table.spawn_child(init).unwrap();
        let other = table.spawn_child(init).unwrap();

        assert_eq!(table.reap(init, running), Err(Error::AlreadyExited));
        assert_eq!(table.get(running).unwrap().state, State::Running);

        table.exit(running, 7).unwrap();
        assert_eq!(table.reap(other, running), Err(Error::NoChild));
        assert_eq!(table.get(running).unwrap().state, State::Zombie(7));
    }

    #[test]
    fn raw_pid_conversion_rejects_zero_and_wide_values() {
        assert_eq!(Pid::from_raw(0), None);
        assert_eq!(Pid::from_raw(1), Some(Pid::INIT));
        assert_eq!(Pid::from_raw(u32::MAX as u64).unwrap().get(), u32::MAX);
        assert_eq!(Pid::from_raw(u32::MAX as u64 + 1), None);
    }

    #[test]
    fn production_self_test_passes() {
        self_test().unwrap();
    }
}
