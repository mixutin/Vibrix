//! Fixed-capacity process identity and lifecycle model for early M5.
//!
//! This module owns PIDs, parent/child relationships and the Running -> Zombie
//! -> reaped transition. It is deliberately independent of the scheduler,
//! address-space owner and syscall dispatcher so those can be connected without
//! weakening lifetime rules. No heap allocation or unsafe Rust is used.

use crate::{credentials::Credentials, path_policy::Policy as PathPolicy, syscall_abi as abi};

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
pub struct ResourceLimits {
    pub cpu_ticks: u64,
    pub memory_pages: u32,
    pub open_files: u16,
    pub sockets: u16,
    pub children: u16,
}

impl ResourceLimits {
    pub const UNLIMITED: Self = Self {
        cpu_ticks: u64::MAX,
        memory_pages: u32::MAX,
        open_files: u16::MAX,
        sockets: u16::MAX,
        children: u16::MAX,
    };

    pub const fn bounded(
        cpu_ticks: u64,
        memory_pages: u32,
        open_files: u16,
        sockets: u16,
        children: u16,
    ) -> Self {
        Self {
            cpu_ticks,
            memory_pages,
            open_files,
            sockets,
            children,
        }
    }

    const fn no_wider_than(self, other: Self) -> bool {
        self.cpu_ticks <= other.cpu_ticks
            && self.memory_pages <= other.memory_pages
            && self.open_files <= other.open_files
            && self.sockets <= other.sockets
            && self.children <= other.children
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ResourceUsage {
    pub cpu_ticks: u64,
    pub memory_pages: u32,
    pub open_files: u16,
    pub sockets: u16,
    pub children: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Process {
    pub pid: Pid,
    pub parent: Option<Pid>,
    pub state: State,
    pub credentials: Credentials,
    pub no_new_privileges: bool,
    pub promises: u64,
    pub path_policy: PathPolicy,
    pub resource_limits: ResourceLimits,
    pub resource_usage: ResourceUsage,
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
    InvalidPromises,
    PromiseExpansion,
    ResourceLimit,
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

    pub fn entry(&self, index: usize) -> Option<Process> {
        self.slots.iter().flatten().copied().nth(index)
    }

    pub fn replace_credentials(&mut self, pid: Pid, credentials: Credentials) -> Result<(), Error> {
        let index = self.index_of(pid).ok_or(Error::NotFound)?;
        let process = self.slots[index].as_mut().ok_or(Error::NotFound)?;
        if process.state != State::Running {
            return Err(Error::ParentNotRunning);
        }
        process.credentials = credentials;
        Ok(())
    }

    /// Permanently restrict the process operation classes available through
    /// the process-facing syscall dispatcher. A process may only remove bits.
    pub fn restrict_promises(&mut self, pid: Pid, promises: u64) -> Result<(), Error> {
        if promises & !abi::PROMISE_ALL != 0 {
            return Err(Error::InvalidPromises);
        }
        let index = self.index_of(pid).ok_or(Error::NotFound)?;
        let process = self.slots[index].as_mut().ok_or(Error::NotFound)?;
        if process.state != State::Running {
            return Err(Error::ParentNotRunning);
        }
        if promises & !process.promises != 0 {
            return Err(Error::PromiseExpansion);
        }
        process.promises = promises;
        Ok(())
    }

    /// Replace the process path policy with an equal-or-narrower allow-list.
    pub fn restrict_paths(
        &mut self,
        pid: Pid,
        rules: &[crate::path_policy::Rule],
    ) -> Result<(), Error> {
        let index = self.index_of(pid).ok_or(Error::NotFound)?;
        let process = self.slots[index].as_mut().ok_or(Error::NotFound)?;
        if process.state != State::Running {
            return Err(Error::ParentNotRunning);
        }
        process
            .path_policy
            .restrict(rules)
            .map_err(|_| Error::PromiseExpansion)
    }

    /// Replace the process resource budget with an equal-or-narrower one.
    /// Limits may not be lowered beneath already-accounted usage.
    pub fn restrict_resource_limits(
        &mut self,
        pid: Pid,
        limits: ResourceLimits,
    ) -> Result<(), Error> {
        let index = self.index_of(pid).ok_or(Error::NotFound)?;
        let process = self.slots[index].as_mut().ok_or(Error::NotFound)?;
        if process.state != State::Running {
            return Err(Error::ParentNotRunning);
        }
        if !limits.no_wider_than(process.resource_limits)
            || limits.cpu_ticks < process.resource_usage.cpu_ticks
            || limits.memory_pages < process.resource_usage.memory_pages
            || limits.open_files < process.resource_usage.open_files
            || limits.sockets < process.resource_usage.sockets
            || limits.children < process.resource_usage.children
        {
            return Err(Error::ResourceLimit);
        }
        process.resource_limits = limits;
        Ok(())
    }

    pub fn charge_cpu(&mut self, pid: Pid, ticks: u64) -> Result<(), Error> {
        let process = self.process_mut_running(pid)?;
        let next = process
            .resource_usage
            .cpu_ticks
            .checked_add(ticks)
            .ok_or(Error::ResourceLimit)?;
        if next > process.resource_limits.cpu_ticks {
            return Err(Error::ResourceLimit);
        }
        process.resource_usage.cpu_ticks = next;
        Ok(())
    }

    pub fn reserve_memory_pages(&mut self, pid: Pid, pages: u32) -> Result<(), Error> {
        let process = self.process_mut_running(pid)?;
        let next = process
            .resource_usage
            .memory_pages
            .checked_add(pages)
            .ok_or(Error::ResourceLimit)?;
        if next > process.resource_limits.memory_pages {
            return Err(Error::ResourceLimit);
        }
        process.resource_usage.memory_pages = next;
        Ok(())
    }

    pub fn release_memory_pages(&mut self, pid: Pid, pages: u32) -> Result<(), Error> {
        let process = self.process_mut_running(pid)?;
        process.resource_usage.memory_pages = process
            .resource_usage
            .memory_pages
            .checked_sub(pages)
            .ok_or(Error::ResourceLimit)?;
        Ok(())
    }

    pub fn reserve_file(&mut self, pid: Pid) -> Result<(), Error> {
        let process = self.process_mut_running(pid)?;
        let next = process
            .resource_usage
            .open_files
            .checked_add(1)
            .ok_or(Error::ResourceLimit)?;
        if next > process.resource_limits.open_files {
            return Err(Error::ResourceLimit);
        }
        process.resource_usage.open_files = next;
        Ok(())
    }

    pub fn release_file(&mut self, pid: Pid) -> Result<(), Error> {
        let process = self.process_mut_running(pid)?;
        process.resource_usage.open_files = process
            .resource_usage
            .open_files
            .checked_sub(1)
            .ok_or(Error::ResourceLimit)?;
        Ok(())
    }

    pub fn reserve_socket(&mut self, pid: Pid) -> Result<(), Error> {
        let process = self.process_mut_running(pid)?;
        let next = process
            .resource_usage
            .sockets
            .checked_add(1)
            .ok_or(Error::ResourceLimit)?;
        if next > process.resource_limits.sockets {
            return Err(Error::ResourceLimit);
        }
        process.resource_usage.sockets = next;
        Ok(())
    }

    pub fn release_socket(&mut self, pid: Pid) -> Result<(), Error> {
        let process = self.process_mut_running(pid)?;
        process.resource_usage.sockets = process
            .resource_usage
            .sockets
            .checked_sub(1)
            .ok_or(Error::ResourceLimit)?;
        Ok(())
    }

    fn process_mut_running(&mut self, pid: Pid) -> Result<&mut Process, Error> {
        let index = self.index_of(pid).ok_or(Error::NotFound)?;
        let process = self.slots[index].as_mut().ok_or(Error::NotFound)?;
        if process.state != State::Running {
            return Err(Error::ParentNotRunning);
        }
        Ok(process)
    }

    /// Permanently prevent this process and descendants from gaining privileges
    /// through future execution transitions.
    pub fn set_no_new_privileges(&mut self, pid: Pid) -> Result<(), Error> {
        let index = self.index_of(pid).ok_or(Error::NotFound)?;
        let process = self.slots[index].as_mut().ok_or(Error::NotFound)?;
        if process.state != State::Running {
            return Err(Error::ParentNotRunning);
        }
        process.no_new_privileges = true;
        Ok(())
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
        self.spawn(
            None,
            Credentials::root(),
            false,
            abi::PROMISE_ALL,
            PathPolicy::unrestricted(),
            ResourceLimits::UNLIMITED,
        )
    }

    pub fn spawn_child(&mut self, parent: Pid) -> Result<Pid, Error> {
        let process = match self.get(parent) {
            Some(
                process @ Process {
                    state: State::Running,
                    ..
                },
            ) => process,
            Some(_) => return Err(Error::ParentNotRunning),
            None => return Err(Error::NotFound),
        };
        let child_count = process
            .resource_usage
            .children
            .checked_add(1)
            .ok_or(Error::ResourceLimit)?;
        if child_count > process.resource_limits.children {
            return Err(Error::ResourceLimit);
        }
        let child = self.spawn(
            Some(parent),
            process.credentials,
            process.no_new_privileges,
            process.promises,
            process.path_policy,
            process.resource_limits,
        )?;
        self.process_mut_running(parent)?.resource_usage.children = child_count;
        Ok(child)
    }

    fn spawn(
        &mut self,
        parent: Option<Pid>,
        credentials: Credentials,
        no_new_privileges: bool,
        promises: u64,
        path_policy: PathPolicy,
        resource_limits: ResourceLimits,
    ) -> Result<Pid, Error> {
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
            credentials,
            no_new_privileges,
            promises,
            path_policy,
            resource_limits,
            resource_usage: ResourceUsage::default(),
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
    pub fn observe_wait(&self, parent: Pid, target: WaitTarget) -> Result<WaitObservation, Error> {
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
        if let Some(parent_index) = self.index_of(parent)
            && let Some(parent_process) = self.slots[parent_index].as_mut()
        {
            parent_process.resource_usage.children = parent_process
                .resource_usage
                .children
                .checked_sub(1)
                .ok_or(Error::ResourceLimit)?;
        }
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
    let mut table = Table::<6>::new();
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
    table.restrict_promises(init, abi::PROMISE_IO | abi::PROMISE_FILESYSTEM)?;
    let child = table.spawn_child(init)?;
    if table.get(child).map(|process| process.promises)
        != Some(abi::PROMISE_IO | abi::PROMISE_FILESYSTEM)
    {
        return Err(Error::InvalidPromises);
    }
    if table.restrict_promises(init, abi::PROMISE_ALL) != Err(Error::PromiseExpansion) {
        return Err(Error::PromiseExpansion);
    }
    let tmp = crate::path_policy::Rule::new(
        b"/tmp",
        crate::path_policy::ACCESS_READ | crate::path_policy::ACCESS_WRITE,
    )
    .map_err(|_| Error::PromiseExpansion)?;
    table.restrict_paths(init, &[tmp])?;
    let path_child = table.spawn_child(init)?;
    if !table.get(path_child).is_some_and(|process| {
        process
            .path_policy
            .permits(b"/tmp/file", crate::path_policy::ACCESS_READ)
    }) {
        return Err(Error::PromiseExpansion);
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
    fn init_is_root_and_children_inherit_credentials() {
        let mut table = Table::<3>::new();
        let init = table.spawn_init().unwrap();
        let child = table.spawn_child(init).unwrap();
        assert_eq!(table.get(init).unwrap().credentials, Credentials::root());
        assert_eq!(
            table.get(child).unwrap().credentials,
            table.get(init).unwrap().credentials
        );
    }

    #[test]
    fn process_promises_only_shrink_and_are_inherited() {
        let mut table = Table::<4>::new();
        let init = table.spawn_init().unwrap();
        assert_eq!(table.get(init).unwrap().promises, abi::PROMISE_ALL);

        let reduced = abi::PROMISE_IO | abi::PROMISE_FILESYSTEM;
        table.restrict_promises(init, reduced).unwrap();
        assert_eq!(table.get(init).unwrap().promises, reduced);

        assert_eq!(
            table.restrict_promises(init, abi::PROMISE_ALL),
            Err(Error::PromiseExpansion)
        );
        assert_eq!(
            table.restrict_promises(init, abi::PROMISE_ALL | (1 << 31)),
            Err(Error::InvalidPromises)
        );
        assert_eq!(table.get(init).unwrap().promises, reduced);

        let child = table.spawn_child(init).unwrap();
        assert_eq!(table.get(child).unwrap().promises, reduced);
        table.restrict_promises(child, abi::PROMISE_IO).unwrap();
        assert_eq!(table.get(child).unwrap().promises, abi::PROMISE_IO);
    }

    #[test]
    fn path_policy_is_monotonic_and_inherited() {
        let mut table = Table::<4>::new();
        let init = table.spawn_init().unwrap();
        let rw = crate::path_policy::Rule::new(
            b"/tmp",
            crate::path_policy::ACCESS_READ | crate::path_policy::ACCESS_WRITE,
        )
        .unwrap();
        table.restrict_paths(init, &[rw]).unwrap();
        assert!(
            table
                .get(init)
                .unwrap()
                .path_policy
                .permits(b"/tmp/file", crate::path_policy::ACCESS_WRITE)
        );

        let child = table.spawn_child(init).unwrap();
        assert_eq!(
            table.get(child).unwrap().path_policy,
            table.get(init).unwrap().path_policy
        );

        let read = crate::path_policy::Rule::new(b"/tmp", crate::path_policy::ACCESS_READ).unwrap();
        table.restrict_paths(child, &[read]).unwrap();
        assert!(
            !table
                .get(child)
                .unwrap()
                .path_policy
                .permits(b"/tmp/file", crate::path_policy::ACCESS_WRITE)
        );
        assert_eq!(
            table.restrict_paths(child, &[rw]),
            Err(Error::PromiseExpansion)
        );
    }

    #[test]
    fn no_new_privileges_is_monotonic_and_inherited() {
        let mut table = Table::<4>::new();
        let init = table.spawn_init().unwrap();
        assert!(!table.get(init).unwrap().no_new_privileges);
        table.set_no_new_privileges(init).unwrap();
        assert!(table.get(init).unwrap().no_new_privileges);

        let child = table.spawn_child(init).unwrap();
        assert!(table.get(child).unwrap().no_new_privileges);
        table.set_no_new_privileges(child).unwrap();
        assert!(table.get(child).unwrap().no_new_privileges);
    }

    #[test]
    fn resource_limits_only_shrink_and_failed_reservations_do_not_mutate_usage() {
        let mut table = Table::<4>::new();
        let init = table.spawn_init().unwrap();
        let limits = ResourceLimits::bounded(10, 4, 2, 1, 1);
        table.restrict_resource_limits(init, limits).unwrap();

        table.charge_cpu(init, 6).unwrap();
        assert_eq!(table.charge_cpu(init, 5), Err(Error::ResourceLimit));
        assert_eq!(table.get(init).unwrap().resource_usage.cpu_ticks, 6);

        table.reserve_memory_pages(init, 4).unwrap();
        assert_eq!(
            table.reserve_memory_pages(init, 1),
            Err(Error::ResourceLimit)
        );
        assert_eq!(table.get(init).unwrap().resource_usage.memory_pages, 4);
        table.release_memory_pages(init, 2).unwrap();

        table.reserve_file(init).unwrap();
        table.reserve_file(init).unwrap();
        assert_eq!(table.reserve_file(init), Err(Error::ResourceLimit));
        table.release_file(init).unwrap();

        table.reserve_socket(init).unwrap();
        assert_eq!(table.reserve_socket(init), Err(Error::ResourceLimit));
        table.release_socket(init).unwrap();

        assert_eq!(
            table.restrict_resource_limits(init, ResourceLimits::UNLIMITED),
            Err(Error::ResourceLimit)
        );
        assert_eq!(
            table.restrict_resource_limits(
                init,
                ResourceLimits::bounded(5, 2, 1, 1, 1)
            ),
            Err(Error::ResourceLimit)
        );
    }

    #[test]
    fn child_limit_is_inherited_and_released_only_on_reap() {
        let mut table = Table::<4>::new();
        let init = table.spawn_init().unwrap();
        let limits = ResourceLimits::bounded(100, 10, 4, 2, 1);
        table.restrict_resource_limits(init, limits).unwrap();

        let child = table.spawn_child(init).unwrap();
        assert_eq!(table.get(child).unwrap().resource_limits, limits);
        assert_eq!(table.get(init).unwrap().resource_usage.children, 1);
        assert_eq!(table.spawn_child(init), Err(Error::ResourceLimit));

        table.exit(child, 0).unwrap();
        assert_eq!(table.get(init).unwrap().resource_usage.children, 1);
        assert_eq!(table.spawn_child(init), Err(Error::ResourceLimit));

        assert_eq!(table.reap(init, child), Ok(0));
        assert_eq!(table.get(init).unwrap().resource_usage.children, 0);
        table.spawn_child(init).unwrap();
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
    fn entries_enumerate_live_processes_without_exposing_slots() {
        let mut table = Table::<4>::new();
        let init = table.spawn_init().unwrap();
        let child = table.spawn_child(init).unwrap();
        assert_eq!(table.entry(0).unwrap().pid, init);
        assert_eq!(table.entry(1).unwrap().pid, child);
        assert_eq!(table.entry(2), None);
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
