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
    Read {
        fd: u64,
        address: u64,
        length: u64,
    },
    Write {
        fd: u64,
        address: u64,
        length: u64,
    },
    Open {
        path_address: u64,
        path_length: u64,
        flags: u64,
    },
    Close {
        fd: u64,
    },
    Create {
        path_address: u64,
        path_length: u64,
    },
    Mkdir {
        path_address: u64,
        path_length: u64,
    },
    Remove {
        path_address: u64,
        path_length: u64,
    },
    Rename {
        old_address: u64,
        old_length: u64,
        new_address: u64,
        new_length: u64,
    },
    GetCwd {
        address: u64,
        length: u64,
    },
    Chdir {
        path_address: u64,
        path_length: u64,
    },
    ReadDir {
        path_address: u64,
        path_length: u64,
        index: u64,
        entry_address: u64,
    },
    ProcessInfo {
        info: abi::ProcessInfo,
        address: u64,
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
        abi::Syscall::Read => Ok(Action::Read {
            fd: args[0],
            address: args[1],
            length: args[2],
        }),
        abi::Syscall::Write => Ok(Action::Write {
            fd: args[0],
            address: args[1],
            length: args[2],
        }),
        abi::Syscall::Open => Ok(Action::Open {
            path_address: args[0],
            path_length: args[1],
            flags: args[2],
        }),
        abi::Syscall::Close => Ok(Action::Close { fd: args[0] }),
        abi::Syscall::Create => Ok(Action::Create {
            path_address: args[0],
            path_length: args[1],
        }),
        abi::Syscall::Mkdir => Ok(Action::Mkdir {
            path_address: args[0],
            path_length: args[1],
        }),
        abi::Syscall::Remove => Ok(Action::Remove {
            path_address: args[0],
            path_length: args[1],
        }),
        abi::Syscall::Rename => Ok(Action::Rename {
            old_address: args[0],
            old_length: args[1],
            new_address: args[2],
            new_length: args[3],
        }),
        abi::Syscall::GetCwd => Ok(Action::GetCwd {
            address: args[0],
            length: args[1],
        }),
        abi::Syscall::Chdir => Ok(Action::Chdir {
            path_address: args[0],
            path_length: args[1],
        }),
        abi::Syscall::ReadDir => Ok(Action::ReadDir {
            path_address: args[0],
            path_length: args[1],
            index: args[2],
            entry_address: args[3],
        }),
        abi::Syscall::Kill => {
            let target = Pid::from_raw(args[0]).ok_or(abi::Errno::InvalidArgument)?;
            if target == current {
                return Err(abi::Errno::InvalidArgument);
            }
            table
                .exit(target, args[1] as u32 as i32)
                .map_err(process_errno)?;
            Ok(Action::Return(0))
        }
        abi::Syscall::ProcessInfo => {
            let index = usize::try_from(args[0]).map_err(|_| abi::Errno::InvalidArgument)?;
            let Some(process) = table.nth(index) else {
                return Ok(Action::Return(0));
            };
            let (state, status) = match process.state {
                process::State::Running => (abi::PROCESS_STATE_RUNNING, 0),
                process::State::Zombie(status) => (abi::PROCESS_STATE_ZOMBIE, status),
            };
            Ok(Action::ProcessInfo {
                info: abi::ProcessInfo {
                    pid: process.pid.get(),
                    parent: process.parent.map_or(0, Pid::get),
                    state,
                    reserved: [0; 3],
                    status,
                },
                address: args[1],
            })
        }
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
    fn read_and_write_preserve_raw_user_memory_arguments() {
        let mut table = Table::<4>::new();
        let init = table.spawn_init().unwrap();
        assert_eq!(
            dispatch(
                &mut table,
                init,
                abi::Syscall::Read.number(),
                args(3, 0x7000, 12)
            ),
            Ok(Action::Read {
                fd: 3,
                address: 0x7000,
                length: 12
            })
        );
        assert_eq!(
            dispatch(
                &mut table,
                init,
                abi::Syscall::Write.number(),
                args(4, 0x8000, 9)
            ),
            Ok(Action::Write {
                fd: 4,
                address: 0x8000,
                length: 9
            })
        );
    }

    #[test]
    fn filesystem_actions_preserve_untrusted_pointer_arguments() {
        let mut table = Table::<4>::new();
        let init = table.spawn_init().unwrap();
        assert_eq!(
            dispatch(
                &mut table,
                init,
                abi::Syscall::Rename.number(),
                [0x1000, 4, 0x2000, 5, 0, 0]
            ),
            Ok(Action::Rename {
                old_address: 0x1000,
                old_length: 4,
                new_address: 0x2000,
                new_length: 5,
            })
        );
        assert_eq!(
            dispatch(
                &mut table,
                init,
                abi::Syscall::ReadDir.number(),
                [0x3000, 4, 7, 0x4000, 0, 0]
            ),
            Ok(Action::ReadDir {
                path_address: 0x3000,
                path_length: 4,
                index: 7,
                entry_address: 0x4000,
            })
        );
    }

    #[test]
    fn process_info_and_kill_are_bounded_to_existing_nonself_pids() {
        let mut table = Table::<4>::new();
        let init = table.spawn_init().unwrap();
        let child = table.spawn_child(init).unwrap();
        assert_eq!(
            dispatch(
                &mut table,
                init,
                abi::Syscall::ProcessInfo.number(),
                [1, 0x5000, 0, 0, 0, 0]
            ),
            Ok(Action::ProcessInfo {
                info: abi::ProcessInfo {
                    pid: child.get(),
                    parent: init.get(),
                    state: abi::PROCESS_STATE_RUNNING,
                    reserved: [0; 3],
                    status: 0,
                },
                address: 0x5000,
            })
        );
        assert_eq!(
            dispatch(
                &mut table,
                init,
                abi::Syscall::Kill.number(),
                [u64::from(child.get()), 9, 0, 0, 0, 0]
            ),
            Ok(Action::Return(0))
        );
        assert_eq!(table.get(child).unwrap().state, process::State::Zombie(9));
        assert_eq!(
            dispatch(
                &mut table,
                init,
                abi::Syscall::Kill.number(),
                [u64::from(init.get()), 1, 0, 0, 0, 0]
            ),
            Err(abi::Errno::InvalidArgument)
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
        assert_eq!(table.get(child).unwrap().state, process::State::Zombie(-17));
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
        assert_eq!(table.get(child).unwrap().state, process::State::Zombie(23));
        assert_eq!(
            commit_wait(&mut table, init, child),
            Ok(u64::from(child.get()))
        );
        assert_eq!(table.get(child), None);
    }

    #[test]
    fn wait_pending_bad_target_and_options_fail_without_reap() {
        let mut table = Table::<4>::new();
        let init = table.spawn_init().unwrap();
        let child = table.spawn_child(init).unwrap();

        assert_eq!(
            dispatch(&mut table, init, abi::Syscall::Wait.number(), args(0, 0, 0)),
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
