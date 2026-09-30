//! Native shell execution policy, also exercised by the host System fixtures.

use crate::manual::{self, Builtin, MANUALS};
use crate::parser::{Command, LINE_BYTES};
use crate::path::{PATH_BYTES, WorkingDir, same_path};
use crate::system::{self, System, number, parse_number, write_all};
use crate::{fetch, text};
use vibrix_syscall::abi;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Usage,
    Os(u16),
    Message(&'static [u8]),
}

pub type Result<T = ()> = core::result::Result<T, Error>;

impl From<u16> for Error {
    fn from(code: u16) -> Self {
        Self::Os(code)
    }
}

impl Error {
    fn message(self) -> &'static [u8] {
        match self {
            Self::Usage => b"invalid arguments; use help COMMAND",
            Self::Message(message) => message,
            Self::Os(code) => match code {
                1 => b"invalid argument or path kind",
                2 => b"invalid memory address",
                3 => b"bad file descriptor",
                4 => b"no such file or process",
                5 => b"operation not supported",
                6 => b"capacity exhausted",
                7 => b"resource busy or entry already exists",
                8 => b"permission denied",
                9 => b"operation interrupted",
                _ => b"I/O operation failed",
            },
        }
    }
}

struct Path {
    bytes: [u8; PATH_BYTES],
    len: usize,
}

impl Path {
    fn resolve(cwd: &WorkingDir, input: &[u8]) -> Result<Self> {
        let mut path = Self {
            bytes: [0; PATH_BYTES],
            len: 0,
        };
        path.len = cwd
            .resolve(input, &mut path.bytes)
            .ok_or(Error::Message(b"empty or oversized path"))?;
        Ok(path)
    }

    fn bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

pub struct Shell {
    cwd: WorkingDir,
    previous: WorkingDir,
    has_previous: bool,
    pub status: u8,
    pub exit: Option<u8>,
    history: [[u8; LINE_BYTES]; 4],
    history_len: [usize; 4],
    history_next: usize,
    history_count: usize,
}

impl Default for Shell {
    fn default() -> Self {
        Self::new()
    }
}

impl Shell {
    pub const fn new() -> Self {
        Self {
            cwd: WorkingDir::root(),
            previous: WorkingDir::root(),
            has_previous: false,
            status: 0,
            exit: None,
            history: [[0; LINE_BYTES]; 4],
            history_len: [0; 4],
            history_next: 0,
            history_count: 0,
        }
    }

    pub fn run(&mut self, io: &mut dyn System, line: &[u8]) -> u8 {
        let command = match Command::parse(line) {
            Ok(command) => command,
            Err(error) => {
                let _ = write_all(io, 2, b"sh: ");
                let _ = write_all(io, 2, error.message());
                let _ = write_all(io, 2, b"\n");
                self.status = 2;
                return self.status;
            }
        };
        if command.argc == 0 {
            return self.status;
        }
        self.history[self.history_next][..line.len()].copy_from_slice(line);
        self.history_len[self.history_next] = line.len();
        self.history_next = (self.history_next + 1) % self.history.len();
        self.history_count = (self.history_count + 1).min(self.history.len());
        let Some(kind) = command.builtin else {
            let _ = write_all(io, 2, b"sh: unknown command: ");
            let _ = write_all(io, 2, command.arg(0));
            let _ = write_all(io, 2, b"; try help or apropos\n");
            self.status = 127;
            return self.status;
        };
        // Help must not execute even a destructive built-in or open a redirect.
        let result = if command.argc == 2 && command.arg(1) == b"--help" {
            show_help(io, 1, Some(command.arg(0))).map(|()| 0)
        } else {
            self.redirected(io, &command, kind)
        };
        self.status = match result {
            Ok(status) => status,
            Err(error) => {
                let _ = write_all(io, 2, b"sh: command failed: ");
                let _ = write_all(io, 2, command.arg(0));
                let _ = write_all(io, 2, b": ");
                let _ = write_all(io, 2, error.message());
                let _ = write_all(io, 2, b"\n");
                if error == Error::Usage {
                    let _ = show_help(io, 2, Some(command.arg(0)));
                }
                if error == Error::Usage || kind == Builtin::Grep {
                    2
                } else {
                    1
                }
            }
        };
        self.status
    }

