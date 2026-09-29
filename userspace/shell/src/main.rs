#![no_std]
#![no_main]

use core::panic::PanicInfo;
use vibrix_shell::{
    Builtin, Command, LINE_BYTES,
    path::{PATH_BYTES, WorkingDir, same_path},
};
use vibrix_syscall::{self as syscall, abi};

const STDIN: u64 = 0;
const STDOUT: u64 = 1;
const STDERR: u64 = 2;
fn write_all(fd: u64, mut bytes: &[u8]) -> bool {
    while !bytes.is_empty() {
        match syscall::write(fd, bytes) {
            Ok(0) | Err(_) => return false,
            Ok(count) => bytes = &bytes[count..],
        }
    }
    true
}

fn write(bytes: &[u8]) {
    let _ = write_all(STDOUT, bytes);
}

fn error(message: &[u8]) {
    let _ = write_all(STDERR, message);
}

fn parse_u64(bytes: &[u8]) -> Option<u64> {
    if bytes.is_empty() {
        return None;
    }
    let mut value = 0u64;
    for &byte in bytes {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add(u64::from(byte - b'0'))?;
    }
    Some(value)
}

fn write_u64(mut value: u64) {
    let mut digits = [0u8; 20];
    let mut cursor = digits.len();
    loop {
        cursor -= 1;
        digits[cursor] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    write(&digits[cursor..]);
}

fn resolve<'a>(
    cwd: &WorkingDir,
    input: &[u8],
    buffer: &'a mut [u8; PATH_BYTES],
) -> Option<&'a [u8]> {
    let len = cwd.resolve(input, buffer)?;
    Some(&buffer[..len])
}

fn command_cat(cwd: &WorkingDir, arg: &[u8]) -> bool {
    let mut path = [0u8; PATH_BYTES];
    let Some(path) = resolve(cwd, arg, &mut path) else {
        return false;
    };
    let Ok(fd) = syscall::open(path, abi::OPEN_READ) else {
        return false;
    };
    let mut buffer = [0u8; 128];
    let mut ok = true;
    loop {
        match syscall::read(fd, &mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                if !write_all(STDOUT, &buffer[..count]) {
                    ok = false;
                    break;
                }
            }
            Err(_) => {
                ok = false;
                break;
            }
        }
    }
    let _ = syscall::close(fd);
    ok
}

fn command_ls(cwd: &WorkingDir, arg: Option<&[u8]>) -> bool {
    let mut path = [0u8; PATH_BYTES];
    let source = arg.unwrap_or(cwd.as_bytes());
    let Some(path) = resolve(cwd, source, &mut path) else {
        return false;
    };
    let mut index = 0u64;
    loop {
        let mut entry = abi::DirEntry::EMPTY;
        match syscall::read_dir(path, index, &mut entry) {
            Ok(false) => return true,
            Ok(true) => {
                let len = usize::from(entry.name_len).min(entry.name.len());
                write(&entry.name[..len]);
                if entry.kind == abi::ENTRY_DIRECTORY {
                    write(b"/");
                }
                write(b"\n");
                index += 1;
            }
            Err(_) => return false,
        }
    }
}

fn command_cd(cwd: &mut WorkingDir, arg: &[u8]) -> bool {
    let mut path = [0u8; PATH_BYTES];
    let Some(path) = resolve(cwd, arg, &mut path) else {
        return false;
    };
    // READDIR is also the bounded directory-kind validation operation.
    let mut entry = abi::DirEntry::EMPTY;
    if syscall::read_dir(path, 0, &mut entry).is_err() {
        return false;
    }
    cwd.set(path).is_some()
}

fn command_mkdir(cwd: &WorkingDir, arg: &[u8]) -> bool {
    let mut path = [0u8; PATH_BYTES];
    resolve(cwd, arg, &mut path).is_some_and(|path| syscall::mkdir(path).is_ok())
}

fn command_rm(cwd: &WorkingDir, arg: &[u8]) -> bool {
    let mut path = [0u8; PATH_BYTES];
    resolve(cwd, arg, &mut path).is_some_and(|path| syscall::remove(path).is_ok())
}

fn copy_file(cwd: &WorkingDir, source: &[u8], destination: &[u8]) -> bool {
    let mut source_path = [0u8; PATH_BYTES];
    let Some(source_path) = resolve(cwd, source, &mut source_path) else {
        return false;
    };
    let mut destination_path = [0u8; PATH_BYTES];
    let Some(destination_path) = resolve(cwd, destination, &mut destination_path) else {
        return false;
    };
    if same_path(source_path, destination_path) {
        return false;
    }
    let Ok(source_fd) = syscall::open(source_path, abi::OPEN_READ) else {
        return false;
    };
    let _ = syscall::create(destination_path);
    let destination_flags = abi::OPEN_WRITE | abi::OPEN_TRUNCATE;
    let Ok(destination_fd) = syscall::open(destination_path, destination_flags) else {
        let _ = syscall::close(source_fd);
        return false;
    };

    let mut buffer = [0u8; 128];
    let mut ok = true;
    loop {
        match syscall::read(source_fd, &mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                if !write_all(destination_fd, &buffer[..count]) {
                    ok = false;
                    break;
                }
            }
            Err(_) => {
                ok = false;
                break;
            }
        }
    }
    let _ = syscall::close(source_fd);
    let _ = syscall::close(destination_fd);
    ok
}

