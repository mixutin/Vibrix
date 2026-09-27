//! Bounded early-kernel input line and command classifier.

pub const LINE_CAPACITY: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Event<'a> { None, Echo(u8), Backspace, Command(&'a str), Full }

pub struct LineBuffer { bytes: [u8; LINE_CAPACITY], len: usize }

impl LineBuffer {
    pub const fn new() -> Self { Self { bytes: [0; LINE_CAPACITY], len: 0 } }
    pub fn feed(&mut self, byte: u8) -> Event<'_> {
        match byte {
            b'\n' | b'\r' => Event::Command(core::str::from_utf8(&self.bytes[..self.len]).unwrap_or("")),
            8 | 0x7f => if self.len == 0 { Event::None } else { self.len -= 1; Event::Backspace },
            b' '..=b'~' => if self.len == LINE_CAPACITY { Event::Full } else {
                self.bytes[self.len] = byte; self.len += 1; Event::Echo(byte)
            },
            _ => Event::None,
        }
    }
    pub fn clear(&mut self) { self.len = 0; }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Command { Empty, Help, Info, Unknown }

pub fn classify(line: &str) -> Command {
    match line.trim() { "" => Command::Empty, "help" => Command::Help, "info" => Command::Info, _ => Command::Unknown }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn edits_and_submits() {
        let mut line = LineBuffer::new();
        assert_eq!(line.feed(b'h'), Event::Echo(b'h'));
        assert_eq!(line.feed(b'x'), Event::Echo(b'x'));
        assert_eq!(line.feed(8), Event::Backspace);
        assert_eq!(line.feed(b'i'), Event::Echo(b'i'));
        assert_eq!(line.feed(b'\n'), Event::Command("hi"));
    }
    #[test] fn commands_are_explicit() {
        assert_eq!(classify(" help "), Command::Help);
        assert_eq!(classify("info"), Command::Info);
        assert_eq!(classify("pci"), Command::Unknown);
    }
    #[test] fn capacity_is_bounded() {
        let mut line = LineBuffer::new();
        for _ in 0..LINE_CAPACITY { assert!(matches!(line.feed(b'a'), Event::Echo(_))); }
        assert_eq!(line.feed(b'b'), Event::Full);
    }
}
