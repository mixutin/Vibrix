//! Fixed bootstrap device namespace.
//!
//! /dev/null and /dev/zero are stateless. /dev/tty owns one bounded canonical
//! line discipline and one bounded output queue. Hardware drivers inject input
//! through the internal Filesystem device hook; userspace-facing reads/writes
//! remain ordinary VFS operations.

use super::{Entry, Error, Filesystem, Kind, Metadata, Name, NodeId, Result};

const TTY_INPUT_BYTES: usize = 256;
const TTY_OUTPUT_BYTES: usize = 512;

pub struct DevFs {
    input: [u8; TTY_INPUT_BYTES],
    input_len: usize,
    committed: usize,
    output: [u8; TTY_OUTPUT_BYTES],
    output_len: usize,
}

impl Default for DevFs {
    fn default() -> Self {
        Self::new()
    }
}

impl DevFs {
    pub const fn new() -> Self {
        Self {
            input: [0; TTY_INPUT_BYTES],
            input_len: 0,
            committed: 0,
            output: [0; TTY_OUTPUT_BYTES],
            output_len: 0,
        }
    }

    fn tty_input(&mut self, byte: u8) -> Result<()> {
        match byte {
            b'\r' | b'\n' => {
                if self.input_len == self.input.len() {
                    return Err(Error::NoSpace);
                }
                self.input[self.input_len] = b'\n';
                self.input_len += 1;
                self.committed = self.input_len;
            }
            0x08 | 0x7f => {
                if self.input_len > self.committed {
                    self.input_len -= 1;
                    self.input[self.input_len] = 0;
                }
            }
            0x15 => {
                self.input[self.committed..self.input_len].fill(0);
                self.input_len = self.committed;
            }
            b'\t' | 0x20..=0x7e => {
                if self.input_len == self.input.len() {
                    return Err(Error::NoSpace);
                }
                self.input[self.input_len] = byte;
                self.input_len += 1;
            }
            _ => {}
        }
        Ok(())
    }

    fn tty_read(&mut self, buffer: &mut [u8]) -> Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        if self.committed == 0 {
            return Err(Error::WouldBlock);
        }
        let count = buffer.len().min(self.committed);
        buffer[..count].copy_from_slice(&self.input[..count]);
        self.input.copy_within(count..self.input_len, 0);
        self.input_len -= count;
        self.committed -= count;
        self.input[self.input_len..].fill(0);
        Ok(count)
    }

    fn tty_write(&mut self, buffer: &[u8]) -> Result<usize> {
        let end = self
            .output_len
            .checked_add(buffer.len())
            .ok_or(Error::NoSpace)?;
        if end > self.output.len() {
            return Err(Error::WouldBlock);
        }
        self.output[self.output_len..end].copy_from_slice(buffer);
        self.output_len = end;
        Ok(buffer.len())
    }

    fn tty_output(&mut self, buffer: &mut [u8]) -> Result<usize> {
        if buffer.is_empty() || self.output_len == 0 {
            return Ok(0);
        }
        let count = buffer.len().min(self.output_len);
        buffer[..count].copy_from_slice(&self.output[..count]);
        self.output.copy_within(count..self.output_len, 0);
        self.output_len -= count;
        self.output[self.output_len..].fill(0);
        Ok(count)
    }
}

impl Filesystem for DevFs {
    fn root(&self) -> NodeId {
        NodeId(0)
    }

    fn metadata(&self, id: NodeId) -> Result<Metadata> {
        let kind = match id.0 {
            0 => Kind::Directory,
            1..=3 => Kind::Device,
            _ => return Err(Error::StaleNode),
        };
        Ok(Metadata { kind, len: 0 })
    }

    fn lookup(&self, dir: NodeId, name: &str) -> Result<NodeId> {
        if self.metadata(dir)?.kind != Kind::Directory {
            return Err(Error::NotDirectory);
        }
        Name::new(name)?;
        match name {
            "null" => Ok(NodeId(1)),
            "zero" => Ok(NodeId(2)),
            "tty" => Ok(NodeId(3)),
            _ => Err(Error::NotFound),
        }
    }

    fn entry(&self, dir: NodeId, index: usize) -> Result<Option<Entry>> {
        if self.metadata(dir)?.kind != Kind::Directory {
            return Err(Error::NotDirectory);
        }
        let (name, id) = match index {
            0 => ("null", NodeId(1)),
            1 => ("zero", NodeId(2)),
            2 => ("tty", NodeId(3)),
            _ => return Ok(None),
        };
        Ok(Some(Entry {
            name: Name::new(name)?,
            id,
            kind: Kind::Device,
        }))
    }

    fn create(&mut self, _dir: NodeId, _name: &str, _kind: Kind) -> Result<NodeId> {
        Err(Error::ReadOnly)
    }

    fn remove(&mut self, _dir: NodeId, _name: &str) -> Result<()> {
        Err(Error::ReadOnly)
    }

    fn read(&mut self, id: NodeId, _offset: usize, buffer: &mut [u8]) -> Result<usize> {
        match id.0 {
            0 => Err(Error::IsDirectory),
            1 => Ok(0),
            2 => {
                buffer.fill(0);
                Ok(buffer.len())
            }
            3 => self.tty_read(buffer),
            _ => Err(Error::StaleNode),
        }
    }

    fn write(&mut self, id: NodeId, _offset: usize, buffer: &[u8]) -> Result<usize> {
        match id.0 {
            0 => Err(Error::IsDirectory),
            1 | 2 => Ok(buffer.len()),
            3 => self.tty_write(buffer),
            _ => Err(Error::StaleNode),
        }
    }

    fn truncate(&mut self, _id: NodeId) -> Result<()> {
        Err(Error::NotSeekable)
    }

    fn device_input(&mut self, id: NodeId, byte: u8) -> Result<()> {
        match id.0 {
            3 => self.tty_input(byte),
            0..=2 => Err(Error::Unsupported),
            _ => Err(Error::StaleNode),
        }
    }

    fn device_output(&mut self, id: NodeId, buffer: &mut [u8]) -> Result<usize> {
        match id.0 {
            3 => self.tty_output(buffer),
            0..=2 => Err(Error::Unsupported),
            _ => Err(Error::StaleNode),
        }
    }
}
