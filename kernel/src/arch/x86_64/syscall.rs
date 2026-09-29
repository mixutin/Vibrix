//! Feature-gated x86-64 SYSCALL/SYSRETQ transport proof.
//!
//! This is the M5 fast-syscall transport boundary, not a process dispatcher.
//! The proof is deliberately single-BSP and uses one dedicated static kernel
//! stack because the SYSCALL instruction does not load TSS.RSP0.

use super::gdt::Gdt;

#[allow(dead_code)]
#[path = "../../../../shared/syscall_abi.rs"]
pub(crate) mod abi;

#[cfg(target_os = "none")]
const IA32_EFER: u32 = 0xc000_0080;
#[cfg(target_os = "none")]
const IA32_STAR: u32 = 0xc000_0081;
#[cfg(target_os = "none")]
const IA32_LSTAR: u32 = 0xc000_0082;
#[cfg(target_os = "none")]
const IA32_FMASK: u32 = 0xc000_0084;
#[cfg(target_os = "none")]
const EFER_SCE: u64 = 1;
const RFLAGS_TF: u64 = 1 << 8;
const RFLAGS_IF: u64 = 1 << 9;
const RFLAGS_DF: u64 = 1 << 10;
const RFLAGS_AC: u64 = 1 << 18;

/// STAR's SYSRET selector base. In 64-bit mode SYSRETQ derives SS as base+8
/// and CS as base+16, then forces both RPLs to 3.
pub const USER_STAR_BASE: u16 = 0x10;

pub const fn star_value() -> u64 {
    ((USER_STAR_BASE as u64) << 48) | ((Gdt::KERNEL_CODE_SELECTOR as u64) << 32)
}

pub const fn fmask_value() -> u64 {
    RFLAGS_TF | RFLAGS_IF | RFLAGS_DF | RFLAGS_AC
}

pub const fn derived_syscall_selectors() -> (u16, u16) {
    (Gdt::KERNEL_CODE_SELECTOR, Gdt::KERNEL_CODE_SELECTOR + 8)
}

pub const fn derived_sysret_selectors() -> (u16, u16) {
    ((USER_STAR_BASE + 16) | 3, (USER_STAR_BASE + 8) | 3)
}

#[cfg(target_os = "none")]
mod native {
    use super::{
        EFER_SCE, Gdt, IA32_EFER, IA32_FMASK, IA32_LSTAR, IA32_STAR, abi,
        derived_syscall_selectors, derived_sysret_selectors, fmask_value, star_value,
    };
    use core::arch::{asm, global_asm, x86_64::__cpuid_count};
    use core::cell::UnsafeCell;
    use core::sync::atomic::{AtomicBool, Ordering};
    #[cfg(feature = "process-syscall-probe")]
    use vibrix_kernel::process::{Pid, Table};
    #[cfg(feature = "process-syscall-probe")]
    use vibrix_kernel::process_syscalls::{self, Action};

    const SYSCALL_STACK_BYTES: usize = 16 * 1024;
    const SYSCALL_CPUID_BIT: u32 = 1 << 11;

    #[repr(C, align(16))]
    struct SyscallStack([u8; SYSCALL_STACK_BYTES]);

    struct StaticSyscallStack(UnsafeCell<SyscallStack>);

    // SAFETY: the feature-gated transport is single-BSP only. No AP or nested
    // syscall may share the stack until a synchronized per-CPU design exists.
    unsafe impl Sync for StaticSyscallStack {}

    static SYSCALL_STACK: StaticSyscallStack =
        StaticSyscallStack(UnsafeCell::new(SyscallStack([0; SYSCALL_STACK_BYTES])));
    static INITIALIZED: AtomicBool = AtomicBool::new(false);
    #[cfg(feature = "syscall-probe")]
    static PROBE_SEEN: AtomicBool = AtomicBool::new(false);

    static mut SYSCALL_KERNEL_RSP: u64 = 0;
    static mut SYSCALL_USER_RSP: u64 = 0;