    fn redirected(&mut self, io: &mut dyn System, command: &Command, kind: Builtin) -> Result<u8> {
        let input_path = command
            .input()
            .map(|arg| Path::resolve(&self.cwd, arg))
            .transpose()?;
        let output_path = command
            .output()
            .map(|arg| Path::resolve(&self.cwd, arg))
            .transpose()?;
        if let Some(output) = &output_path {
            if input_path
                .as_ref()
                .is_some_and(|input| same_path(input.bytes(), output.bytes()))
            {
                return Err(Error::Message(b"input and output refer to the same path"));
            }
            // Conservatively protect file operands before opening a truncating
            // redirect. Extra false-positive refusals are preferable to loss.
            if matches!(
                kind,
                Builtin::Cat
                    | Builtin::Cp
                    | Builtin::Mv
                    | Builtin::Head
                    | Builtin::Tail
                    | Builtin::Wc
                    | Builtin::Grep
                    | Builtin::Sort
                    | Builtin::Uniq
                    | Builtin::Nl
                    | Builtin::Hexdump
            ) {
                for arg in &command.argv()[1..command.argc] {
                    if let Ok(input) = Path::resolve(&self.cwd, arg)
                        && same_path(input.bytes(), output.bytes())
                    {
                        return Err(Error::Message(b"refusing to truncate a command input"));
                    }
                }
            }
        }
        let input = input_path
            .as_ref()
            .map(|path| {
                require_regular(io, path.bytes())?;
                io.open(path.bytes(), abi::OPEN_READ).map_err(Error::from)
            })
            .transpose()?;
        let output = match output_path
            .as_ref()
            .map(|path| open_output(io, path.bytes()))
            .transpose()
        {
            Ok(fd) => fd,
            Err(error) => {
                if let Some(fd) = input {
                    let _ = io.close(fd);
                }
                return Err(error);
            }
        };
        let mut result = self.execute(io, command, kind, input, output.unwrap_or(1));
        for fd in [input, output].into_iter().flatten() {
            if let Err(code) = io.close(fd)
                && result.is_ok()
            {
                result = Err(code.into());
            }
        }
        result
    }

