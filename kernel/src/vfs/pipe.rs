use super::{Error, Result};

/// End ownership counts open descriptions, not duplicated descriptor numbers.
pub(super) struct Pipe<const B: usize> {
    bytes: [u8; B],
    head: usize,
    len: usize,
    pub reader: bool,
    pub writer: bool,
}

impl<const B: usize> Pipe<B> {
    pub const fn new() -> Self {
        Self {
            bytes: [0; B],
            head: 0,
            len: 0,
            reader: true,
            writer: true,
        }
    }
    pub fn read(&mut self, output: &mut [u8]) -> Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        if self.len == 0 {
            return if self.writer {
                Err(Error::WouldBlock)
            } else {
                Ok(0)
            };
        }
        let count = output.len().min(self.len);
        for byte in &mut output[..count] {
            *byte = self.bytes[self.head];
            self.bytes[self.head] = 0;
            self.head = (self.head + 1) % B;
        }
        self.len -= count;
        Ok(count)
    }
    pub fn write(&mut self, input: &[u8]) -> Result<usize> {
        if input.is_empty() {
            return Ok(0);
        }
        if !self.reader {
            return Err(Error::BrokenPipe);
        }
        if input.len() > B {
            return Err(Error::NoSpace);
        }
        if input.len() > B - self.len {
            return Err(Error::WouldBlock);
        }
        for (index, byte) in input.iter().copied().enumerate() {
            self.bytes[(self.head + self.len + index) % B] = byte;
        }
        self.len += input.len();
        Ok(input.len())
    }
}
