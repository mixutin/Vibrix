//! Process-facing Vibrix syscall ABI v1 policy.
//!
//! This layer translates ABI register values into the bounded process table
//! without dereferencing userspace pointers. Wait is deliberately two-phase:
//! prepare_wait observes a zombie and returns the status-copy address, while
//! commit_wait reaps only after the architecture/user-memory layer has copied
//! the status successfully.

use crate::process::{self, Pid, Table, WaitObservation, WaitTarget};

#[allow(dead_code)]
#[path = "../../shared/syscall_abi.rs"]
mod abi;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    Return(u64),
    Terminated,
    WaitReady {
        pid: Pid,
        status: i32,
        status_address: u64,
    },
}

fn process_errno(error: process::Error) -> abi::Errno {
    match error {
        process::Error::Capacity | process::Error::PidExhausted => abi::Errno::NoMemory,
        process::Error::NotFound | process::Error::NoChild => abi::Errno::NotFound,
        process::Error::ParentNotRunning | process::Error::AlreadyExited => {
            abi::Errno::InvalidArgument
        }
    }
}

pub fn dispatch<const N: usize>(
    table: &mut Table<N>,
    current: Pid,
    number: u64,
    args: [u64; abi::MAX_ARGS],
) -> Result<Action, abi::Errno> {
    let call = abi::Syscall::from_number(number).ok_or(abi::Errno::NotSupported)?;
    match call {
        abi::Syscall::GetPid => Ok(Action::Return(u64::from(current.get()))),
        abi::Syscall::Exit => {
            let status = args[0] as u32 as i32;
            table.exit(current, status).map_err(process_errno)?;
            Ok(Action::Terminated)
        }
        abi::Syscall::Wait => {
            if args[2] != 0 {
                return Err(abi::Errno::InvalidArgument);
            }
            let target = if args[0] == 0 {
                WaitTarget::Any
            } else {
                WaitTarget::Pid(Pid::from_raw(args[0]).ok_or(abi::Errno::InvalidArgument)?)
            };
            match table.observe_wait(current, target).map_err(process_errno)? {
                WaitObservation::Pending => Err(abi::Errno::Busy),
                WaitObservation::Ready { pid, status } => Ok(Action::WaitReady {
                    pid,
                    status,
                    status_address: args[1],
                }),
            }
        }
        _ => Err(abi::Errno::NotSupported),
    }
}

/// Complete a prepared wait after any requested status copy-out has succeeded.
pub fn commit_wait<const N: usize>(
    table: &mut Table<N>,
    parent: Pid,
    child: Pid,
) -> Result<u64, abi::Errno> {
    table.reap(parent, child).map_err(process_errno)?;
    Ok(u64::from(child.get()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(a0: u64, a1: u64, a2: u64) -> [u64; abi::MAX_ARGS] {
        [a0, a1, a2, 0, 0, 0]
    }

    #[test]
    fn getpid_and_unknown_syscalls_are_abi_stable() {
        let mut table = Table::<4>::new();
        let init = table.spawn_init().unwrap();
        assert_eq!(
            dispatch(
                &mut table,
                init,
                abi::Syscall::GetPid.number(),
                [0; abi::MAX_ARGS]
            ),
            Ok(Action::Return(1))
        );
        assert_eq!(
            dispatch(&mut table, init, u64::MAX, [0; abi::MAX_ARGS]),
            Err(abi::Errno::NotSupported)
        );
    }

    #[test]
    fn exit_marks_current_process_zombie_and_does_not_return_value() {
        let mut table = Table::<4>::new();
        let init = table.spawn_init().unwrap();
        let child = table.spawn_child(init).unwrap();
        assert_eq!(
            dispatch(
                &mut table,
                child,
                abi::Syscall::Exit.number(),
                args((-17i32 as u32) as u64, 0, 0)
            ),
            Ok(Action::Terminated)
        );
        assert_eq!(
            table.get(child).unwrap().state,
            process::State::Zombie(-17)
        );
    }

    #[test]
    fn wait_is_two_phase_and_status_pointer_is_not_dereferenced() {
        let mut table = Table::<4>::new();
        let init = table.spawn_init().unwrap();
        let child = table.spawn_child(init).unwrap();
        table.exit(child, 23).unwrap();

        let status_address = 0x7000;
        assert_eq!(
            dispatch(
                &mut table,
                init,
                abi::Syscall::Wait.number(),
                args(u64::from(child.get()), status_address, 0)
            ),
            Ok(Action::WaitReady {
                pid: child,
                status: 23,
                status_address
            })
        );
        assert_eq!(
            table.get(child).unwrap().state,
            process::State::Zombie(23)
        );
        assert_eq!(commit_wait(&mut table, init, child), Ok(u64::from(child.get())));
        assert_eq!(table.get(child), None);
    }

    #[test]
    fn wait_pending_bad_target_and_options_fail_without_reap() {
        let mut table = Table::<4>::new();
        let init = table.spawn_init().unwrap();
        let child = table.spawn_child(init).unwrap();

        assert_eq!(
            dispatch(
                &mut table,
                init,
                abi::Syscall::Wait.number(),
                args(0, 0, 0)
            ),
            Err(abi::Errno::Busy)
        );
        assert_eq!(
            dispatch(
                &mut table,
                init,
                abi::Syscall::Wait.number(),
                args(u64::from(u32::MAX) + 1, 0, 0)
            ),
            Err(abi::Errno::InvalidArgument)
        );
        assert_eq!(
            dispatch(
                &mut table,
                init,
                abi::Syscall::Wait.number(),
                args(u64::from(child.get()), 0, 1)
            ),
            Err(abi::Errno::InvalidArgument)
        );
        assert_eq!(table.get(child).unwrap().state, process::State::Running);
    }
}
