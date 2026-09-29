#![no_std]

//! Native Rust wrappers for Vibrix syscall ABI v1.
//!
//! The ABI numbers and result encoding come directly from the shared contract.
//! On Vibrix x86-64 (`target_os = "none"`) the backend emits the architectural
//! `syscall` instruction. Host builds exist only so pure wrapper policy can be
//! unit-tested without accidentally invoking the host operating system.

#[path = "../../../shared/syscall_abi.rs"]
pub mod abi;

#[path = "../../../shared/display_abi.rs"]
pub mod display;

pub type Result<T = u64> = core::result::Result<T, u16>;

#[cfg(target_os = "none")]
mod native {
    use core::arch::asm;

    /// Invoke one Vibrix syscall using the ABI v1 register convention.
    ///
    /// # Safety
    /// Pointer-valued arguments, if any, must satisfy the contract of the
    /// selected syscall. The kernel is still required to validate userspace
    /// ranges before dereferencing them.
    #[inline(always)]
    pub unsafe fn raw(
        number: u64,
        arg0: u64,
        arg1: u64,
        arg2: u64,
        arg3: u64,
        arg4: u64,
        arg5: u64,
    ) -> u64 {
        let mut result = number;
        // SAFETY: caller owns the argument contract. RCX and R11 are declared
        // clobbered exactly as required by x86-64 SYSCALL/SYSRETQ.
        unsafe {
            asm!(
                "syscall",
                inlateout("rax") result,
                inlateout("rdi") arg0 => _,
                inlateout("rsi") arg1 => _,
                inlateout("rdx") arg2 => _,
                inlateout("r10") arg3 => _,
                inlateout("r8") arg4 => _,
                inlateout("r9") arg5 => _,
                lateout("rcx") _,
                lateout("r11") _,
                options(nostack)
            )
        };
        result
    }
}

#[cfg(not(target_os = "none"))]
mod native {
    use super::abi;

    /// Host-only test backend. It deliberately never executes the host OS
    /// syscall instruction because Vibrix syscall numbers are unrelated.
    #[inline(always)]
    pub unsafe fn raw(
        _number: u64,
        _arg0: u64,
        _arg1: u64,
        _arg2: u64,
        _arg3: u64,
        _arg4: u64,
        _arg5: u64,
    ) -> u64 {
        abi::encode_error(abi::Errno::NotSupported)
    }
}

/// Lowest-level Vibrix ABI v1 entry.
///
/// # Safety
/// The caller must uphold the selected syscall's pointer and lifetime rules.
#[inline(always)]
pub unsafe fn raw_syscall6(number: u64, args: [u64; abi::MAX_ARGS]) -> u64 {
    // SAFETY: forwarded unchanged from the caller's raw ABI contract.
    unsafe { native::raw(number, args[0], args[1], args[2], args[3], args[4], args[5]) }
}

#[inline(always)]
fn call(number: abi::Syscall, args: [u64; abi::MAX_ARGS]) -> Result {
    // SAFETY: named safe wrappers below construct only arguments whose memory
    // validity is represented by Rust references/slices. Raw nested-pointer
    // calls remain explicitly unsafe.
    let raw = unsafe { raw_syscall6(number.number(), args) };
    abi::decode_result(raw)
}

#[inline(always)]
fn no_args(number: abi::Syscall) -> Result {
    call(number, [0; abi::MAX_ARGS])
}

pub fn exit(status: u64) -> Result<()> {
    call(abi::Syscall::Exit, [status, 0, 0, 0, 0, 0]).map(|_| ())
}

pub fn yield_now() -> Result<()> {
    no_args(abi::Syscall::Yield).map(|_| ())
}

pub fn getpid() -> Result<u64> {
    no_args(abi::Syscall::GetPid)
}

fn id_arg(value: Option<u32>) -> u64 {
    value.map_or(abi::ID_UNCHANGED, u64::from)
}

pub fn getresuid(ids: &mut abi::IdTriple) -> Result<()> {
    call(
        abi::Syscall::GetResUid,
        [ids as *mut abi::IdTriple as u64, 0, 0, 0, 0, 0],
    )
    .map(|_| ())
}

