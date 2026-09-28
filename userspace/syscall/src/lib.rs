#![no_std]

//! Native Rust userspace bindings for the Vibrix syscall ABI v1.
//!
//! This crate owns the x86-64 userspace instruction/register transport. It
//! intentionally does not imply that every reserved ABI call already has a
//! kernel dispatcher implementation.

use core::arch::asm;

#[path = "../../../shared/syscall_abi.rs"]
pub mod abi;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Error {
    code: u16,
}

impl Error {
    pub const fn code(self) -> u16 {
        self.code
    }

    pub const fn known(self) -> Option<abi::Errno> {
        match self.code {
            1 => Some(abi::Errno::InvalidArgument),
            2 => Some(abi::Errno::BadAddress),
            3 => Some(abi::Errno::BadFileDescriptor),
            4 => Some(abi::Errno::NotFound),
            5 => Some(abi::Errno::NotSupported),
            6 => Some(abi::Errno::NoMemory),
            7 => Some(abi::Errno::Busy),
            8 => Some(abi::Errno::PermissionDenied),
            9 => Some(abi::Errno::Interrupted),
            10 => Some(abi::Errno::Io),
            _ => None,
        }
    }
}

pub type Result<T> = core::result::Result<T, Error>;

pub const fn decode_result(raw: u64) -> Result<u64> {
    match abi::decode_result(raw) {
        Ok(value) => Ok(value),
        Err(code) => Err(Error { code }),
    }
}

/// Execute one ABI-v1 syscall with all six argument registers.
///
/// # Safety
///
/// The caller must satisfy the selected syscall semantic contract, including
/// pointer validity, aliasing and lifetime requirements for any raw addresses.
/// The kernel may terminate or replace the calling process for calls such as
/// exit or exec.
#[inline(always)]
pub unsafe fn raw(number: u64, args: [u64; abi::MAX_ARGS]) -> u64 {
    let result: u64;
    // SAFETY: the caller accepts the syscall semantic effects. The x86-64
    // instruction returns through the kernel-configured SYSRETQ path.
    // RCX/R11 are architectural clobbers. Treat every argument register as
    // clobbered too so the library does not depend on input preservation.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") number => result,
            inlateout("rdi") args[0] => _,
            inlateout("rsi") args[1] => _,
            inlateout("rdx") args[2] => _,
            inlateout("r10") args[3] => _,
            inlateout("r8") args[4] => _,
            inlateout("r9") args[5] => _,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack)
        );
    }
    result
}

#[inline(always)]
unsafe fn invoke(call: abi::Syscall, args: [u64; abi::MAX_ARGS]) -> Result<u64> {
    // SAFETY: forwarded from each wrapper argument contract.
    decode_result(unsafe { raw(call.number(), args) })
}

pub fn yield_now() -> Result<()> {
    // SAFETY: yield has no pointer or ownership arguments.
    unsafe { invoke(abi::Syscall::Yield, [0; abi::MAX_ARGS]) }.map(|_| ())
}

pub fn getpid() -> Result<u64> {
    // SAFETY: getpid has no pointer or ownership arguments.
    unsafe { invoke(abi::Syscall::GetPid, [0; abi::MAX_ARGS]) }
}

pub fn read(fd: u64, buffer: &mut [u8]) -> Result<usize> {
    let args = [
        fd,
        buffer.as_mut_ptr() as u64,
        buffer.len() as u64,
        0,
        0,
        0,
    ];
    // SAFETY: the mutable slice provides writable storage for the whole range
    // during the synchronous call.
    unsafe { invoke(abi::Syscall::Read, args) }.map(|value| value as usize)
}

pub fn write(fd: u64, buffer: &[u8]) -> Result<usize> {
    let args = [
        fd,
        buffer.as_ptr() as u64,
        buffer.len() as u64,
        0,
        0,
        0,
    ];
    // SAFETY: the immutable slice provides readable storage for the whole range
    // during the synchronous call.
    unsafe { invoke(abi::Syscall::Write, args) }.map(|value| value as usize)
}

pub fn open(path: &[u8], flags: u64) -> Result<u64> {
    let args = [
        path.as_ptr() as u64,
        path.len() as u64,
        flags,
        0,
        0,
        0,
    ];
    // SAFETY: path is readable for the complete synchronous call.
    unsafe { invoke(abi::Syscall::Open, args) }
}

pub fn close(fd: u64) -> Result<()> {
    // SAFETY: close has only a scalar descriptor argument.
    unsafe { invoke(abi::Syscall::Close, [fd, 0, 0, 0, 0, 0]) }.map(|_| ())
}

pub fn wait(pid: u64, status: Option<&mut i32>, options: u64) -> Result<u64> {
    let status_ptr = match status {
        Some(value) => value as *mut i32 as u64,
        None => 0,
    };
    // SAFETY: an optional mutable reference, when supplied, remains valid and
    // exclusively borrowed for the synchronous call.
    unsafe { invoke(abi::Syscall::Wait, [pid, status_ptr, options, 0, 0, 0]) }
}

/// Request process termination.
///
/// A successfully implemented kernel exit does not return. Returning Ok
/// therefore indicates a future dispatcher returned instead of terminating.
pub fn exit(status: u64) -> Result<u64> {
    // SAFETY: exit has only a scalar status argument.
    unsafe { invoke(abi::Syscall::Exit, [status, 0, 0, 0, 0, 0]) }
}

/// Invoke the reserved ABI-v1 exec call with raw argv/envp vector addresses.
///
/// # Safety
///
/// argv and envp must follow the process ABI that will be frozen by the later
/// executable/argv milestone and remain readable for the call.
pub unsafe fn exec_raw(path: &[u8], argv: *const u64, envp: *const u64) -> Result<u64> {
    let args = [
        path.as_ptr() as u64,
        path.len() as u64,
        argv as u64,
        envp as u64,
        0,
        0,
    ];
    // SAFETY: the caller owns the raw vector-layout validity contract.
    unsafe { invoke(abi::Syscall::Exec, args) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_success_and_reserved_errno_window() {
        assert_eq!(decode_result(0), Ok(0));
        assert_eq!(decode_result(42), Ok(42));
        assert_eq!(
            decode_result(abi::encode_error(abi::Errno::NotSupported)),
            Err(Error {
                code: abi::Errno::NotSupported.code()
            })
        );
    }

    #[test]
    fn maps_every_known_v1_errno_without_reinterpreting_unknown_codes() {
        for errno in [
            abi::Errno::InvalidArgument,
            abi::Errno::BadAddress,
            abi::Errno::BadFileDescriptor,
            abi::Errno::NotFound,
            abi::Errno::NotSupported,
            abi::Errno::NoMemory,
            abi::Errno::Busy,
            abi::Errno::PermissionDenied,
            abi::Errno::Interrupted,
            abi::Errno::Io,
        ] {
            let error = Error { code: errno.code() };
            assert_eq!(error.known(), Some(errno));
        }
        assert_eq!(Error { code: 4095 }.known(), None);
    }

    #[test]
    fn uses_the_shared_v1_register_and_number_contract() {
        assert_eq!(abi::ABI_VERSION, 1);
        assert_eq!(
            abi::ARGUMENT_REGISTERS,
            ["rdi", "rsi", "rdx", "r10", "r8", "r9"]
        );
        assert_eq!(abi::Syscall::Exit.number(), 0);
        assert_eq!(abi::Syscall::Exec.number(), 8);
    }
}
