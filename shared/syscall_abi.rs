//! Vibrix userspace/kernel syscall ABI v1.
//!
//! This file is architecture-neutral policy for the current x86-64 port. The
//! transport instruction is deliberately separate: ROADMAP tracks syscall/sysret
//! as its own implementation milestone.

pub const ABI_VERSION: u64 = 1;
pub const MAX_ARGS: usize = 6;
pub const MAX_ERRNO: u16 = 4095;

/// x86-64 v1 register contract.
///
/// Input:
/// - RAX: syscall number
/// - RDI, RSI, RDX, R10, R8, R9: arguments 0..5
///
/// Output:
/// - RAX: success value or negative errno encoded in two's-complement.
///
/// RCX and R11 are transport-clobbered once SYSCALL/SYSRET is implemented and
/// are therefore never argument registers in ABI v1.
pub const ARGUMENT_REGISTERS: [&str; MAX_ARGS] = ["rdi", "rsi", "rdx", "r10", "r8", "r9"];

#[repr(u64)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Syscall {
    Exit = 0,
    Yield = 1,
    GetPid = 2,
    Read = 3,
    Write = 4,
    Open = 5,
    Close = 6,
    Wait = 7,
    Exec = 8,
    Create = 9,
    Mkdir = 10,
    Remove = 11,
    ReadDir = 12,
    ProcessInfo = 13,
    Kill = 14,
    DisplayInfo = 15,
    DisplayFill = 16,
    DisplayBlit = 17,
    InputPoll = 18,
    GetResUid = 19,
    SetResUid = 20,
    GetResGid = 21,
    SetResGid = 22,
    NoNewPrivileges = 23,
    Promises = 24,
    FdRights = 25,
}

impl Syscall {
    pub const fn from_number(number: u64) -> Option<Self> {
        match number {
            0 => Some(Self::Exit),
            1 => Some(Self::Yield),
            2 => Some(Self::GetPid),
            3 => Some(Self::Read),
            4 => Some(Self::Write),
            5 => Some(Self::Open),
            6 => Some(Self::Close),
            7 => Some(Self::Wait),
            8 => Some(Self::Exec),
            9 => Some(Self::Create),
            10 => Some(Self::Mkdir),
            11 => Some(Self::Remove),
            12 => Some(Self::ReadDir),
            13 => Some(Self::ProcessInfo),
            14 => Some(Self::Kill),
            15 => Some(Self::DisplayInfo),
            16 => Some(Self::DisplayFill),
            17 => Some(Self::DisplayBlit),
            18 => Some(Self::InputPoll),
            19 => Some(Self::GetResUid),
            20 => Some(Self::SetResUid),
            21 => Some(Self::GetResGid),
            22 => Some(Self::SetResGid),
            23 => Some(Self::NoNewPrivileges),
            24 => Some(Self::Promises),
            25 => Some(Self::FdRights),
            _ => None,
        }
    }

    pub const fn number(self) -> u64 {
        self as u64
    }
}

pub const PROMISE_IO: u64 = 1 << 0;
pub const PROMISE_FILESYSTEM: u64 = 1 << 1;
pub const PROMISE_PROCESS: u64 = 1 << 2;
pub const PROMISE_CREDENTIALS: u64 = 1 << 3;
pub const PROMISE_ALL: u64 =
    PROMISE_IO | PROMISE_FILESYSTEM | PROMISE_PROCESS | PROMISE_CREDENTIALS;

pub const OPEN_READ: u64 = 0;
pub const OPEN_WRITE: u64 = 1;
pub const OPEN_READ_WRITE: u64 = 2;
pub const OPEN_TRUNCATE: u64 = 1 << 8;

pub const FD_RIGHT_READ: u64 = 1 << 0;
pub const FD_RIGHT_WRITE: u64 = 1 << 1;
pub const FD_RIGHT_SEEK: u64 = 1 << 2;
pub const FD_RIGHT_ALL: u64 = FD_RIGHT_READ | FD_RIGHT_WRITE | FD_RIGHT_SEEK;

pub const ENTRY_FILE: u8 = 1;
pub const ENTRY_DIRECTORY: u8 = 2;
pub const ENTRY_DEVICE: u8 = 3;
pub const ENTRY_NAME_BYTES: usize = 32;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirEntry {
    pub kind: u8,
    pub name_len: u8,
    pub reserved: [u8; 6],
    pub name: [u8; ENTRY_NAME_BYTES],
}

impl DirEntry {
    pub const EMPTY: Self = Self {
        kind: 0,
        name_len: 0,
        reserved: [0; 6],
        name: [0; ENTRY_NAME_BYTES],
    };
}

pub const ID_UNCHANGED: u64 = u32::MAX as u64;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IdTriple {
    pub real: u32,
    pub effective: u32,
    pub saved: u32,
}

impl IdTriple {
    pub const ROOT: Self = Self {
        real: 0,
        effective: 0,
        saved: 0,
    };
}

pub const PROCESS_RUNNING: u8 = 1;
pub const PROCESS_ZOMBIE: u8 = 2;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessInfo {
    pub pid: u32,
    pub parent: u32,
    pub state: u8,
    pub reserved: [u8; 3],
    pub status: i32,
}

impl ProcessInfo {
    pub const EMPTY: Self = Self {
        pid: 0,
        parent: 0,
        state: 0,
        reserved: [0; 3],
        status: 0,
    };
}

