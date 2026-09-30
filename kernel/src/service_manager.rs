//! Fixed-capacity service-manager control state.
//!
//! This module owns enable/disable/start/stop/reload/status transitions. It does
//! not spawn processes yet; process supervision and dependency ordering are
//! separate integration layers.

pub const MAX_SERVICES: usize = 16;
pub const SERVICE_NAME_BYTES: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    InvalidName,
    Capacity,
    Duplicate,
    NotFound,
    Disabled,
    AlreadyRunning,
    NotRunning,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServiceName {
    bytes: [u8; SERVICE_NAME_BYTES],
    len: u8,
}

impl ServiceName {
    pub const EMPTY: Self = Self {
        bytes: [0; SERVICE_NAME_BYTES],
        len: 0,
    };

    pub fn new(name: &[u8]) -> Result<Self, Error> {
        if name.is_empty()
            || name.len() > SERVICE_NAME_BYTES
            || !name.iter().copied().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'-' | b'_' | b'.')
            })
        {
            return Err(Error::InvalidName);
        }
        let mut value = Self::EMPTY;
        value.bytes[..name.len()].copy_from_slice(name);
        value.len = name.len() as u8;
        Ok(value)
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeState {
    Stopped,
    Running { generation: u64 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Status {
    pub enabled: bool,
    pub runtime: RuntimeState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Entry {
    name: ServiceName,
    enabled: bool,
    runtime: RuntimeState,
    next_generation: u64,
}

pub struct Manager {
    entries: [Option<Entry>; MAX_SERVICES],
}

impl Manager {
    pub const fn new() -> Self {
        Self {
            entries: [None; MAX_SERVICES],
        }
    }

    pub fn register(&mut self, name: ServiceName, enabled: bool) -> Result<(), Error> {
        if self.entries.iter().flatten().any(|entry| entry.name == name) {
            return Err(Error::Duplicate);
        }
        let slot = self
            .entries
            .iter()
            .position(Option::is_none)
            .ok_or(Error::Capacity)?;
        self.entries[slot] = Some(Entry {
            name,
            enabled,
            runtime: RuntimeState::Stopped,
            next_generation: 1,
        });
        Ok(())
    }

    fn entry_mut(&mut self, name: ServiceName) -> Result<&mut Entry, Error> {
        self.entries
            .iter_mut()
            .flatten()
            .find(|entry| entry.name == name)
            .ok_or(Error::NotFound)
    }

    fn entry(&self, name: ServiceName) -> Result<&Entry, Error> {
        self.entries
            .iter()
            .flatten()
            .find(|entry| entry.name == name)
            .ok_or(Error::NotFound)
    }

    pub fn status(&self, name: ServiceName) -> Result<Status, Error> {
        let entry = self.entry(name)?;
        Ok(Status {
            enabled: entry.enabled,
            runtime: entry.runtime,
        })
    }

    pub fn enable(&mut self, name: ServiceName) -> Result<(), Error> {
        self.entry_mut(name)?.enabled = true;
        Ok(())
    }

    pub fn disable(&mut self, name: ServiceName) -> Result<(), Error> {
        let entry = self.entry_mut(name)?;
        entry.enabled = false;
        Ok(())
    }

    pub fn start(&mut self, name: ServiceName) -> Result<u64, Error> {
        let entry = self.entry_mut(name)?;
        if !entry.enabled {
            return Err(Error::Disabled);
        }
        if matches!(entry.runtime, RuntimeState::Running { .. }) {
            return Err(Error::AlreadyRunning);
        }
        let generation = entry.next_generation;
        entry.next_generation = entry.next_generation.checked_add(1).ok_or(Error::Capacity)?;
        entry.runtime = RuntimeState::Running { generation };
        Ok(generation)
    }

    pub fn stop(&mut self, name: ServiceName) -> Result<(), Error> {
        let entry = self.entry_mut(name)?;
        if entry.runtime == RuntimeState::Stopped {
            return Err(Error::NotRunning);
        }
        entry.runtime = RuntimeState::Stopped;
        Ok(())
    }

    pub fn reload(&mut self, name: ServiceName) -> Result<u64, Error> {
        let entry = self.entry_mut(name)?;
        let RuntimeState::Running { .. } = entry.runtime else {
            return Err(Error::NotRunning);
        };
        let generation = entry.next_generation;
        entry.next_generation = entry.next_generation.checked_add(1).ok_or(Error::Capacity)?;
        entry.runtime = RuntimeState::Running { generation };
        Ok(generation)
    }
}

impl Default for Manager {
    fn default() -> Self {
        Self::new()
    }
}

pub fn self_test() -> Result<(), Error> {
    let ssh = ServiceName::new(b"ssh")?;
    let mut manager = Manager::new();
    manager.register(ssh, false)?;
    if manager.start(ssh) != Err(Error::Disabled) {
        return Err(Error::Disabled);
    }
    manager.enable(ssh)?;
    let first = manager.start(ssh)?;
    let second = manager.reload(ssh)?;
    if first == second || manager.status(ssh)?.runtime != RuntimeState::Running { generation: second } {
        return Err(Error::AlreadyRunning);
    }
    manager.stop(ssh)?;
    manager.disable(ssh)?;
    if manager.status(ssh)? != (Status { enabled: false, runtime: RuntimeState::Stopped }) {
        return Err(Error::NotRunning);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(value: &[u8]) -> ServiceName {
        ServiceName::new(value).unwrap()
    }

    #[test]
    fn lifecycle_requires_enabled_service_and_advances_generation() {
        let mut manager = Manager::new();
        let svc = n(b"example");
        manager.register(svc, false).unwrap();
        assert_eq!(manager.start(svc), Err(Error::Disabled));
        manager.enable(svc).unwrap();
        assert_eq!(manager.start(svc), Ok(1));
        assert_eq!(manager.start(svc), Err(Error::AlreadyRunning));
        assert_eq!(manager.reload(svc), Ok(2));
        assert_eq!(
            manager.status(svc).unwrap(),
            Status {
                enabled: true,
                runtime: RuntimeState::Running { generation: 2 }
            }
        );
        manager.stop(svc).unwrap();
        assert_eq!(manager.reload(svc), Err(Error::NotRunning));
    }

    #[test]
    fn disable_does_not_fake_stop_a_running_service() {
        let mut manager = Manager::new();
        let svc = n(b"daemon");
        manager.register(svc, true).unwrap();
        manager.start(svc).unwrap();
        manager.disable(svc).unwrap();
        assert_eq!(
            manager.status(svc).unwrap(),
            Status {
                enabled: false,
                runtime: RuntimeState::Running { generation: 1 }
            }
        );
        manager.stop(svc).unwrap();
        assert_eq!(manager.start(svc), Err(Error::Disabled));
    }

    #[test]
    fn registration_is_bounded_and_unique() {
        let mut manager = Manager::new();
        let first = n(b"same");
        manager.register(first, true).unwrap();
        assert_eq!(manager.register(first, false), Err(Error::Duplicate));
        assert_eq!(manager.status(n(b"missing")), Err(Error::NotFound));
    }

    #[test]
    fn production_self_test_passes() {
        self_test().unwrap();
    }
}