    fn execute(
        &mut self,
        io: &mut dyn System,
        command: &Command,
        kind: Builtin,
        input: Option<u64>,
        out: u64,
    ) -> Result<u8> {
        let argv = command.argv();
        let raw = &argv[1..command.argc];
        let args = raw.strip_prefix(&[&b"--"[..]]).unwrap_or(raw);
        match kind {
            Builtin::Help => {
                if args.len() > 1 {
                    return Err(Error::Usage);
                }
                show_help(io, out, args.first().copied())?;
            }
            Builtin::Man => {
                if args.len() == 2 && args[0] == b"-k" {
                    return search_manuals(io, out, args[1]);
                }
                let args = args.strip_prefix(&[&b"1"[..]]).unwrap_or(args);
                if args.len() != 1 {
                    return Err(Error::Usage);
                }
                show_manual(io, out, args[0])?;
            }
            Builtin::Apropos => {
                if args.len() != 1 {
                    return Err(Error::Usage);
                }
                return search_manuals(io, out, args[0]);
            }
            Builtin::Which => {
                if args.is_empty() {
                    return Err(Error::Usage);
                }
                let mut status = 0;
                for &arg in args {
                    write_all(io, out, arg)?;
                    match manual::lookup(arg) {
                        Some(page) => {
                            write_all(io, out, b": shell builtin ")?;
                            write_all(io, out, page.name)?;
                        }
                        None => {
                            write_all(io, out, b": not found")?;
                            status = 1;
                        }
                    }
                    write_all(io, out, b"\n")?;
                }
                return Ok(status);
            }
            Builtin::Echo => {
                let newline = raw.first() != Some(&&b"-n"[..]);
                let args = if newline { raw } else { &raw[1..] };
                let args = args.strip_prefix(&[&b"--"[..]]).unwrap_or(args);
                echo(io, out, args, newline)?;
            }
            Builtin::Pwd => {
                require_empty(args)?;
                write_all(io, out, self.cwd.as_bytes())?;
                write_all(io, out, b"\n")?;
            }
            Builtin::Cd => {
                if args.len() > 1 {
                    return Err(Error::Usage);
                }
                let target = args.first().copied().unwrap_or(b"/");
                let target = if target == b"-" {
                    if !self.has_previous {
                        return Err(Error::Message(b"no previous directory"));
                    }
                    self.previous.as_bytes()
                } else {
                    target
                };
                let path = Path::resolve(&self.cwd, target)?;
                directory(io, path.bytes())?;
                self.previous.set(self.cwd.as_bytes()).ok_or(Error::Usage)?;
                self.cwd.set(path.bytes()).ok_or(Error::Usage)?;
                self.has_previous = true;
            }
            Builtin::Ls => {
                let all = raw.first() == Some(&&b"-a"[..]);
                let args = if all { &raw[1..] } else { raw };
                let args = operands(args)?;
                if args.len() > 1 {
                    return Err(Error::Usage);
                }
                let path = Path::resolve(
                    &self.cwd,
                    args.first().copied().unwrap_or(self.cwd.as_bytes()),
                )?;
                for index in 0..1024 {
                    let mut entry = abi::DirEntry::EMPTY;
                    if !io.read_dir(path.bytes(), index, &mut entry)? {
                        return Ok(0);
                    }
                    let name = entry_name(&entry)?;
                    if !all && name.starts_with(b".") {
                        continue;
                    }
                    write_all(io, out, name)?;
                    if entry.kind == abi::ENTRY_DIRECTORY {
                        write_all(io, out, b"/")?;
                    }
                    write_all(io, out, b"\n")?;
                }
                return Err(Error::Message(b"directory enumeration limit reached"));
            }
            Builtin::Cat => {
                let args = operands(raw)?;
                if args.is_empty() {
                    stream(
                        io,
                        input.ok_or(Error::Message(b"provide a file or < INPUT"))?,
                        out,
                    )?;
                } else {
                    for &arg in args {
                        let path = Path::resolve(&self.cwd, arg)?;
                        require_regular(io, path.bytes())?;
                        let fd = io.open(path.bytes(), abi::OPEN_READ)?;
                        let result = stream(io, fd, out);
                        finish(io, fd, result)?;
                    }
                }
            }
            Builtin::Mkdir | Builtin::Touch | Builtin::Rm | Builtin::Rmdir => {
                let args = operands(raw)?;
                if args.is_empty() {
                    return Err(Error::Usage);
                }
                for &arg in args {
                    let path = Path::resolve(&self.cwd, arg)?;
                    match kind {
                        Builtin::Mkdir => io.mkdir(path.bytes())?,
                        Builtin::Touch => match io.open(path.bytes(), abi::OPEN_READ) {
                            Ok(fd) => io.close(fd)?,
                            Err(code) if code == abi::Errno::NotFound.code() => {
                                io.create(path.bytes())?
                            }
                            Err(code) => return Err(code.into()),
                        },
                        Builtin::Rm => {
                            if directory(io, path.bytes()).is_ok() {
                                return Err(Error::Message(b"is a directory; use rmdir"));
                            }
                            io.remove(path.bytes())?;
                        }
                        Builtin::Rmdir => {
                            directory(io, path.bytes())?;
                            io.remove(path.bytes())?;
                        }
                        _ => unreachable!(),
                    }
                }
            }
            Builtin::Write => {
                let args = operands(raw)?;
                if args.is_empty() {
                    return Err(Error::Usage);
                }
                let path = Path::resolve(&self.cwd, args[0])?;
                let fd = open_output(io, path.bytes())?;
                let result = echo(io, fd, &args[1..], true);
                finish(io, fd, result)?;
            }
            Builtin::Cp | Builtin::Mv => {
                let args = operands(raw)?;
                if args.len() != 2 {
                    return Err(Error::Usage);
                }
                let source = Path::resolve(&self.cwd, args[0])?;
                let destination = Path::resolve(&self.cwd, args[1])?;
                if same_path(source.bytes(), destination.bytes()) {
                    return Err(Error::Message(b"source and destination are the same file"));
                }
                require_regular(io, source.bytes())?;
                let source_fd = io.open(source.bytes(), abi::OPEN_READ)?;
                let result = match open_output(io, destination.bytes()) {
                    Ok(fd) => {
                        let result = stream(io, source_fd, fd);
                        finish(io, fd, result)
                    }
                    Err(error) => Err(error),
                };
                finish(io, source_fd, result)?;
                if kind == Builtin::Mv {
                    io.remove(source.bytes())?;
                }
            }
            Builtin::Head
            | Builtin::Tail
            | Builtin::Wc
            | Builtin::Grep
            | Builtin::Sort
            | Builtin::Uniq
            | Builtin::Nl
            | Builtin::Hexdump => {
                let options = text::Options::parse(kind, raw)?;
                let opened = options
                    .file
                    .map(|arg| {
                        let path = Path::resolve(&self.cwd, arg)?;
                        require_regular(io, path.bytes())?;
                        io.open(path.bytes(), abi::OPEN_READ).map_err(Error::from)
                    })
                    .transpose()?;
                let fd = opened
                    .or(input)
                    .ok_or(Error::Message(b"provide a file or < INPUT"))?;
                let mut bytes = [0; text::TEXT_BYTES];
                let result = load_text(io, fd, &mut bytes).and_then(|len| {
                    options.render(&bytes[..len], |part| {
                        write_all(io, out, part).map_err(Error::from)
                    })
                });
                return if opened.is_some() {
                    finish(io, fd, result)
                } else {
                    result
                };
            }
            Builtin::Basename | Builtin::Dirname => {
                let args = operands(raw)?;
                if args.is_empty() || args.len() > if kind == Builtin::Basename { 2 } else { 1 } {
                    return Err(Error::Usage);
                }
                let mut name = if kind == Builtin::Basename {
                    text::basename(args[0])
                } else {
                    text::dirname(args[0])
                };
                if args.len() == 2 && !args[1].is_empty() && args[1].len() < name.len() {
                    name = name.strip_suffix(args[1]).unwrap_or(name);
                }
                write_all(io, out, name)?;
                write_all(io, out, b"\n")?;
            }
            Builtin::Ps => {
                require_empty(args)?;
                write_all(io, out, b"PID PPID STATE\n")?;
                for index in 0..1024 {
                    let mut info = abi::ProcessInfo::EMPTY;
                    match io.process_info(index, &mut info) {
                        Ok(false) => return Ok(0),
                        Err(code) if code == abi::Errno::NotFound.code() => return Ok(0),
                        Err(code) => return Err(code.into()),
                        Ok(true) => {}
                    }
                    number(io, out, u64::from(info.pid))?;
                    write_all(io, out, b" ")?;
                    number(io, out, u64::from(info.parent))?;
                    write_all(
                        io,
                        out,
                        if info.state == abi::PROCESS_RUNNING {
                            b" running\n"
                        } else if info.state == abi::PROCESS_ZOMBIE {
                            b" zombie\n"
                        } else {
                            b" unknown\n"
                        },
                    )?;
                }
                return Err(Error::Message(b"process enumeration limit reached"));
            }
            Builtin::Kill => {
                if args.is_empty() || args.len() > 2 {
                    return Err(Error::Usage);
                }
                let pid = parse_number(args[0]).ok_or(Error::Usage)?;
                let status = if args.len() == 2 {
                    parse_number(args[1])
                        .and_then(|value| i32::try_from(value).ok())
                        .ok_or(Error::Usage)?
                } else {
                    143
                };
                io.kill(pid, status)?;
            }
            Builtin::Pid => {
                require_empty(args)?;
                let pid = io.getpid()?;
                number(io, out, pid)?;
                write_all(io, out, b"\n")?;
            }
            Builtin::Vibrix => match args {
                [b"status"] => render_vibrix_status(io, out)?,
                [b"doctor"] => render_vibrix_doctor(io, out)?,
                [b"doctor", b"--bundle"] => render_vibrix_doctor_bundle(io, out)?,
                [b"compat-report", b"--anonymized"] => render_vibrix_compatibility_report(io, out)?,
                _ => return Err(Error::Usage),
            },
            Builtin::Fetch => {
                require_empty(args)?;
                render_fetch(io, out)?;
            }
            Builtin::Uname => {
                let value = match args {
                    [] => &b"Vibrix"[..],
                    [b"-s"] => b"Vibrix",
                    [b"-a"] => b"Vibrix x86_64 native Rust userspace",
                    [b"-m"] => b"x86_64",
                    [b"-r"] => env!("CARGO_PKG_VERSION").as_bytes(),
                    _ => return Err(Error::Usage),
                };
                write_all(io, out, value)?;
                write_all(io, out, b"\n")?;
            }
            Builtin::Sysctl => render_sysctl(io, out, args)?,
            Builtin::Clear => {
                require_empty(args)?;
                write_all(io, out, b"\x0c")?;
            }
            Builtin::True | Builtin::False => {
                require_empty(args)?;
                return Ok(u8::from(kind == Builtin::False));
            }
            Builtin::Status => {
                require_empty(args)?;
                number(io, out, u64::from(self.status))?;
                write_all(io, out, b"\n")?;
            }
            Builtin::History => {
                require_empty(args)?;
                let start = (self.history_next + self.history.len() - self.history_count)
                    % self.history.len();
                for index in 0..self.history_count {
                    let slot = (start + index) % self.history.len();
                    number(io, out, (index + 1) as u64)?;
                    write_all(io, out, b" ")?;
                    write_all(io, out, &self.history[slot][..self.history_len[slot]])?;
                    write_all(io, out, b"\n")?;
                }
            }
            Builtin::Exit => {
                if args.len() > 1 {
                    return Err(Error::Usage);
                }
                let status = match args.first() {
                    Some(arg) => parse_number(arg)
                        .and_then(|value| u8::try_from(value).ok())
                        .ok_or(Error::Usage)?,
                    None => self.status,
                };
                self.exit = Some(status);
                return Ok(status);
            }
        }
        Ok(0)
    }
}

fn directory(io: &mut dyn System, path: &[u8]) -> vibrix_syscall::Result<bool> {
    let mut entry = abi::DirEntry::EMPTY;
    io.read_dir(path, 0, &mut entry)
}

fn require_empty(args: &[&[u8]]) -> Result<()> {
    if args.is_empty() {
        Ok(())
    } else {
        Err(Error::Usage)
    }
}

fn operands<'a, 'b>(args: &'a [&'b [u8]]) -> Result<&'a [&'b [u8]]> {
    if let Some(args) = args.strip_prefix(&[&b"--"[..]]) {
        return Ok(args);
    }
    if args
        .first()
        .is_some_and(|arg| arg.len() > 1 && arg.starts_with(b"-"))
    {
        return Err(Error::Usage);
    }
    Ok(args)
}

