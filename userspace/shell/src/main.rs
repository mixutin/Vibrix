#![no_std]
#![no_main]

use core::panic::PanicInfo;
use vibrix_shell::{Builtin, Command, LINE_BYTES};
use vibrix_syscall::{self as syscall, abi};

const STDIN: u64 = 0;
const STDOUT: u64 = 1;
const STDERR: u64 = 2;
const PATH_BYTES: usize = 256;
const PATH_DEPTH: usize = 16;

fn write(bytes: &[u8]) {
    let _ = syscall::write(STDOUT, bytes);
}

fn error(message: &[u8]) {
    let _ = syscall::write(STDERR, message);
}

#[derive(Clone, Copy)]
struct Path {
    bytes: [u8; PATH_BYTES],
    len: usize,
}

impl Path {
    fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

fn normalize_absolute(input: &[u8]) -> Option<Path> {
    if input.first().copied() != Some(b'/') || input.len() > PATH_BYTES {
        return None;
    }
    let mut path = Path {
        bytes: [0; PATH_BYTES],
        len: 1,
    };
    path.bytes[0] = b'/';
    let mut starts = [0usize; PATH_DEPTH];
    let mut depth = 0usize;
    let mut cursor = 1usize;

    while cursor <= input.len() {
        let start = cursor;
        while cursor < input.len() && input[cursor] != b'/' {
            cursor += 1;
        }
        let component = &input[start..cursor];
        cursor += 1;
        if component.is_empty() || component == b"." {
            continue;
        }
        if component == b".." {
            if depth != 0 {
                depth -= 1;
                path.len = starts[depth];
                if path.len == 0 {
                    path.len = 1;
                    path.bytes[0] = b'/';
                }
            }
            continue;
        }
        if depth == PATH_DEPTH || component.len() > abi::DIRECTORY_ENTRY_NAME_MAX {
            return None;
        }
        let old_len = path.len;
        if path.len > 1 {
            if path.len == PATH_BYTES {
                return None;
            }
            path.bytes[path.len] = b'/';
            path.len += 1;
        }
        let end = path.len.checked_add(component.len())?;
        if end > PATH_BYTES {
            return None;
        }
        starts[depth] = old_len;
        depth += 1;
        path.bytes[path.len..end].copy_from_slice(component);
        path.len = end;
    }
    Some(path)
}

fn absolute(input: &[u8]) -> Option<Path> {
    if input.first().copied() == Some(b'/') {
        return normalize_absolute(input);
    }
    let mut joined = [0u8; PATH_BYTES];
    let cwd_len = syscall::getcwd(&mut joined).ok()?;
    if cwd_len == 0 || cwd_len > joined.len() {
        return None;
    }
    let mut len = cwd_len;
    if len > 1 {
        if len == joined.len() {
            return None;
        }
        joined[len] = b'/';
        len += 1;
    }
    let end = len.checked_add(input.len())?;
    if end > joined.len() {
        return None;
    }
    joined[len..end].copy_from_slice(input);
    normalize_absolute(&joined[..end])
}

fn decimal(mut value: u64, out: &mut [u8; 20]) -> &[u8] {
    if value == 0 {
        out[19] = b'0';
        return &out[19..];
    }
    let mut index = out.len();
    while value != 0 {
        index -= 1;
        out[index] = b'0' + (value % 10) as u8;
        value /= 10;
    }
    &out[index..]
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

fn command_path(command: &Command<'_>, index: usize) -> Option<Path> {
    (command.argc > index)
        .then(|| absolute(command.args[index]))
        .flatten()
}

fn cat(command: &Command<'_>) {
    let Some(path) = command_path(command, 1) else {
        error(b"cat: path required\n");
        return;
    };
    let Ok(fd) = syscall::open(path.as_bytes(), abi::OPEN_READ) else {
        error(b"cat: open failed\n");
        return;
    };
    let mut buffer = [0u8; 128];
    loop {
        match syscall::read(fd, &mut buffer) {
            Ok(0) => break,
            Ok(count) => write(&buffer[..count]),
            Err(_) => {
                error(b"cat: read failed\n");
                break;
            }
        }
    }
    let _ = syscall::close(fd);
}

fn ls(command: &Command<'_>) {
    let path = if command.argc > 1 {
        command_path(command, 1)
    } else {
        let mut cwd = [0u8; PATH_BYTES];
        syscall::getcwd(&mut cwd)
            .ok()
            .and_then(|count| normalize_absolute(&cwd[..count]))
    };
    let Some(path) = path else {
        error(b"ls: invalid path\n");
        return;
    };
    let mut index = 0u64;
    loop {
        let mut entry = abi::DirectoryEntry::empty();
        match syscall::readdir(path.as_bytes(), index, &mut entry) {
            Ok(false) => break,
            Ok(true) => {
                let len = usize::from(entry.name_len).min(entry.name.len());
                write(&entry.name[..len]);
                if entry.kind == abi::DIRECTORY_KIND_DIRECTORY {
                    write(b"/");
                }
                write(b"\n");
                index += 1;
            }
            Err(_) => {
                error(b"ls: read failed\n");
                break;
            }
        }
    }
}

fn pwd() {
    let mut cwd = [0u8; PATH_BYTES];
    match syscall::getcwd(&mut cwd) {
        Ok(count) => {
            write(&cwd[..count]);
            write(b"\n");
        }
        Err(_) => error(b"pwd: failed\n"),
    }
}

fn cd(command: &Command<'_>) {
    let path = if command.argc > 1 {
        command_path(command, 1)
    } else {
        normalize_absolute(b"/")
    };
    match path {
        Some(path) if syscall::chdir(path.as_bytes()).is_ok() => {}
        _ => error(b"cd: failed\n"),
    }
}

fn mkdir(command: &Command<'_>) {
    let Some(path) = command_path(command, 1) else {
        error(b"mkdir: path required\n");
        return;
    };
    if syscall::mkdir(path.as_bytes()).is_err() {
        error(b"mkdir: failed\n");
    }
}

fn cp(command: &Command<'_>) {
    let (Some(source), Some(target)) = (command_path(command, 1), command_path(command, 2)) else {
        error(b"cp: source and target required\n");
        return;
    };
    let Ok(input) = syscall::open(source.as_bytes(), abi::OPEN_READ) else {
        error(b"cp: source open failed\n");
        return;
    };
    if syscall::create(target.as_bytes()).is_err() {
        let _ = syscall::close(input);
        error(b"cp: target create failed\n");
        return;
    }
    let Ok(output) = syscall::open(target.as_bytes(), abi::OPEN_WRITE) else {
        let _ = syscall::close(input);
        error(b"cp: target open failed\n");
        return;
    };
    let mut buffer = [0u8; 128];
    loop {
        match syscall::read(input, &mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                let mut offset = 0usize;
                while offset < count {
                    match syscall::write(output, &buffer[offset..count]) {
                        Ok(0) | Err(_) => {
                            error(b"cp: write failed\n");
                            offset = count;
                        }
                        Ok(written) => offset += written,
                    }
                }
            }
            Err(_) => {
                error(b"cp: read failed\n");
                break;
            }
        }
    }
    let _ = syscall::close(input);
    let _ = syscall::close(output);
}

fn mv(command: &Command<'_>) {
    let (Some(source), Some(target)) = (command_path(command, 1), command_path(command, 2)) else {
        error(b"mv: source and target required\n");
        return;
    };
    if syscall::rename(source.as_bytes(), target.as_bytes()).is_err() {
        error(b"mv: failed\n");
    }
}

fn rm(command: &Command<'_>) {
    let Some(path) = command_path(command, 1) else {
        error(b"rm: path required\n");
        return;
    };
    if syscall::remove(path.as_bytes()).is_err() {
        error(b"rm: failed\n");
    }
}

fn ps() {
    write(b"PID STATE\n");
    let mut index = 0u64;
    loop {
        let mut info = abi::ProcessInfo::empty();
        match syscall::process_info(index, &mut info) {
            Ok(false) => break,
            Ok(true) => {
                let mut digits = [0u8; 20];
                write(decimal(u64::from(info.pid), &mut digits));
                write(b" ");
                if info.state == abi::PROCESS_STATE_RUNNING {
                    write(b"running");
                } else if info.state == abi::PROCESS_STATE_ZOMBIE {
                    write(b"zombie");
                } else {
                    write(b"unknown");
                }
                write(b"\n");
                index += 1;
            }
            Err(_) => {
                error(b"ps: failed\n");
                break;
            }
        }
    }
}

fn kill(command: &Command<'_>) {
    let Some(pid) = (command.argc > 1)
        .then(|| parse_u64(command.args[1]))
        .flatten()
    else {
        error(b"kill: pid required\n");
        return;
    };
    if syscall::kill(pid, 9).is_err() {
        error(b"kill: failed\n");
    }
}

fn dispatch(line: &[u8]) -> bool {
    let Ok(command) = Command::parse(line) else {
        write(b"sh: too many arguments\n");
        return true;
    };
    match command.builtin {
        None if command.argc == 0 => {}
        None => write(b"sh: unknown command\n"),
        Some(Builtin::Help) => {
            write(b"cat echo ls pwd cd mkdir cp mv rm ps kill exit help\n");
        }
        Some(Builtin::Echo) => {
            for (offset, argument) in command.args[1..command.argc].iter().enumerate() {
                if offset != 0 {
                    write(b" ");
                }
                write(argument);
            }
            write(b"\n");
        }
        Some(Builtin::Cat) => cat(&command),
        Some(Builtin::Ls) => ls(&command),
        Some(Builtin::Pwd) => pwd(),
        Some(Builtin::Cd) => cd(&command),
        Some(Builtin::Mkdir) => mkdir(&command),
        Some(Builtin::Cp) => cp(&command),
        Some(Builtin::Mv) => mv(&command),
        Some(Builtin::Rm) => rm(&command),
        Some(Builtin::Ps) => ps(),
        Some(Builtin::Kill) => kill(&command),
        Some(Builtin::Exit) => return false,
    }
    true
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let mut line = [0u8; LINE_BYTES];
    let mut len = 0usize;
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
                    if !dispatch(&line[..len]) {
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