// These structures are copied as bytes across the ABI. They must not contain
// uninitialized padding, regardless of compiler layout changes elsewhere.
const _: () = assert!(core::mem::size_of::<DirEntry>() == 40);
const _: () = assert!(core::mem::align_of::<DirEntry>() == 1);
const _: () = assert!(core::mem::size_of::<ProcessInfo>() == 16);
const _: () = assert!(core::mem::size_of::<IdTriple>() == 12);
const _: () = assert!(core::mem::align_of::<IdTriple>() == 4);
const _: () = assert!(core::mem::offset_of!(ProcessInfo, status) == 12);

#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Errno {
    InvalidArgument = 1,
    BadAddress = 2,
    BadFileDescriptor = 3,
    NotFound = 4,
    NotSupported = 5,
    NoMemory = 6,
    Busy = 7,
    PermissionDenied = 8,
    Interrupted = 9,
    Io = 10,
}

impl Errno {
    pub const fn code(self) -> u16 {
        self as u16
    }
}

/// Encode an ABI v1 error return in RAX.
pub const fn encode_error(error: Errno) -> u64 {
    0u64.wrapping_sub(error.code() as u64)
}

/// Decode an ABI v1 RAX result.
///
/// Values -1 through -4095 are errors. Every other bit-pattern is a success
/// value, matching the contract documented for ABI v1.
pub const fn decode_result(raw: u64) -> Result<u64, u16> {
    let threshold = 0u64.wrapping_sub(MAX_ERRNO as u64);
    if raw >= threshold {
        Err((0u64.wrapping_sub(raw)) as u16)
    } else {
        Ok(raw)
    }
}

/// User pointers are raw virtual addresses. The kernel must validate the full
/// range before touching userspace memory; the ABI itself never treats a
/// non-zero pointer as proof that memory is mapped or accessible.
pub const fn checked_user_range(address: u64, len: u64) -> Option<(u64, u64)> {
    if len == 0 {
        return Some((address, address));
    }
    let end = match address.checked_add(len - 1) {
        Some(end) => end,
        None => return None,
    };
    // x86-64 48-bit lower-half canonical user addresses only for ABI v1.
    if address > 0x0000_7fff_ffff_ffff || end > 0x0000_7fff_ffff_ffff {
        return None;
    }
    Some((address, end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn syscall_numbers_are_stable_and_unique() {
        let calls = [
            Syscall::Exit,
            Syscall::Yield,
            Syscall::GetPid,
            Syscall::Read,
            Syscall::Write,
            Syscall::Open,
            Syscall::Close,
            Syscall::Wait,
            Syscall::Exec,
            Syscall::Create,
            Syscall::Mkdir,
            Syscall::Remove,
            Syscall::ReadDir,
            Syscall::ProcessInfo,
            Syscall::Kill,
            Syscall::DisplayInfo,
            Syscall::DisplayFill,
            Syscall::DisplayBlit,
            Syscall::InputPoll,
            Syscall::GetResUid,
            Syscall::SetResUid,
            Syscall::GetResGid,
            Syscall::SetResGid,
            Syscall::NoNewPrivileges,
            Syscall::Promises,
            Syscall::FdRights,
        ];
        for (expected, call) in calls.into_iter().enumerate() {
            assert_eq!(call.number(), expected as u64);
            assert_eq!(Syscall::from_number(expected as u64), Some(call));
        }
        assert_eq!(Syscall::from_number(calls.len() as u64), None);
    }

    #[test]
    fn error_encoding_uses_reserved_negative_window() {
        for error in [
            Errno::InvalidArgument,
            Errno::BadAddress,
            Errno::BadFileDescriptor,
            Errno::NotFound,
            Errno::NotSupported,
            Errno::NoMemory,
            Errno::Busy,
            Errno::PermissionDenied,
            Errno::Interrupted,
            Errno::Io,
        ] {
            assert_eq!(decode_result(encode_error(error)), Err(error.code()));
        }
        assert_eq!(decode_result(0), Ok(0));
        assert_eq!(decode_result(MAX_ERRNO as u64), Ok(MAX_ERRNO as u64));
        assert_eq!(
            decode_result(0u64.wrapping_sub((MAX_ERRNO as u64) + 1)),
            Ok(0u64.wrapping_sub((MAX_ERRNO as u64) + 1))
        );
    }

    #[test]
    fn user_ranges_reject_overflow_and_upper_half() {
        assert_eq!(checked_user_range(0x1000, 0), Some((0x1000, 0x1000)));
        assert_eq!(checked_user_range(0x1000, 0x20), Some((0x1000, 0x101f)));
        assert_eq!(
            checked_user_range(0x0000_7fff_ffff_ffff, 1),
            Some((0x0000_7fff_ffff_ffff, 0x0000_7fff_ffff_ffff))
        );
        assert_eq!(checked_user_range(0x0000_8000_0000_0000, 1), None);
        assert_eq!(checked_user_range(u64::MAX - 3, 8), None);
    }

    #[test]
    fn register_contract_never_uses_transport_clobbers() {
        assert_eq!(ARGUMENT_REGISTERS, ["rdi", "rsi", "rdx", "r10", "r8", "r9"]);
        assert!(!ARGUMENT_REGISTERS.contains(&"rcx"));
        assert!(!ARGUMENT_REGISTERS.contains(&"r11"));
    }
}