fn open_output(io: &mut dyn System, path: &[u8]) -> Result<u64> {
    let flags = abi::OPEN_WRITE | abi::OPEN_TRUNCATE;
    match io.open(path, flags) {
        Ok(fd) => Ok(fd),
        Err(code) if code == abi::Errno::NotFound.code() => {
            io.create(path)?;
            Ok(io.open(path, flags)?)
        }
        Err(code) => Err(code.into()),
    }
}

fn finish<T>(io: &mut dyn System, fd: u64, result: Result<T>) -> Result<T> {
    let closed = io.close(fd);
    let value = result?;
    closed?;
    Ok(value)
}

fn echo(io: &mut dyn System, fd: u64, args: &[&[u8]], newline: bool) -> Result<()> {
    for (index, &arg) in args.iter().enumerate() {
        if index != 0 {
            write_all(io, fd, b" ")?;
        }
        write_all(io, fd, arg)?;
    }
    if newline {
        write_all(io, fd, b"\n")?;
    }
    Ok(())
}

fn stream(io: &mut dyn System, input: u64, output: u64) -> Result<()> {
    let mut bytes = [0; 128];
    loop {
        let count = system::read(io, input, &mut bytes)?;
        if count == 0 {
            return Ok(());
        }
        write_all(io, output, &bytes[..count])?;
    }
}

