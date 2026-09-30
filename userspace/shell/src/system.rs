//! Safe I/O boundary shared by the native shell and host regression tests.

use vibrix_syscall::{self as syscall, Result, abi};

pub trait System {
    fn read(&mut self, fd: u64, bytes: &mut [u8]) -> Result<usize>;
    fn write(&mut self, fd: u64, bytes: &[u8]) -> Result<usize>;
    fn open(&mut self, path: &[u8], flags: u64) -> Result<u64>;
    fn close(&mut self, fd: u64) -> Result<()>;
    fn create(&mut self, path: &[u8]) -> Result<()>;
    fn mkdir(&mut self, path: &[u8]) -> Result<()>;
    fn remove(&mut self, path: &[u8]) -> Result<()>;
    fn rename(&mut self, from: &[u8], to: &[u8]) -> Result<()>;
    fn read_dir(&mut self, path: &[u8], index: u64, entry: &mut abi::DirEntry) -> Result<bool>;
    fn process_info(&mut self, index: u64, info: &mut abi::ProcessInfo) -> Result<bool>;
    fn getpid(&mut self) -> Result<u64>;
    fn kill(&mut self, pid: u64, status: i32) -> Result<()>;
}

pub struct Native;

impl System for Native {
    fn read(&mut self, fd: u64, bytes: &mut [u8]) -> Result<usize> {
        syscall::read(fd, bytes)
    }

    fn write(&mut self, fd: u64, bytes: &[u8]) -> Result<usize> {
        syscall::write(fd, bytes)
    }

    fn open(&mut self, path: &[u8], flags: u64) -> Result<u64> {
        syscall::open(path, flags)
    }

    fn close(&mut self, fd: u64) -> Result<()> {
        syscall::close(fd)
    }

    fn create(&mut self, path: &[u8]) -> Result<()> {
        syscall::create(path)
    }

    fn mkdir(&mut self, path: &[u8]) -> Result<()> {
        syscall::mkdir(path)
    }

    fn remove(&mut self, path: &[u8]) -> Result<()> {
        syscall::remove(path)
    }

    fn rename(&mut self, from: &[u8], to: &[u8]) -> Result<()> {
        syscall::rename(from, to)
    }

    fn read_dir(&mut self, path: &[u8], index: u64, entry: &mut abi::DirEntry) -> Result<bool> {
        syscall::read_dir(path, index, entry)
    }

    fn process_info(&mut self, index: u64, info: &mut abi::ProcessInfo) -> Result<bool> {
        syscall::process_info(index, info)
    }

    fn getpid(&mut self) -> Result<u64> {
        syscall::getpid()
    }

    fn kill(&mut self, pid: u64, status: i32) -> Result<()> {
        syscall::kill(pid, status)
    }
}

pub fn write_all(io: &mut dyn System, fd: u64, mut bytes: &[u8]) -> Result<()> {
    while !bytes.is_empty() {
        let count = io.write(fd, bytes)?;
        if count == 0 || count > bytes.len() {
            return Err(abi::Errno::Io.code());
        }
        bytes = &bytes[count..];
    }
    Ok(())
}

pub fn read(io: &mut dyn System, fd: u64, bytes: &mut [u8]) -> Result<usize> {
    let count = io.read(fd, bytes)?;
    if count > bytes.len() {
        return Err(abi::Errno::Io.code());
    }
    Ok(count)
}

pub fn number(io: &mut dyn System, fd: u64, value: u64) -> Result<()> {
    let mut result = Ok(());
    crate::fetch::decimal(value, &mut |bytes| {
        if result.is_ok() {
            result = write_all(io, fd, bytes);
        }
    });
    result
}

pub fn parse_number(bytes: &[u8]) -> Option<u64> {
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