pub fn setresuid(real: Option<u32>, effective: Option<u32>, saved: Option<u32>) -> Result<()> {
    call(
        abi::Syscall::SetResUid,
        [id_arg(real), id_arg(effective), id_arg(saved), 0, 0, 0],
    )
    .map(|_| ())
}

pub fn getresgid(ids: &mut abi::IdTriple) -> Result<()> {
    call(
        abi::Syscall::GetResGid,
        [ids as *mut abi::IdTriple as u64, 0, 0, 0, 0, 0],
    )
    .map(|_| ())
}

pub fn setresgid(real: Option<u32>, effective: Option<u32>, saved: Option<u32>) -> Result<()> {
    call(
        abi::Syscall::SetResGid,
        [id_arg(real), id_arg(effective), id_arg(saved), 0, 0, 0],
    )
    .map(|_| ())
}

pub fn no_new_privileges() -> Result<bool> {
    call(
        abi::Syscall::NoNewPrivileges,
        [0, 0, 0, 0, 0, 0],
    )
    .map(|value| value != 0)
}

pub fn set_no_new_privileges() -> Result<()> {
    call(
        abi::Syscall::NoNewPrivileges,
        [1, 0, 0, 0, 0, 0],
    )
    .map(|_| ())
}

pub fn read(fd: u64, buffer: &mut [u8]) -> Result<usize> {
    let raw = call(
        abi::Syscall::Read,
        [fd, buffer.as_mut_ptr() as u64, buffer.len() as u64, 0, 0, 0],
    )?;
    Ok(raw as usize)
}

pub fn write(fd: u64, buffer: &[u8]) -> Result<usize> {
    let raw = call(
        abi::Syscall::Write,
        [fd, buffer.as_ptr() as u64, buffer.len() as u64, 0, 0, 0],
    )?;
    Ok(raw as usize)
}

pub fn open(path: &[u8], flags: u64) -> Result<u64> {
    call(
        abi::Syscall::Open,
        [path.as_ptr() as u64, path.len() as u64, flags, 0, 0, 0],
    )
}

pub fn close(fd: u64) -> Result<()> {
    call(abi::Syscall::Close, [fd, 0, 0, 0, 0, 0]).map(|_| ())
}

pub fn wait(pid: u64, status: Option<&mut i32>, options: u64) -> Result<u64> {
    let status_ptr = status.map_or(0, |value| value as *mut i32 as u64);
    call(abi::Syscall::Wait, [pid, status_ptr, options, 0, 0, 0])
}

/// Execute a new image.
///
/// # Safety
/// `argv` and `envp` are ABI-defined userspace virtual addresses to
/// pointer-vector structures that are not yet representable by a stable safe
/// Rust type in Vibrix. They must remain valid for the duration of the syscall.
pub unsafe fn exec(path: &[u8], argv: u64, envp: u64) -> Result<u64> {
    call(
        abi::Syscall::Exec,
        [path.as_ptr() as u64, path.len() as u64, argv, envp, 0, 0],
    )
}

pub fn create(path: &[u8]) -> Result<()> {
    call(
        abi::Syscall::Create,
        [path.as_ptr() as u64, path.len() as u64, 0, 0, 0, 0],
    )
    .map(|_| ())
}

pub fn mkdir(path: &[u8]) -> Result<()> {
    call(
        abi::Syscall::Mkdir,
        [path.as_ptr() as u64, path.len() as u64, 0, 0, 0, 0],
    )
    .map(|_| ())
}

pub fn remove(path: &[u8]) -> Result<()> {
    call(
        abi::Syscall::Remove,
        [path.as_ptr() as u64, path.len() as u64, 0, 0, 0, 0],
    )
    .map(|_| ())
}

pub fn read_dir(path: &[u8], index: u64, entry: &mut abi::DirEntry) -> Result<bool> {
    let raw = call(
        abi::Syscall::ReadDir,
        [
            path.as_ptr() as u64,
            path.len() as u64,
            index,
            entry as *mut abi::DirEntry as u64,
            0,
            0,
        ],
    )?;
    Ok(raw != 0)
}

pub fn process_info(index: u64, info: &mut abi::ProcessInfo) -> Result<bool> {
    let raw = call(
        abi::Syscall::ProcessInfo,
        [index, info as *mut abi::ProcessInfo as u64, 0, 0, 0, 0],
    )?;
    Ok(raw != 0)
}