fn load_text(io: &mut dyn System, fd: u64, bytes: &mut [u8; text::TEXT_BYTES]) -> Result<usize> {
    let mut len = 0;
    while len < bytes.len() {
        let count = system::read(io, fd, &mut bytes[len..])?;
        if count == 0 {
            return Ok(len);
        }
        len += count;
    }
    if system::read(io, fd, &mut [0; 1])? != 0 {
        return Err(Error::Message(b"text input exceeds 1024 bytes"));
    }
    Ok(len)
}

fn entry_name(entry: &abi::DirEntry) -> Result<&[u8]> {
    let len = usize::from(entry.name_len);
    if len == 0 || len > entry.name.len() {
        return Err(Error::Message(b"invalid directory entry"));
    }
    Ok(&entry.name[..len])
}

fn require_regular(io: &mut dyn System, path: &[u8]) -> Result<()> {
    let parent = text::dirname(path);
    let name = text::basename(path);
    for index in 0..1024 {
        let mut entry = abi::DirEntry::EMPTY;
        if !io.read_dir(parent, index, &mut entry)? {
            break;
        }
        if entry_name(&entry)? == name {
            return if entry.kind == abi::ENTRY_FILE {
                Ok(())
            } else {
                Err(Error::Message(b"operation requires a regular file"))
            };
        }
    }
    Err(Error::Os(abi::Errno::NotFound.code()))
}