    #[repr(C)]
    struct SavedArgs {
        r9: u64,
        r8: u64,
        r10: u64,
        rdx: u64,
        rsi: u64,
        rdi: u64,
        rcx: u64,
        r11: u64,
    }

    #[cfg(feature = "process-syscall-probe")]
    struct ProcessTableCell(UnsafeCell<Table<4>>);

    #[cfg(feature = "process-syscall-probe")]
    unsafe impl Sync for ProcessTableCell {}

    #[cfg(feature = "process-syscall-probe")]
    static PROCESS_TABLE: ProcessTableCell = ProcessTableCell(UnsafeCell::new(Table::new()));
    #[cfg(feature = "process-syscall-probe")]
    static PROCESS_READY: AtomicBool = AtomicBool::new(false);

    global_asm!(
        r#"
        .global vibrix_syscall_entry
        vibrix_syscall_entry:
            mov qword ptr [rip + {user_rsp}], rsp
            mov rsp, qword ptr [rip + {kernel_rsp}]

            push r11
            push rcx
            push rdi
            push rsi
            push rdx
            push r10
            push r8
            push r9

            mov rdi, rax
            mov rsi, rsp
            call {handler}

            pop r9
            pop r8
            pop r10
            pop rdx
            pop rsi
            pop rdi
            pop rcx
            pop r11

            mov rsp, qword ptr [rip + {user_rsp}]
            sysretq
        "#,
        user_rsp = sym SYSCALL_USER_RSP,
        kernel_rsp = sym SYSCALL_KERNEL_RSP,
        handler = sym syscall_probe_handler,
    );