pub fn kill(pid: u64, status: i32) -> Result<()> {
    call(
        abi::Syscall::Kill,
        [pid, u64::from(status as u32), 0, 0, 0, 0],
    )
    .map(|_| ())
}

/// Query the foreground desktop display; unsupported outside that boot profile.
pub fn display_info(info: &mut display::DisplayInfo) -> Result<()> {
    call(
        abi::Syscall::DisplayInfo,
        [info as *mut _ as u64, 0, 0, 0, 0, 0],
    )
    .map(|_| ())
}

pub fn display_fill(rect: display::Rect, color: u32) -> Result<()> {
    call(
        abi::Syscall::DisplayFill,
        [
            u64::from(rect.x),
            u64::from(rect.y),
            u64::from(rect.width),
            u64::from(rect.height),
            u64::from(color),
            0,
        ],
    )
    .map(|_| ())
}

pub fn display_blit(rect: display::Rect, pixels: &[u32]) -> Result<()> {
    if rect.pixels() != Some(pixels.len()) || pixels.len() > display::MAX_BLIT_PIXELS {
        return Err(abi::Errno::InvalidArgument.code());
    }
    call(
        abi::Syscall::DisplayBlit,
        [
            u64::from(rect.x),
            u64::from(rect.y),
            u64::from(rect.width),
            u64::from(rect.height),
            pixels.as_ptr() as u64,
            pixels.len() as u64,
        ],
    )
    .map(|_| ())
}

/// Nonblocking event delivery. No event returns false and an empty event.
pub fn input_poll(event: &mut display::InputEvent) -> Result<bool> {
    call(
        abi::Syscall::InputPoll,
        [event as *mut _ as u64, 0, 0, 0, 0, 0],
    )
    .map(|v| v != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_backend_never_executes_host_syscalls() {
        assert_eq!(getpid(), Err(abi::Errno::NotSupported.code()));
        assert_eq!(yield_now(), Err(abi::Errno::NotSupported.code()));
        let mut ids = abi::IdTriple::ROOT;
        assert_eq!(getresuid(&mut ids), Err(abi::Errno::NotSupported.code()));
        assert_eq!(
            setresuid(None, Some(1000), None),
            Err(abi::Errno::NotSupported.code())
        );
    }

    #[test]
    fn slice_wrappers_are_host_safe_and_preserve_error_encoding() {
        let mut read_buffer = [0u8; 8];
        assert_eq!(
            read(3, &mut read_buffer),
            Err(abi::Errno::NotSupported.code())
        );
        assert_eq!(write(4, b"vibrix"), Err(abi::Errno::NotSupported.code()));
        assert_eq!(open(b"/test", 0), Err(abi::Errno::NotSupported.code()));
    }

    #[test]
    fn named_wrappers_use_v1_numbers() {
        assert_eq!(abi::Syscall::Exit.number(), 0);
        assert_eq!(abi::Syscall::Yield.number(), 1);
        assert_eq!(abi::Syscall::GetPid.number(), 2);
        assert_eq!(abi::Syscall::Read.number(), 3);
        assert_eq!(abi::Syscall::Write.number(), 4);
        assert_eq!(abi::Syscall::Open.number(), 5);
        assert_eq!(abi::Syscall::Close.number(), 6);
        assert_eq!(abi::Syscall::Wait.number(), 7);
        assert_eq!(abi::Syscall::Exec.number(), 8);
        assert_eq!(abi::Syscall::Create.number(), 9);
        assert_eq!(abi::Syscall::Mkdir.number(), 10);
        assert_eq!(abi::Syscall::Remove.number(), 11);
        assert_eq!(abi::Syscall::ReadDir.number(), 12);
        assert_eq!(abi::Syscall::ProcessInfo.number(), 13);
        assert_eq!(abi::Syscall::Kill.number(), 14);
        assert_eq!(abi::Syscall::GetResUid.number(), 19);
        assert_eq!(abi::Syscall::SetResUid.number(), 20);
        assert_eq!(abi::Syscall::GetResGid.number(), 21);
        assert_eq!(abi::Syscall::SetResGid.number(), 22);
        assert_eq!(abi::Syscall::NoNewPrivileges.number(), 23);
    }
}