fn show_help(io: &mut dyn System, fd: u64, name: Option<&[u8]>) -> Result<()> {
    if let Some(name) = name {
        let page = manual::lookup(name).ok_or(Error::Message(b"no manual entry; try apropos"))?;
        write_all(io, fd, page.summary)?;
        write_all(io, fd, b"\nUsage: ")?;
        write_all(io, fd, page.usage)?;
        write_all(io, fd, b"\nUse man ")?;
        write_all(io, fd, page.name)?;
        write_all(io, fd, b" for details.\n")?;
    } else {
        write_all(
            io,
            fd,
            b"Vibrix commands - help COMMAND, man COMMAND, man shell\n",
        )?;
        for (index, page) in MANUALS.iter().enumerate() {
            write_all(io, fd, page.name)?;
            if index % 5 == 4 || index + 1 == MANUALS.len() {
                write_all(io, fd, b"\n")?;
            } else {
                for _ in page.name.len()..12 {
                    write_all(io, fd, b" ")?;
                }
            }
        }
        write_all(io, fd, b"vfetch (aliases: neofetch fastfetch); which alias: type\nFiles live in RAM and are lost when the VM stops.\n")?;
    }
    Ok(())
}

fn show_manual(io: &mut dyn System, fd: u64, name: &[u8]) -> Result<()> {
    if name == b"shell" {
        if fd == 1 {
            write_all(io, fd, b"\x0c")?;
        }
        write_all(io, fd, manual::SHELL_MANUAL)?;
        return Ok(());
    }
    let page = manual::lookup(name).ok_or(Error::Message(b"no manual entry; try apropos"))?;
    if fd == 1 {
        write_all(io, fd, b"\x0c")?;
    }
    for part in [
        page.name,
        b"(1) - Vibrix userspace\n\nNAME\n  ",
        page.name,
        b" - ",
        page.summary,
        b"\n\nSYNOPSIS\n  ",
        page.usage,
        b"\n\nDESCRIPTION\n  ",
    ] {
        write_all(io, fd, part)?;
    }
    let mut column = 2;
    for word in page
        .description
        .split(|byte| byte.is_ascii_whitespace())
        .filter(|word| !word.is_empty())
    {
        if column > 2 {
            if column + 1 + word.len() > 74 {
                write_all(io, fd, b"\n  ")?;
                column = 2;
            } else {
                write_all(io, fd, b" ")?;
                column += 1;
            }
        }
        write_all(io, fd, word)?;
        column += word.len();
    }
    for part in [&b"\n\nEXAMPLES\n  "[..], page.example, b"\n\nEXIT STATUS\n  0 success; 1 failure/no match; 2 usage; see description.\n\nLIMITS\n  Built-in, no external binary. RAM files are not persistent.\n  Text filters: 1024 bytes; sort/uniq: 128 lines. See man shell.\n"] { write_all(io, fd, part)?; }
    Ok(())
}

fn search_manuals(io: &mut dyn System, fd: u64, word: &[u8]) -> Result<u8> {
    let mut found = false;
    for page in MANUALS {
        if text::contains(page.name, word, true) || text::contains(page.summary, word, true) {
            found = true;
            write_all(io, fd, page.name)?;
            write_all(io, fd, b" - ")?;
            write_all(io, fd, page.summary)?;
            write_all(io, fd, b"\n")?;
        }
    }
    Ok(u8::from(!found))
}

#[derive(Clone, Copy)]
struct DoctorSnapshot {
    process_count: u64,
}

fn collect_vibrix_doctor(io: &mut dyn System) -> Result<DoctorSnapshot> {
    let pid = io.getpid()?;
    let mut current_seen = false;
    let mut process_count = 0u64;
    for index in 0..1024 {
        let mut info = abi::ProcessInfo::EMPTY;
        match io.process_info(index, &mut info) {
            Ok(false) => break,
            Err(code) if code == abi::Errno::NotFound.code() => break,
            Err(code) => return Err(code.into()),
            Ok(true) => {
                process_count += 1;
                if u64::from(info.pid) == pid {
                    current_seen = true;
                }
            }
        }
    }
    if !current_seen {
        return Err(Error::Message(
            b"doctor: current PID absent from process table",
        ));
    }

    if !directory(io, b"/")? {
        return Err(Error::Message(b"doctor: root directory unavailable"));
    }
    if !directory(io, b"/dev")? {
        return Err(Error::Message(b"doctor: /dev unavailable"));
    }

    let welcome = io.open(b"/welcome", abi::OPEN_READ)?;
    let mut welcome_byte = [0u8; 1];
    let welcome_read = system::read(io, welcome, &mut welcome_byte);
    let welcome_close = io.close(welcome);
    if welcome_read? == 0 {
        return Err(Error::Message(b"doctor: /welcome is unexpectedly empty"));
    }
    welcome_close?;

    let zero = io.open(b"/dev/zero", abi::OPEN_READ)?;
    let mut zero_byte = [0xa5u8; 1];
    let zero_read = system::read(io, zero, &mut zero_byte);
    let zero_close = io.close(zero);
    if zero_read? != 1 || zero_byte != [0] {
        return Err(Error::Message(b"doctor: /dev/zero contract failed"));
    }
    zero_close?;

    let null = io.open(b"/dev/null", abi::OPEN_READ)?;
    let mut null_byte = [0xa5u8; 1];
    let null_read = system::read(io, null, &mut null_byte);
    let null_close = io.close(null);
    if null_read? != 0 {
        return Err(Error::Message(b"doctor: /dev/null read contract failed"));
    }
    null_close?;

    Ok(DoctorSnapshot { process_count })
}

