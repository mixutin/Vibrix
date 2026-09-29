extern crate std;

use self::std::{collections::BTreeMap, vec::Vec};
use crate::manual::MANUALS;
use crate::system::System;
use crate::{Shell, system};
use vibrix_syscall::{Result, abi};

struct Handle {
    path: Vec<u8>,
    offset: usize,
    write: bool,
}

struct Memory {
    files: BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    handles: BTreeMap<u64, Handle>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    next: u64,
    mutations: usize,
    broken_write: bool,
    oversized_read: bool,
}

fn canonical(path: &[u8]) -> Vec<u8> {
    let mut parts = Vec::new();
    for part in path.split(|&byte| byte == b'/') {
        match part {
            b"" | b"." => {}
            b".." => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    let mut out = Vec::from(&b"/"[..]);
    for (index, part) in parts.into_iter().enumerate() {
        if index != 0 {
            out.push(b'/');
        }
        out.extend_from_slice(part);
    }
    out
}

impl Memory {
    fn new() -> Self {
        let mut files = BTreeMap::new();
        files.insert(Vec::from(&b"/"[..]), None);
        files.insert(Vec::from(&b"/tmp"[..]), None);
        files.insert(
            Vec::from(&b"/welcome"[..]),
            Some(Vec::from(&b"hello RAM\n"[..])),
        );
        Self {
            files,
            handles: BTreeMap::new(),
            stdout: Vec::new(),
            stderr: Vec::new(),
            next: 3,
            mutations: 0,
            broken_write: false,
            oversized_read: false,
        }
    }

    fn run(&mut self, shell: &mut Shell, line: &[u8]) -> u8 {
        self.stdout.clear();
        self.stderr.clear();
        let status = shell.run(self, line);
        assert!(self.handles.is_empty(), "descriptor leaked after {line:?}");
        status
    }

    fn data(&self, path: &[u8]) -> &[u8] {
        self.files.get(path).unwrap().as_ref().unwrap()
    }

    fn insert(&mut self, path: &[u8], value: Option<Vec<u8>>) -> Result<()> {
        let path = canonical(path);
        if self.files.contains_key(&path) {
            return Err(abi::Errno::Busy.code());
        }
        if self.files.get(crate::text::dirname(&path)) != Some(&None) {
            return Err(abi::Errno::NotFound.code());
        }
        self.files.insert(path, value);
        self.mutations += 1;
        Ok(())
    }
}

impl System for Memory {
    fn open(&mut self, path: &[u8], flags: u64) -> Result<u64> {
        let path = canonical(path);
        let file = self
            .files
            .get_mut(&path)
            .ok_or(abi::Errno::NotFound.code())?;
        let data = file.as_mut().ok_or(abi::Errno::InvalidArgument.code())?;
        if flags & abi::OPEN_TRUNCATE != 0 {
            data.clear();
            self.mutations += 1;
        }
        let fd = self.next;
        self.next += 1;
        self.handles.insert(
            fd,
            Handle {
                path,
                offset: 0,
                write: flags & 3 != 0,
            },
        );
        Ok(fd)
    }

    fn close(&mut self, fd: u64) -> Result<()> {
        self.handles
            .remove(&fd)
            .map(|_| ())
            .ok_or(abi::Errno::BadFileDescriptor.code())
    }

    fn read(&mut self, fd: u64, bytes: &mut [u8]) -> Result<usize> {
        if self.oversized_read {
            return Ok(bytes.len() + 1);
        }
        let handle = self
            .handles
            .get_mut(&fd)
            .ok_or(abi::Errno::BadFileDescriptor.code())?;
        let data = self.files.get(&handle.path).unwrap().as_ref().unwrap();
        let count = bytes
            .len()
            .min(64)
            .min(data.len().saturating_sub(handle.offset));
        bytes[..count].copy_from_slice(&data[handle.offset..handle.offset + count]);
        handle.offset += count;
        Ok(count)
    }

    fn write(&mut self, fd: u64, bytes: &[u8]) -> Result<usize> {
        if fd == 1 || fd == 2 {
            if fd == 1 {
                self.stdout.extend_from_slice(bytes);
            } else {
                self.stderr.extend_from_slice(bytes);
            }
            return Ok(bytes.len());
        }
        if self.broken_write {
            return Ok(0);
        }
        let handle = self
            .handles
            .get_mut(&fd)
            .ok_or(abi::Errno::BadFileDescriptor.code())?;
        if !handle.write {
            return Err(abi::Errno::PermissionDenied.code());
        }
        let data = self.files.get_mut(&handle.path).unwrap().as_mut().unwrap();
        let count = bytes.len().min(17);
        data.resize(data.len().max(handle.offset + count), 0);
        data[handle.offset..handle.offset + count].copy_from_slice(&bytes[..count]);
        handle.offset += count;
        self.mutations += 1;
        Ok(count)
    }

    fn create(&mut self, path: &[u8]) -> Result<()> {
        self.insert(path, Some(Vec::new()))
    }
    fn mkdir(&mut self, path: &[u8]) -> Result<()> {
        self.insert(path, None)
    }

    fn remove(&mut self, path: &[u8]) -> Result<()> {
        let path = canonical(path);
        if path == b"/" {
            return Err(abi::Errno::PermissionDenied.code());
        }
        let mut prefix = path.clone();
        prefix.push(b'/');
        if self.files.keys().any(|key| key.starts_with(&prefix)) {
            return Err(abi::Errno::Busy.code());
        }
        self.files
            .remove(&path)
            .ok_or(abi::Errno::NotFound.code())?;
        self.mutations += 1;
        Ok(())
    }

    fn read_dir(&mut self, path: &[u8], index: u64, entry: &mut abi::DirEntry) -> Result<bool> {
        let path = canonical(path);
        if self.files.get(&path) != Some(&None) {
            return Err(abi::Errno::InvalidArgument.code());
        }
        let mut prefix = path;
        if prefix != b"/" {
            prefix.push(b'/');
        }
        let mut children = self.files.iter().filter_map(|(name, data)| {
            let relative = name.strip_prefix(prefix.as_slice())?;
            if relative.is_empty() || relative.contains(&b'/') {
                return None;
            }
            Some((relative, data))
        });
        let Some((name, data)) = children.nth(index as usize) else {
            return Ok(false);
        };
        if name.len() > entry.name.len() {
            return Err(abi::Errno::InvalidArgument.code());
        }
        *entry = abi::DirEntry::EMPTY;
        entry.name[..name.len()].copy_from_slice(name);
        entry.name_len = name.len() as u8;
        entry.kind = if data.is_none() {
            abi::ENTRY_DIRECTORY
        } else {
            abi::ENTRY_FILE
        };
        Ok(true)
    }

    fn process_info(&mut self, index: u64, info: &mut abi::ProcessInfo) -> Result<bool> {
        if index != 0 {
            return Ok(false);
        }
        *info = abi::ProcessInfo::EMPTY;
        info.pid = 7;
        info.state = abi::PROCESS_RUNNING;
        Ok(true)
    }

    fn getpid(&mut self) -> Result<u64> {
        Ok(7)
    }
    fn kill(&mut self, _pid: u64, _status: i32) -> Result<()> {
        self.mutations += 1;
        Err(abi::Errno::NotFound.code())
    }
}

#[test]
fn every_manual_and_help_path_executes_without_mutation() {
    let mut io = Memory::new();
    let mut shell = Shell::new();
    for page in MANUALS {
        let mut line = Vec::from(page.name);
        line.extend_from_slice(b" --help > /tmp/must-not-exist");
        assert_eq!(io.run(&mut shell, &line), 0, "{:?}", page.name);
        assert!(io.stdout.windows(7).any(|bytes| bytes == b"Usage: "));
        assert_eq!(io.mutations, 0);
        assert!(shell.exit.is_none());
        let mut line = Vec::from(&b"man "[..]);
        line.extend_from_slice(page.name);
        assert_eq!(io.run(&mut shell, &line), 0);
        for heading in [
            &b"NAME\n"[..],
            b"SYNOPSIS\n",
            b"DESCRIPTION\n",
            b"EXAMPLES\n",
            b"EXIT STATUS\n",
            b"LIMITS\n",
        ] {
            assert!(
                io.stdout
                    .windows(heading.len())
                    .any(|bytes| bytes == heading)
            );
        }
    }
}

#[test]
fn quoted_creation_redirection_touch_copy_move_and_removal_work() {
    let mut io = Memory::new();
    let mut shell = Shell::new();
    assert_eq!(
        io.run(&mut shell, b"echo 'hello world' > '/tmp/my note'"),
        0
    );
    assert!(io.stdout.is_empty());
    assert_eq!(io.data(b"/tmp/my note"), b"hello world\n");
    assert_eq!(io.run(&mut shell, b"touch '/tmp/my note'"), 0);
    assert_eq!(io.data(b"/tmp/my note"), b"hello world\n");
    assert_eq!(io.run(&mut shell, b"cat < '/tmp/my note'"), 0);
    assert_eq!(io.stdout, b"hello world\n");
    assert_eq!(io.run(&mut shell, b"cp '/tmp/my note' /tmp/copy"), 0);
    assert_eq!(io.run(&mut shell, b"mv /tmp/copy /tmp/moved"), 0);
    assert!(!io.files.contains_key(&b"/tmp/copy"[..]));
    assert_eq!(io.data(b"/tmp/moved"), b"hello world\n");
    assert_eq!(io.run(&mut shell, b"rm /tmp/moved"), 0);
}

#[test]
fn source_aliases_and_bad_syntax_never_truncate_input() {
    let mut io = Memory::new();
    let mut shell = Shell::new();
    for line in [
        &b"cp /welcome /./welcome"[..],
        b"cat /welcome > /welcome",
        b"cat < /welcome > /./welcome",
        b"echo x >> /welcome",
        b"echo x > /welcome | cat",
        b"write /welcome 'unterminated",
    ] {
        assert_ne!(io.run(&mut shell, line), 0);
        assert_eq!(io.data(b"/welcome"), b"hello RAM\n");
    }
    let mut line = Vec::from(&b"write /welcome "[..]);
    line.resize(255, b'x');
    assert_eq!(io.run(&mut shell, &line), 2);
    assert_eq!(io.data(b"/welcome"), b"hello RAM\n");
}

#[test]
fn directory_changes_and_empty_directory_removal_are_guarded() {
    let mut io = Memory::new();
    let mut shell = Shell::new();
    assert_eq!(io.run(&mut shell, b"mkdir /tmp/work"), 0);
    assert_eq!(io.run(&mut shell, b"cd /tmp/work"), 0);
    assert_eq!(io.run(&mut shell, b"write note content"), 0);
    assert_eq!(io.run(&mut shell, b"cd /missing"), 1);
    assert_eq!(io.run(&mut shell, b"pwd"), 0);
    assert_eq!(io.stdout, b"/tmp/work\n");
    assert_eq!(io.run(&mut shell, b"cd -"), 0);
    assert_eq!(io.run(&mut shell, b"pwd"), 0);
    assert_eq!(io.stdout, b"/\n");
    assert_eq!(io.run(&mut shell, b"rm /tmp/work"), 1);
    assert_eq!(io.run(&mut shell, b"rmdir /tmp/work"), 1);
    assert_eq!(io.run(&mut shell, b"rm /tmp/work/note"), 0);
    assert_eq!(io.run(&mut shell, b"rmdir /tmp/work"), 0);
}

#[test]
fn status_errors_search_and_exit_are_observable() {
    let mut io = Memory::new();
    let mut shell = Shell::new();
    assert_eq!(io.run(&mut shell, b"false"), 1);
    assert!(io.stderr.is_empty());
    assert_eq!(io.run(&mut shell, b" # comment"), 1);
    assert_eq!(io.run(&mut shell, b"status"), 0);
    assert_eq!(io.stdout, b"1\n");
    assert_eq!(io.run(&mut shell, b"notacommand"), 127);
    assert_eq!(io.run(&mut shell, b"head -n nope /welcome"), 2);
    assert_eq!(io.run(&mut shell, b"grep absent /welcome"), 1);
    assert!(io.stderr.is_empty());
    assert_eq!(io.run(&mut shell, b"grep x /missing"), 2);
    assert_eq!(io.run(&mut shell, b"grep -i -n ram /welcome"), 0);
    assert_eq!(io.stdout, b"1:hello RAM\n");
    assert_eq!(io.run(&mut shell, b"man -k file"), 0);
    assert_eq!(io.run(&mut shell, b"which type fastfetch"), 0);
    assert_eq!(io.run(&mut shell, b"exit 256"), 2);
    assert!(shell.exit.is_none());
    assert_eq!(io.run(&mut shell, b"exit 23"), 23);
    assert_eq!(shell.exit, Some(23));
}

#[test]
fn streaming_and_limit_failures_close_all_descriptors_and_keep_move_source() {
    let mut io = Memory::new();
    let mut shell = Shell::new();
    io.files
        .insert(Vec::from(&b"/large"[..]), Some(self::std::vec![b'x'; 1100]));
    assert_eq!(io.run(&mut shell, b"wc /large"), 1);
    assert!(io.stdout.is_empty());
    assert_eq!(io.run(&mut shell, b"cat /large"), 0);
    assert_eq!(io.stdout.len(), 1100);
    io.broken_write = true;
    assert_eq!(io.run(&mut shell, b"mv /welcome /tmp/fail"), 1);
    assert_eq!(io.data(b"/welcome"), b"hello RAM\n");
    io.broken_write = false;
    io.oversized_read = true;
    assert_eq!(io.run(&mut shell, b"cat /welcome"), 1);
    assert!(io.stdout.is_empty());
    assert_eq!(system::parse_number(b"18446744073709551616"), None);
}

#[test]
fn all_text_commands_and_bounded_history_are_wired_to_dispatch() {
    let mut io = Memory::new();
    let mut shell = Shell::new();
    io.files.insert(
        Vec::from(&b"/lines"[..]),
        Some(Vec::from(&b"b\na\na\n"[..])),
    );
    for (command, expected) in [
        (&b"head -n 1 /lines"[..], &b"b\n"[..]),
        (b"tail -n 1 /lines", b"a\n"),
        (b"sort -u /lines", b"a\nb\n"),
        (b"uniq -c /lines", b"1 b\n2 a\n"),
        (b"wc /lines", b"3 3 6\n"),
        (b"nl /lines", b"1\tb\n2\ta\n3\ta\n"),
        (b"basename /tmp/note.txt .txt", b"note\n"),
        (b"dirname /tmp/note.txt", b"/tmp\n"),
        (b"pid", b"7\n"),
    ] {
        assert_eq!(io.run(&mut shell, command), 0);
        assert_eq!(io.stdout, expected);
    }
    assert_eq!(io.run(&mut shell, b"hexdump /lines"), 0);
    assert_eq!(io.stdout, b"00000000: 62 0a 61 0a 61 0a\n");
    assert_eq!(io.run(&mut shell, b"history"), 0);
    assert_eq!(io.stdout.iter().filter(|&&byte| byte == b'\n').count(), 4);
}
