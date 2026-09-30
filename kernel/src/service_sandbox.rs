//! Bounded privilege-separated service sandbox profiles.
//!
//! Trusted service launch code applies one profile before entering daemon code.
//! The operation is fail-closed: every change either preserves authority or
//! removes it. No-new-privileges is enabled before the final credential drop.

use crate::{
    credentials::{Credentials, Gid, Uid},
    path_policy::{ACCESS_READ, ACCESS_WRITE, Rule},
    process::{Error as ProcessError, Pid, Table},
    syscall_abi as abi,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Service {
    Resolver,
    Ntp,
    Ssh,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkScope {
    LoopbackOnly,
    ClientOnly,
    Listener,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Applied {
    pub uid: Uid,
    pub gid: Gid,
    pub promises: u64,
    pub network: NetworkScope,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Process(ProcessError),
    Path,
    Invariant,
}

impl From<ProcessError> for Error {
    fn from(value: ProcessError) -> Self {
        Self::Process(value)
    }
}

fn rules(service: Service) -> Result<([Rule; 3], usize), Error> {
    let empty = Rule::EMPTY;
    match service {
        Service::Resolver => Ok((
            [
                Rule::new(b"/etc", ACCESS_READ).map_err(|_| Error::Path)?,
                Rule::new(b"/tmp", ACCESS_READ | ACCESS_WRITE).map_err(|_| Error::Path)?,
                empty,
            ],
            2,
        )),
        Service::Ntp => Ok((
            [
                Rule::new(b"/etc", ACCESS_READ).map_err(|_| Error::Path)?,
                empty,
                empty,
            ],
            1,
        )),
        Service::Ssh => Ok((
            [
                Rule::new(b"/etc", ACCESS_READ).map_err(|_| Error::Path)?,
                Rule::new(b"/home", ACCESS_READ | ACCESS_WRITE).map_err(|_| Error::Path)?,
                Rule::new(b"/tmp", ACCESS_READ | ACCESS_WRITE).map_err(|_| Error::Path)?,
            ],
            3,
        )),
    }
}

fn identity(service: Service) -> (Uid, Gid, u64, NetworkScope) {
    match service {
        Service::Resolver => (
            Uid::from_raw(53),
            Gid::from_raw(53),
            abi::PROMISE_IO | abi::PROMISE_FILESYSTEM,
            NetworkScope::LoopbackOnly,
        ),
        Service::Ntp => (
            Uid::from_raw(123),
            Gid::from_raw(123),
            abi::PROMISE_IO | abi::PROMISE_FILESYSTEM,
            NetworkScope::ClientOnly,
        ),
        Service::Ssh => (
            Uid::from_raw(74),
            Gid::from_raw(74),
            abi::PROMISE_IO | abi::PROMISE_FILESYSTEM | abi::PROMISE_PROCESS,
            NetworkScope::Listener,
        ),
    }
}

/// Apply the monotonic service sandbox to an already-created process.
///
/// Trusted launch code is responsible for creating the process. This function
/// then narrows syscall classes and paths, sets no-new-privileges, and finally
/// drops to the service UID/GID. If an intermediate restriction fails, the
/// process is left no more privileged than it was before the call.
pub fn apply<const N: usize>(
    table: &mut Table<N>,
    pid: Pid,
    service: Service,
) -> Result<Applied, Error> {
    let (uid, gid, promises, network) = identity(service);
    let (rules, count) = rules(service)?;

    table.restrict_promises(pid, promises)?;
    table.restrict_paths(pid, &rules[..count])?;
    table.set_no_new_privileges(pid)?;
    table.replace_credentials(pid, Credentials::user(uid, gid))?;

    Ok(Applied {
        uid,
        gid,
        promises,
        network,
    })
}

pub fn self_test() -> Result<(), Error> {
    let mut table = Table::<4>::new();
    let init = table.spawn_init()?;
    let resolver = table.spawn_child(init)?;
    let applied = apply(&mut table, resolver, Service::Resolver)?;
    let process = table.get(resolver).ok_or(Error::Invariant)?;

    if applied.uid.get() != 53
        || process.credentials.effective_uid != applied.uid
        || process.credentials.effective_gid != applied.gid
        || !process.no_new_privileges
        || process.promises != applied.promises
        || !process.path_policy.permits(b"/etc/resolv.conf", ACCESS_READ)
        || process.path_policy.permits(b"/etc/resolv.conf", ACCESS_WRITE)
        || !process.path_policy.permits(b"/tmp/resolver.cache", ACCESS_WRITE)
        || process.path_policy.permits(b"/home/user/.ssh", ACCESS_READ)
    {
        return Err(Error::Invariant);
    }

    if table.restrict_promises(resolver, abi::PROMISE_ALL).is_ok() {
        return Err(Error::Invariant);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_service_drops_root_and_enables_no_new_privileges() {
        for service in [Service::Resolver, Service::Ntp, Service::Ssh] {
            let mut table = Table::<3>::new();
            let init = table.spawn_init().unwrap();
            let child = table.spawn_child(init).unwrap();
            let applied = apply(&mut table, child, service).unwrap();
            let process = table.get(child).unwrap();
            assert_ne!(process.credentials.effective_uid, Uid::ROOT);
            assert_eq!(process.credentials.effective_uid, applied.uid);
            assert!(process.no_new_privileges);
            assert_eq!(process.promises, applied.promises);
        }
    }

    #[test]
    fn resolver_paths_are_component_aware_and_fail_closed() {
        let mut table = Table::<3>::new();
        let init = table.spawn_init().unwrap();
        let child = table.spawn_child(init).unwrap();
        apply(&mut table, child, Service::Resolver).unwrap();
        let policy = table.get(child).unwrap().path_policy;
        assert!(policy.permits(b"/etc/resolv.conf", ACCESS_READ));
        assert!(!policy.permits(b"/etc2/secret", ACCESS_READ));
        assert!(!policy.permits(b"/home/user", ACCESS_READ));
    }

    #[test]
    fn child_inherits_the_already_reduced_service_authority() {
        let mut table = Table::<4>::new();
        let init = table.spawn_init().unwrap();
        let daemon = table.spawn_child(init).unwrap();
        apply(&mut table, daemon, Service::Ssh).unwrap();
        let worker = table.spawn_child(daemon).unwrap();
        let parent = table.get(daemon).unwrap();
        let child = table.get(worker).unwrap();
        assert_eq!(child.credentials, parent.credentials);
        assert_eq!(child.promises, parent.promises);
        assert_eq!(child.path_policy, parent.path_policy);
        assert!(child.no_new_privileges);
    }

    #[test]
    fn production_self_test_passes() {
        self_test().unwrap();
    }
}
