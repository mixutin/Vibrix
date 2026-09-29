//! Terminal output decoration over the existing real shell System backend.
use crate::Terminal;
use vibrix_shell::system::System;
use vibrix_syscall::{Result, abi};
pub struct TerminalIo<'a> {
    pub terminal: &'a mut Terminal,
    pub backend: &'a mut dyn System,
}
impl System for TerminalIo<'_> {
    fn read(&mut self, fd: u64, bytes: &mut [u8]) -> Result<usize> {
        // This foreground editor owns input. Stream consumers need redirection;
        // an unredirected filter reports unsupported, never steals UI events.
        if fd == 0 {
            Err(abi::Errno::NotSupported.code())
        } else {
            self.backend.read(fd, bytes)
        }
    }
    fn write(&mut self, fd: u64, bytes: &[u8]) -> Result<usize> {
        let count = self.backend.write(fd, bytes)?;
        if count > bytes.len() {
            return Err(abi::Errno::Io.code());
        }
        if fd == 1 || fd == 2 {
            self.terminal.write(&bytes[..count]);
        }
        Ok(count)
    }
    fn open(&mut self, path: &[u8], flags: u64) -> Result<u64> {
        self.backend.open(path, flags)
    }
    fn close(&mut self, fd: u64) -> Result<()> {
        self.backend.close(fd)
    }
    fn create(&mut self, path: &[u8]) -> Result<()> {
        self.backend.create(path)
    }
    fn mkdir(&mut self, path: &[u8]) -> Result<()> {
        self.backend.mkdir(path)
    }
    fn remove(&mut self, path: &[u8]) -> Result<()> {
        self.backend.remove(path)
    }
    fn read_dir(&mut self, path: &[u8], index: u64, entry: &mut abi::DirEntry) -> Result<bool> {
        self.backend.read_dir(path, index, entry)
    }
    fn process_info(&mut self, index: u64, info: &mut abi::ProcessInfo) -> Result<bool> {
        self.backend.process_info(index, info)
    }
    fn getpid(&mut self) -> Result<u64> {
        self.backend.getpid()
    }
    fn kill(&mut self, pid: u64, status: i32) -> Result<()> {
        self.backend.kill(pid, status)
    }
}