    unsafe extern "C" {
        fn vibrix_syscall_entry();
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum InitError {
        AlreadyInitialized,
        Unsupported,
        InvalidSelectors,
        VerificationFailed,
    }

    fn stack_bounds() -> (u64, u64) {
        let base = SYSCALL_STACK.0.get() as u64;
        (base, base + SYSCALL_STACK_BYTES as u64)
    }

    fn stack_contains(rsp: u64) -> bool {
        let (base, top) = stack_bounds();
        (base..top).contains(&rsp)
    }

    unsafe fn read_msr(msr: u32) -> u64 {
        let low: u32;
        let high: u32;
        // SAFETY: caller runs at CPL0 on x86-64 and supplies an architectural
        // MSR number supported by the verified SYSCALL capability.
        unsafe {
            asm!(
                "rdmsr",
                in("ecx") msr,
                out("eax") low,
                out("edx") high,
                options(nomem, nostack)
            )
        };
        (u64::from(high) << 32) | u64::from(low)
    }

    unsafe fn write_msr(msr: u32, value: u64) {
        // SAFETY: caller establishes CPL0 and the exact architectural MSR.
        unsafe {
            asm!(
                "wrmsr",
                in("ecx") msr,
                in("eax") value as u32,
                in("edx") (value >> 32) as u32,
                options(nomem, nostack)
            )
        };
    }

    fn syscall_supported() -> bool {
        let max_extended = __cpuid_count(0x8000_0000, 0).eax;
        if max_extended < 0x8000_0001 {
            return false;
        }
        // AMD64 architectural SYSCALL/SYSRET capability bit.
        __cpuid_count(0x8000_0001, 0).edx & SYSCALL_CPUID_BIT != 0
    }

    /// Install the bounded single-BSP fast-syscall entry contract.
    ///
    /// # Safety
    /// Call once on the boot CPU after the permanent GDT is loaded and before
    /// entering CPL3. No other CPU may mutate these MSRs or use this stack.
    pub unsafe fn init() -> Result<(), InitError> {
        if INITIALIZED.swap(true, Ordering::SeqCst) {
            return Err(InitError::AlreadyInitialized);
        }
        if !syscall_supported() {
            INITIALIZED.store(false, Ordering::SeqCst);
            return Err(InitError::Unsupported);
        }

        let (kernel_cs, kernel_ss) = derived_syscall_selectors();
        let (user_cs, user_ss) = derived_sysret_selectors();
        if kernel_cs != Gdt::KERNEL_CODE_SELECTOR
            || kernel_ss != Gdt::KERNEL_DATA_SELECTOR
            || user_cs != Gdt::USER_CODE_SELECTOR
            || user_ss != Gdt::USER_DATA_SELECTOR
        {
            INITIALIZED.store(false, Ordering::SeqCst);
            return Err(InitError::InvalidSelectors);
        }

        let (_, stack_top) = stack_bounds();
        if stack_top == 0 || stack_top & 0xf != 0 {
            INITIALIZED.store(false, Ordering::SeqCst);
            return Err(InitError::VerificationFailed);
        }

        // SAFETY: this feature has sole ownership of both static words before
        // SCE is published. No syscall can execute until EFER.SCE is set last.
        unsafe {
            core::ptr::addr_of_mut!(SYSCALL_KERNEL_RSP).write(stack_top);
            core::ptr::addr_of_mut!(SYSCALL_USER_RSP).write(0);
        }

        let entry = vibrix_syscall_entry as *const () as u64;
        // SAFETY: CPL0, supported architectural MSRs, and valid permanent
        // kernel entry/stack addresses. Publish SCE only after STAR/LSTAR/FMASK.
        let original_efer = unsafe { read_msr(IA32_EFER) };
        unsafe {
            write_msr(IA32_STAR, star_value());
            write_msr(IA32_LSTAR, entry);
            write_msr(IA32_FMASK, fmask_value());
            write_msr(IA32_EFER, original_efer | EFER_SCE);
        }

        // SAFETY: same CPL0 architectural MSRs; reads only verify publication.
        let verified = unsafe {
            read_msr(IA32_STAR) == star_value()
                && read_msr(IA32_LSTAR) == entry
                && read_msr(IA32_FMASK) == fmask_value()
                && read_msr(IA32_EFER) & EFER_SCE != 0
        };
        if !verified {
            // SAFETY: retract SCE on verification failure so no partial entry
            // contract remains callable.
            unsafe { write_msr(IA32_EFER, original_efer) };
            INITIALIZED.store(false, Ordering::SeqCst);
            return Err(InitError::VerificationFailed);
        }

        Ok(())
    }

    #[cfg(feature = "process-syscall-probe")]
    pub fn init_process_probe() -> Result<(), InitError> {
        if !INITIALIZED.load(Ordering::SeqCst) || PROCESS_READY.swap(true, Ordering::SeqCst) {
            return Err(InitError::VerificationFailed);
        }
        // SAFETY: this feature is single-BSP and initializes the table before
        // entering userspace or enabling a process-facing syscall.
        let table = unsafe { &mut *PROCESS_TABLE.0.get() };
        // SAFETY: no live references or syscall users exist yet.
        unsafe { core::ptr::write(table, Table::new()) };
        let init = table
            .spawn_init()
            .map_err(|_| InitError::VerificationFailed)?;
        let child = table
            .spawn_child(init)
            .map_err(|_| InitError::VerificationFailed)?;
        if init != Pid::INIT || child.get() != 2 {
            PROCESS_READY.store(false, Ordering::SeqCst);
            return Err(InitError::VerificationFailed);
        }
        if !cfg!(feature = "core-utils-probe") {
            table
                .exit(child, 23)
                .map_err(|_| InitError::VerificationFailed)?;
        }
        Ok(())
    }

    #[cfg(all(feature = "process-syscall-probe", feature = "userspace-io-probe"))]
    fn encode_vfs_error(error: vibrix_kernel::vfs::Error) -> u64 {
        let errno = match error {
            vibrix_kernel::vfs::Error::BadDescriptor => abi::Errno::BadFileDescriptor,
            vibrix_kernel::vfs::Error::AccessDenied => abi::Errno::PermissionDenied,
            vibrix_kernel::vfs::Error::WouldBlock | vibrix_kernel::vfs::Error::Busy => {
                abi::Errno::Busy
            }
            vibrix_kernel::vfs::Error::NotFound => abi::Errno::NotFound,
            vibrix_kernel::vfs::Error::NoSpace => abi::Errno::NoMemory,
            vibrix_kernel::vfs::Error::InvalidPath
            | vibrix_kernel::vfs::Error::NameTooLong
            | vibrix_kernel::vfs::Error::NotDirectory
            | vibrix_kernel::vfs::Error::IsDirectory
            | vibrix_kernel::vfs::Error::NotEmpty
            | vibrix_kernel::vfs::Error::InvalidOffset => abi::Errno::InvalidArgument,
            vibrix_kernel::vfs::Error::Exists => abi::Errno::Busy,
            vibrix_kernel::vfs::Error::Unsupported => abi::Errno::NotSupported,
            _ => abi::Errno::Io,
        };
        abi::encode_error(errno)
    }

    #[cfg(all(feature = "process-syscall-probe", feature = "userspace-io-probe"))]
    fn user_path(
        address: u64,
        length: u64,
        buffer: &mut [u8; vibrix_kernel::vfs::PATH_MAX],
    ) -> Result<&str, abi::Errno> {
        let length = usize::try_from(length).map_err(|_| abi::Errno::InvalidArgument)?;
        if length == 0 || length > buffer.len() {
            return Err(abi::Errno::InvalidArgument);
        }
        crate::memory::address_space::copy_from_user(address, &mut buffer[..length])
            .map_err(|_| abi::Errno::BadAddress)?;
        core::str::from_utf8(&buffer[..length]).map_err(|_| abi::Errno::InvalidArgument)
    }

    #[cfg(feature = "process-syscall-probe")]
    fn process_probe_dispatch(number: u64, args: [u64; abi::MAX_ARGS]) -> u64 {
        if !PROCESS_READY.load(Ordering::SeqCst) {
            return abi::encode_error(abi::Errno::NotSupported);
        }
        #[cfg(feature = "userspace-desktop")]
        if let Some(result) = crate::framebuffer::desktop::dispatch(number, args) {
            return result;
        }
        // SAFETY: one BSP, FMASK cleared IF on entry, and no nested syscall
        // path exists in this bounded process proof.
        let table = unsafe { &mut *PROCESS_TABLE.0.get() };
        match process_syscalls::dispatch(table, Pid::INIT, number, args) {
            Ok(Action::Return(value)) => {
                if number == abi::Syscall::GetPid.number() && value == 1 {
                    crate::debugcon::write("VIBRIX: kernel process getpid syscall verified\r\n");
                    #[cfg(feature = "rust-init-probe")]
                    crate::debugcon::write("VIBRIX: Rust init userspace syscall reached\r\n");
                    #[cfg(feature = "package-metadata-probe")]
                    crate::debugcon::write(
                        "VIBRIX: package metadata userspace syscall reached\r\n",
                    );
                    crate::println!("kernel process syscall: getpid={}", value);
                }
                value
            }
            Ok(Action::Read {
                fd,
                address,
                length,
            }) => {
                #[cfg(feature = "userspace-io-probe")]
                {
                    let Ok(fd) = usize::try_from(fd) else {
                        return abi::encode_error(abi::Errno::BadFileDescriptor);
                    };
                    let mut buffer = [0u8; 256];
                    let count = core::cmp::min(length, buffer.len() as u64) as usize;
                    match crate::userspace_io::read(fd, &mut buffer[..count]) {
                        Ok(read) => {
                            if crate::memory::address_space::copy_to_user(address, &buffer[..read])
                                .is_err()
                            {
                                abi::encode_error(abi::Errno::BadAddress)
                            } else {
                                read as u64
                            }
                        }
                        Err(error) => encode_vfs_error(error),
                    }
                }
                #[cfg(not(feature = "userspace-io-probe"))]
                {
                    let _ = (fd, address, length);
                    abi::encode_error(abi::Errno::NotSupported)
                }
            }
            Ok(Action::Write {
                fd,
                address,
                length,
            }) => {
                #[cfg(feature = "userspace-io-probe")]
                {
                    let Ok(fd) = usize::try_from(fd) else {
                        return abi::encode_error(abi::Errno::BadFileDescriptor);
                    };
                    let mut buffer = [0u8; 256];
                    let count = core::cmp::min(length, buffer.len() as u64) as usize;
                    if crate::memory::address_space::copy_from_user(address, &mut buffer[..count])
                        .is_err()
                    {
                        return abi::encode_error(abi::Errno::BadAddress);
                    }
                    match crate::userspace_io::write(fd, &buffer[..count]) {
                        Ok(written) => written as u64,
                        Err(error) => encode_vfs_error(error),
                    }
                }
                #[cfg(not(feature = "userspace-io-probe"))]
                {
                    let _ = (fd, address, length);
                    abi::encode_error(abi::Errno::NotSupported)
                }
            }
            Ok(Action::Open {
                path,
                length,
                flags,
            }) => {
                #[cfg(feature = "userspace-io-probe")]
                {
                    let mut bytes = [0u8; vibrix_kernel::vfs::PATH_MAX];
                    let path = match user_path(path, length, &mut bytes) {
                        Ok(path) => path,
                        Err(error) => return abi::encode_error(error),
                    };
                    match crate::userspace_io::open(path, flags) {
                        Ok(fd) => fd as u64,
                        Err(error) => encode_vfs_error(error),
                    }
                }
                #[cfg(not(feature = "userspace-io-probe"))]
                {
                    let _ = (path, length, flags);
                    abi::encode_error(abi::Errno::NotSupported)
                }
            }
            Ok(Action::Close { fd }) => {
                #[cfg(feature = "userspace-io-probe")]
                {
                    let Ok(fd) = usize::try_from(fd) else {
                        return abi::encode_error(abi::Errno::BadFileDescriptor);
                    };
                    match crate::userspace_io::close(fd) {
                        Ok(()) => 0,
                        Err(error) => encode_vfs_error(error),
                    }
                }
                #[cfg(not(feature = "userspace-io-probe"))]
                {
                    let _ = fd;
                    abi::encode_error(abi::Errno::NotSupported)
                }
            }
            Ok(Action::Create { path, length })
            | Ok(Action::Mkdir { path, length })
            | Ok(Action::Remove { path, length }) => {
                #[cfg(feature = "userspace-io-probe")]
                {
                    let mut bytes = [0u8; vibrix_kernel::vfs::PATH_MAX];
                    let path_text = match user_path(path, length, &mut bytes) {
                        Ok(path) => path,
                        Err(error) => return abi::encode_error(error),
                    };
                    let result = match abi::Syscall::from_number(number) {
                        Some(abi::Syscall::Create) => crate::userspace_io::create(path_text),
                        Some(abi::Syscall::Mkdir) => crate::userspace_io::mkdir(path_text),
                        Some(abi::Syscall::Remove) => crate::userspace_io::remove(path_text),
                        _ => return abi::encode_error(abi::Errno::NotSupported),
                    };
                    match result {
                        Ok(()) => 0,
                        Err(error) => encode_vfs_error(error),
                    }
                }
                #[cfg(not(feature = "userspace-io-probe"))]
                {
                    let _ = (path, length);
                    abi::encode_error(abi::Errno::NotSupported)
                }
            }
            Ok(Action::ReadDir {
                path,
                length,
                index,
                output,
            }) => {
                #[cfg(feature = "userspace-io-probe")]
                {
                    let Ok(index) = usize::try_from(index) else {
                        return abi::encode_error(abi::Errno::InvalidArgument);
                    };
                    let mut bytes = [0u8; vibrix_kernel::vfs::PATH_MAX];
                    let path = match user_path(path, length, &mut bytes) {
                        Ok(path) => path,
                        Err(error) => return abi::encode_error(error),
                    };
                    let entry = match crate::userspace_io::entry(path, index) {
                        Ok(Some(entry)) => entry,
                        Ok(None) => return 0,
                        Err(error) => return encode_vfs_error(error),
                    };
                    let mut user = abi::DirEntry::EMPTY;
                    user.kind = match entry.kind {
                        vibrix_kernel::vfs::Kind::File => abi::ENTRY_FILE,
                        vibrix_kernel::vfs::Kind::Directory => abi::ENTRY_DIRECTORY,
                        vibrix_kernel::vfs::Kind::Device => abi::ENTRY_DEVICE,
                    };
                    let name = entry.name.as_str().as_bytes();
                    if name.len() > user.name.len() {
                        return abi::encode_error(abi::Errno::Io);
                    }
                    user.name_len = name.len() as u8;
                    user.name[..name.len()].copy_from_slice(name);
                    // SAFETY: DirEntry is repr(C), all fields are initialized
                    // byte arrays/u8 values, and its asserted layout has no
                    // padding. The slice lives only through checked copy-out.
                    let user_bytes = unsafe {
                        core::slice::from_raw_parts(
                            (&user as *const abi::DirEntry).cast::<u8>(),
                            core::mem::size_of::<abi::DirEntry>(),
                        )
                    };
                    if crate::memory::address_space::copy_to_user(output, user_bytes).is_err() {
                        abi::encode_error(abi::Errno::BadAddress)
                    } else {
                        1
                    }
                }
                #[cfg(not(feature = "userspace-io-probe"))]
                {
                    let _ = (path, length, index, output);
                    abi::encode_error(abi::Errno::NotSupported)
                }
            }
            Ok(Action::ProcessInfo { process, output }) => {
                let (state, status) = match process.state {
                    vibrix_kernel::process::State::Running => (abi::PROCESS_RUNNING, 0),
                    vibrix_kernel::process::State::Zombie(status) => (abi::PROCESS_ZOMBIE, status),
                };
                let user = abi::ProcessInfo {
                    pid: process.pid.get(),
                    parent: process.parent.map_or(0, vibrix_kernel::process::Pid::get),
                    state,
                    reserved: [0; 3],
                    status,
                };
                // SAFETY: ProcessInfo is repr(C) with every field initialized
                // and explicit reserved bytes filling the only alignment gap.
                // ABI layout assertions exclude hidden padding.
                let bytes = unsafe {
                    core::slice::from_raw_parts(
                        (&user as *const abi::ProcessInfo).cast::<u8>(),
                        core::mem::size_of::<abi::ProcessInfo>(),
                    )
                };
                if crate::memory::address_space::copy_to_user(output, bytes).is_err() {
                    abi::encode_error(abi::Errno::BadAddress)
                } else {
                    1
                }
            }
            Ok(Action::WaitReady {
                pid,
                status,
                status_address,
            }) => {
                if status_address != 0 {
                    let bytes = status.to_ne_bytes();
                    if crate::memory::address_space::copy_to_user(status_address, &bytes).is_err() {
                        return abi::encode_error(abi::Errno::BadAddress);
                    }
                }
                match process_syscalls::commit_wait(table, Pid::INIT, pid) {
                    Ok(value) => {
                        crate::debugcon::write(
                            "VIBRIX: kernel process wait copyout and reap verified\r\n",
                        );
                        crate::println!(
                            "kernel process syscall: wait pid={} status={} copied={}",
                            value,
                            status,
                            status_address != 0
                        );
                        value
                    }
                    Err(error) => 0u64.wrapping_sub(u64::from(error.code())),
                }
            }
            Ok(Action::Terminated) => {
                crate::debugcon::write("VIBRIX: kernel process exit syscall verified\r\n");
                #[cfg(feature = "rust-init-probe")]
                crate::debugcon::write("VIBRIX: Rust init PID 1 exited through syscall\r\n");
                #[cfg(feature = "package-metadata-probe")]
                if args[0] == 0 {
                    crate::debugcon::write("VIBRIX: package metadata userspace verified\r\n");
                }
                crate::println!(
                    "kernel process syscall: exit pid=1 status={}",
                    args[0] as i32
                );
                loop {
                    core::hint::spin_loop();
                }
            }
            Err(error) => 0u64.wrapping_sub(u64::from(error.code())),
        }
    }

    extern "C" fn syscall_probe_handler(number: u64, saved: *const SavedArgs) -> u64 {
        let kernel_rsp: u64;
        // SAFETY: read-only inspection of the current CPL0 stack pointer.
        unsafe {
            asm!(
                "mov {}, rsp",
                out(reg) kernel_rsp,
                options(nomem, nostack, preserves_flags)
            )
        };

        if !stack_contains(kernel_rsp) || saved.is_null() {
            crate::debugcon::write("VIBRIX: kernel SYSCALL entry validation failed\r\n");
            return abi::encode_error(abi::Errno::NotSupported);
        }

        // SAFETY: assembly passes RSP after saving these exact eight words on
        // the dedicated syscall stack, which remains live across this call.
        let saved = unsafe { &*saved };
        let args = [
            saved.rdi, saved.rsi, saved.rdx, saved.r10, saved.r8, saved.r9,
        ];

        #[cfg(feature = "process-syscall-probe")]
        {
            process_probe_dispatch(number, args)
        }

        #[cfg(not(feature = "process-syscall-probe"))]
        {
            let result = abi::encode_error(abi::Errno::NotSupported);
            if number == u64::MAX {
                #[cfg(feature = "syscall-probe")]
                PROBE_SEEN.store(true, Ordering::SeqCst);
                crate::debugcon::write("VIBRIX: kernel SYSCALL entry reached\r\n");
                crate::println!(
                    "kernel syscall probe: number={:#x} kernel_rsp={:#x} result={:#x}",
                    number,
                    kernel_rsp,
                    result
                );
            } else {
                crate::debugcon::write("VIBRIX: kernel SYSCALL entry validation failed\r\n");
            }
            result
        }
    }

    #[cfg(feature = "syscall-probe")]
    pub fn probe_observed() -> bool {
        PROBE_SEEN.load(Ordering::SeqCst)
    }
}

#[cfg(target_os = "none")]
pub use native::init;
#[cfg(all(target_os = "none", feature = "process-syscall-probe"))]
pub use native::init_process_probe;
#[cfg(all(target_os = "none", feature = "syscall-probe"))]
pub use native::probe_observed;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn star_selectors_match_gdt_and_sysret_rules() {
        let (kernel_cs, kernel_ss) = derived_syscall_selectors();
        let (user_cs, user_ss) = derived_sysret_selectors();
        assert_eq!(kernel_cs, Gdt::KERNEL_CODE_SELECTOR);
        assert_eq!(kernel_ss, Gdt::KERNEL_DATA_SELECTOR);
        assert_eq!(user_cs, Gdt::USER_CODE_SELECTOR);
        assert_eq!(user_ss, Gdt::USER_DATA_SELECTOR);
        assert_eq!(user_cs, 0x23);
        assert_eq!(user_ss, 0x1b);
        assert_eq!((star_value() >> 32) as u16, Gdt::KERNEL_CODE_SELECTOR);
        assert_eq!((star_value() >> 48) as u16, USER_STAR_BASE);
    }

    #[test]
    fn fmask_blocks_unsafe_entry_flags() {
        assert_ne!(fmask_value() & RFLAGS_IF, 0);
        assert_ne!(fmask_value() & RFLAGS_TF, 0);
        assert_ne!(fmask_value() & RFLAGS_DF, 0);
        assert_ne!(fmask_value() & RFLAGS_AC, 0);
    }

    #[test]
    fn probe_result_uses_abi_not_supported_encoding() {
        assert_eq!(
            abi::decode_result(abi::encode_error(abi::Errno::NotSupported)),
            Err(abi::Errno::NotSupported.code())
        );
    }
}