fn render_vibrix_doctor(io: &mut dyn System, fd: u64) -> Result<()> {
    let snapshot = collect_vibrix_doctor(io)?;
    write_all(io, fd, b"Vibrix doctor\n")?;
    write_all(io, fd, b"  process table: PASS (records=")?;
    number(io, fd, snapshot.process_count)?;
    write_all(io, fd, b")\n")?;
    write_all(io, fd, b"  root mount: PASS\n")?;
    write_all(io, fd, b"  /dev mount: PASS\n")?;
    write_all(io, fd, b"  /welcome read: PASS\n")?;
    write_all(io, fd, b"  /dev/zero: PASS\n")?;
    write_all(io, fd, b"  /dev/null: PASS\n")?;
    write_all(
        io,
        fd,
        b"doctor: PASS (bootstrap checks only; persistent USB, network link, updates and hardware health not tested)\n",
    )?;
    Ok(())
}

fn render_vibrix_doctor_bundle(io: &mut dyn System, fd: u64) -> Result<()> {
    let snapshot = collect_vibrix_doctor(io)?;
    write_all(io, fd, b"VIBRIX-SUPPORT-BUNDLE v1\n")?;
    write_all(io, fd, b"privacy=bounded-anonymous\n")?;
    write_all(io, fd, b"architecture=x86_64\n")?;
    write_all(io, fd, b"shell_version=")?;
    write_all(io, fd, env!("CARGO_PKG_VERSION").as_bytes())?;
    write_all(io, fd, b"\nprocess_records=")?;
    number(io, fd, snapshot.process_count)?;
    write_all(
        io,
        fd,
        b"\nroot_mount=pass\ndev_mount=pass\nwelcome_read=pass\ndev_zero=pass\ndev_null=pass\n",
    )?;
    write_all(
        io,
        fd,
        b"persistence=unavailable\nnetwork_link=unavailable\nupdates=unavailable\nhardware_health=unavailable\n",
    )?;
    write_all(
        io,
        fd,
        b"privacy_note=no-file-contents,no-pid-list,no-memory-addresses,no-hardware-identifiers,no-environment,no-history\nEND-VIBRIX-SUPPORT-BUNDLE\n",
    )?;
    Ok(())
}

fn render_vibrix_compatibility_report(io: &mut dyn System, fd: u64) -> Result<()> {
    // This report is local-only and intentionally contains no stable device,
    // account, network, process or filesystem identifiers. The explicit
    // --anonymized token is required by dispatch before this function runs.
    write_all(io, fd, b"VIBRIX-COMPATIBILITY-REPORT v1\n")?;
    write_all(io, fd, b"consent=explicit-anonymized\n")?;
    write_all(io, fd, b"upload=none\n")?;
    write_all(io, fd, b"architecture=x86_64\n")?;
    write_all(io, fd, b"boot_environment=qemu-or-hardware-unclassified\n")?;
    write_all(io, fd, b"usb_hid=implemented-bounded\n")?;
    write_all(io, fd, b"usb_storage=not-claimed\n")?;
    write_all(io, fd, b"network_driver=not-claimed\n")?;
    write_all(io, fd, b"graphics=framebuffer-userspace-api\n")?;
    write_all(io, fd, b"known_limit=xhci-32-byte-contexts-only\n")?;
    write_all(
        io,
        fd,
        b"privacy_note=no-device-ids,no-serials,no-network-addresses,no-pids,no-user-paths,no-file-contents,no-history\n",
    )?;
    write_all(io, fd, b"END-VIBRIX-COMPATIBILITY-REPORT\n")?;
    Ok(())
}