fn command_ps() -> bool {
    write(b"PID PPID STATE\n");
    let mut index = 0u64;
    loop {
        let mut info = abi::ProcessInfo::EMPTY;
        match syscall::process_info(index, &mut info) {
            Ok(false) => return true,
            Ok(true) => {
                write_u64(u64::from(info.pid));
                write(b" ");
                write_u64(u64::from(info.parent));
                write(b" ");
                match info.state {
                    abi::PROCESS_RUNNING => write(b"running"),
                    abi::PROCESS_ZOMBIE => write(b"zombie"),
                    _ => write(b"unknown"),
                }
                write(b"\n");
                index += 1;
            }
            Err(code) if code == abi::Errno::NotFound.code() => return true,
            Err(_) => return false,
        }
    }
}

fn dispatch(line: &[u8], cwd: &mut WorkingDir) -> bool {
    let Ok(command) = Command::parse(line) else {
        error(b"sh: too many arguments\n");
        return true;
    };
    let succeeded = match command.builtin {
        None if command.argc == 0 => true,
        None => {
            error(b"sh: unknown command\n");
            true
        }
        Some(Builtin::Help) => {
            write(b"cat echo ls pwd cd mkdir cp mv rm ps kill exit help\n");
            true
        }
        Some(Builtin::Echo) => {
            for (offset, argument) in command.args[1..command.argc].iter().enumerate() {
                if offset != 0 {
                    write(b" ");
                }
                write(argument);
            }
            write(b"\n");
            true
        }
        Some(Builtin::Pwd) if command.argc == 1 => {
            write(cwd.as_bytes());
            write(b"\n");
            true
        }
        Some(Builtin::Cat) if command.argc == 2 => command_cat(cwd, command.args[1]),
        Some(Builtin::Ls) if command.argc <= 2 => {
            command_ls(cwd, (command.argc == 2).then_some(command.args[1]))
        }
        Some(Builtin::Cd) if command.argc == 2 => command_cd(cwd, command.args[1]),
        Some(Builtin::Mkdir) if command.argc == 2 => command_mkdir(cwd, command.args[1]),
        Some(Builtin::Rm) if command.argc == 2 => command_rm(cwd, command.args[1]),
        Some(Builtin::Cp) if command.argc == 3 => copy_file(cwd, command.args[1], command.args[2]),
        Some(Builtin::Mv) if command.argc == 3 => {
            copy_file(cwd, command.args[1], command.args[2]) && command_rm(cwd, command.args[1])
        }
        Some(Builtin::Ps) if command.argc == 1 => command_ps(),
        Some(Builtin::Kill) if command.argc == 2 || command.argc == 3 => {
            let pid = parse_u64(command.args[1]);
            let status = if command.argc == 3 {
                parse_u64(command.args[2]).and_then(|value| i32::try_from(value).ok())
            } else {
                Some(143)
            };
            match (pid, status) {
                (Some(pid), Some(status)) => syscall::kill(pid, status).is_ok(),
                _ => false,
            }
        }
        Some(Builtin::Exit) if command.argc == 1 => return false,
        Some(_) => false,
    };
    if !succeeded {
        error(b"sh: command failed\n");
    }
    true
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let mut line = [0u8; LINE_BYTES];
    let mut len = 0usize;
    let mut cwd = WorkingDir::root();
    write(b"Vibrix shell\nvibrix$ ");

    loop {
        let mut byte = [0u8; 1];
        match syscall::read(STDIN, &mut byte) {
            Ok(0) => {
                let _ = syscall::yield_now();
            }
            Ok(_) => match byte[0] {
                b'\n' | b'\r' => {
                    write(b"\n");
                    if !dispatch(&line[..len], &mut cwd) {
                        let _ = syscall::exit(0);
                    }
                    len = 0;
                    write(b"vibrix$ ");
                }
                8 | 127 => {
                    len = len.saturating_sub(1);
                }
                value if len < line.len() => {
                    line[len] = value;
                    len += 1;
                }
                _ => {}
            },
            Err(_) => {
                let _ = syscall::yield_now();
            }
        }
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    let _ = syscall::write(STDERR, b"Vibrix shell panic\n");
    loop {
        core::hint::spin_loop();
    }
}