fn sysctl_process_count(io: &mut dyn System) -> Result<u64> {
    let mut total = 0u64;
    for index in 0..1024 {
        let mut info = abi::ProcessInfo::EMPTY;
        match io.process_info(index, &mut info) {
            Ok(true) => total += 1,
            Ok(false) => return Ok(total),
            Err(code) if code == abi::Errno::NotFound.code() => return Ok(total),
            Err(code) => return Err(code.into()),
        }
    }
    Err(Error::Message(b"sysctl: process enumeration limit reached"))
}

fn render_sysctl_value(io: &mut dyn System, fd: u64, name: &[u8]) -> Result<()> {
    write_all(io, fd, name)?;
    write_all(io, fd, b" = ")?;
    match name {
        b"kern.ostype" => write_all(io, fd, b"Vibrix")?,
        b"kern.osrelease" => write_all(io, fd, env!("CARGO_PKG_VERSION").as_bytes())?,
        b"hw.machine" => write_all(io, fd, b"x86_64")?,
        b"kern.pid" => number(io, fd, io.getpid()?)?,
        b"kern.processes" => number(io, fd, sysctl_process_count(io)?)?,
        b"vfs.root" => write_all(
            io,
            fd,
            if directory(io, b"/")? {
                b"mounted-volatile"
            } else {
                b"unavailable"
            },
        )?,
        b"vfs.dev" => write_all(
            io,
            fd,
            if directory(io, b"/dev")? {
                b"mounted"
            } else {
                b"unavailable"
            },
        )?,
        _ => return Err(Error::Message(b"sysctl: unknown name")),
    }
    write_all(io, fd, b"\n")?;
    Ok(())
}

fn render_sysctl(io: &mut dyn System, fd: u64, args: &[&[u8]]) -> Result<()> {
    const NAMES: [&[u8]; 7] = [
        b"kern.ostype",
        b"kern.osrelease",
        b"hw.machine",
        b"kern.pid",
        b"kern.processes",
        b"vfs.root",
        b"vfs.dev",
    ];
    match args {
        [b"-a"] => {
            for name in NAMES {
                render_sysctl_value(io, fd, name)?;
            }
            Ok(())
        }
        [name] if !name.contains(&b'=') => render_sysctl_value(io, fd, name),
        _ => Err(Error::Usage),
    }
}

fn render_vibrix_status(io: &mut dyn System, fd: u64) -> Result<()> {
    let pid = io.getpid()?;
    let mut total = 0u64;
    let mut running = 0u64;
    let mut zombie = 0u64;
    for index in 0..1024 {
        let mut info = abi::ProcessInfo::EMPTY;
        match io.process_info(index, &mut info) {
            Ok(false) => break,
            Err(code) if code == abi::Errno::NotFound.code() => break,
            Err(code) => return Err(code.into()),
            Ok(true) => {
                total += 1;
                if info.state == abi::PROCESS_RUNNING {
                    running += 1;
                } else if info.state == abi::PROCESS_ZOMBIE {
                    zombie += 1;
                }
            }
        }
    }

    let root_ready = directory(io, b"/")?;
    let dev_ready = directory(io, b"/dev")?;

    write_all(io, fd, b"Vibrix status\n")?;
    write_all(io, fd, b"  userspace: native ring3 shell\n")?;
    write_all(io, fd, b"  pid: ")?;
    number(io, fd, pid)?;
    write_all(io, fd, b"\n  processes: total=")?;
    number(io, fd, total)?;
    write_all(io, fd, b" running=")?;
    number(io, fd, running)?;
    write_all(io, fd, b" zombie=")?;
    number(io, fd, zombie)?;
    write_all(io, fd, b"\n  root: ")?;
    write_all(
        io,
        fd,
        if root_ready {
            b"bootstrap RAM mounted (volatile)\n"
        } else {
            b"unavailable\n"
        },
    )?;
    write_all(io, fd, b"  dev: ")?;
    write_all(
        io,
        fd,
        if dev_ready {
            b"/dev mounted\n"
        } else {
            b"unavailable\n"
        },
    )?;
    write_all(
        io,
        fd,
        b"  persistence: not available in bootstrap root\n  network link: not exposed to userspace status\n",
    )?;
    Ok(())
}

pub fn render_fetch(io: &mut dyn System, fd: u64) -> Result<()> {
    let pid = io.getpid().ok();
    let mut result = Ok(());
    fetch::render(
        &fetch::Cpu::discover(),
        pid,
        fetch::privilege_level(),
        env!("CARGO_PKG_VERSION"),
        |part| {
            if result.is_ok() {
                result = write_all(io, fd, part).map_err(Error::from);
            }
        },
    );
    result
}
