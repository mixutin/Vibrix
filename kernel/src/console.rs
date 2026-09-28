//! Bounded single-CPU early COM1 development-console line editor.
//!
//! Pure ASCII state machine. The PS/2 driver owns port reads; this module
//! never accesses firmware, IO ports, heap, static mutable memory or storage.

pub const CAPACITY: usize = 80;

#[derive(Debug, PartialEq, Eq)]
pub enum Edit<'a> {
    Echo(u8),
    Erase,
    Complete(&'a [u8]),
    Full,
    Ignore,
}

pub struct LineEditor {
    bytes: [u8; CAPACITY],
    used: usize,
}

impl LineEditor {
    pub const fn new() -> Self {
        Self {
            bytes: [0; CAPACITY],
            used: 0,
        }
    }

    pub fn feed(&mut self, byte: u8) -> Edit<'_> {
        match byte {
            b'\r' | b'\n' => Edit::Complete(&self.bytes[..self.used]),
            8 | 127 if self.used != 0 => {
                self.used -= 1;
                self.bytes[self.used] = 0;
                Edit::Erase
            }
            8 | 127 => Edit::Ignore,
            b' '..=b'~' if self.used < CAPACITY => {
                self.bytes[self.used] = byte;
                self.used += 1;
                Edit::Echo(byte)
            }
            b' '..=b'~' => Edit::Full,
            _ => Edit::Ignore,
        }
    }

    /// Call after consuming Complete: the returned slice must not escape.
    pub fn reset(&mut self) {
        self.bytes[..self.used].fill(0);
        self.used = 0;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Empty,
    Help,
    Info,
    Uptime,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandSpec {
    pub name: &'static [u8],
    pub command: Command,
}

pub const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: b"help",
        command: Command::Help,
    },
    CommandSpec {
        name: b"info",
        command: Command::Info,
    },
    CommandSpec {
        name: b"uptime",
        command: Command::Uptime,
    },
];

/// Match whole words only against the fixed dispatch registry, allowing
/// harmless leading/trailing ASCII spaces. No allocator, UTF-8 guesswork,
/// shell expansions or argument parsing.
pub fn command(bytes: &[u8]) -> Command {
    let trimmed = bytes.trim_ascii();
    if trimmed.is_empty() {
        return Command::Empty;
    }
    COMMANDS
        .iter()
        .find(|spec| spec.name == trimmed)
        .map_or(Command::Unknown, |spec| spec.command)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_before_completion_and_clears_owned_buffer() {
        let mut line = LineEditor::new();
        assert_eq!(line.feed(b'h'), Edit::Echo(b'h'));
        assert_eq!(line.feed(b'e'), Edit::Echo(b'e'));
        assert_eq!(line.feed(b'l'), Edit::Echo(b'l'));
        assert_eq!(line.feed(b'x'), Edit::Echo(b'x'));
        assert_eq!(line.feed(8), Edit::Erase);
        assert_eq!(line.feed(b'p'), Edit::Echo(b'p'));
        assert_eq!(line.feed(b'\n'), Edit::Complete(b"help"));
        line.reset();
        assert_eq!(line.feed(b'\n'), Edit::Complete(b""));
    }

    #[test]
    fn bounded_full_line_and_recovery_after_erase() {
        let mut line = LineEditor::new();
        for _ in 0..CAPACITY {
            assert_eq!(line.feed(b'a'), Edit::Echo(b'a'));
        }
        assert_eq!(line.feed(b'b'), Edit::Full);
        match line.feed(b'\n') {
            Edit::Complete(bytes) => {
                assert_eq!(bytes.len(), CAPACITY);
                assert!(bytes.iter().all(|&b| b == b'a'));
            }
            _ => panic!("full line must still terminate"),
        }
        assert_eq!(line.feed(127), Edit::Erase);
        assert_eq!(line.feed(b'b'), Edit::Echo(b'b'));
    }

    #[test]
    fn unsupported_control_bytes_do_not_mutate_line() {
        let mut line = LineEditor::new();
        assert_eq!(line.feed(8), Edit::Ignore);
        assert_eq!(line.feed(0), Edit::Ignore);
        assert_eq!(line.feed(0x80), Edit::Ignore);
        assert_eq!(line.feed(b'\r'), Edit::Complete(b""));
        assert_eq!(line.feed(b'i'), Edit::Echo(b'i'));
        assert_eq!(line.feed(b'\n'), Edit::Complete(b"i"));
    }

    #[test]
    fn dispatch_registry_has_unique_exact_names() {
        for (index, command) in COMMANDS.iter().enumerate() {
            assert!(!command.name.is_empty());
            assert!(
                command
                    .name
                    .iter()
                    .all(|byte| byte.is_ascii_graphic() && *byte != b' ')
            );
            assert!(
                !COMMANDS[..index]
                    .iter()
                    .any(|earlier| earlier.name == command.name)
            );
            assert_eq!(super::command(command.name), command.command);
        }
    }

    #[test]
    fn command_matching_requires_exact_ascii_words() {
        assert_eq!(command(b""), Command::Empty);
        assert_eq!(command(b"  "), Command::Empty);
        assert_eq!(command(b"help"), Command::Help);
        assert_eq!(command(b" help  "), Command::Help);
        assert_eq!(command(b"info"), Command::Info);
        assert_eq!(command(b"uptime"), Command::Uptime);
        for unknown in [
            b"HELP".as_slice(),
            b"hel".as_slice(),
            b"help me".as_slice(),
            b"uptime now".as_slice(),
        ] {
            assert_eq!(command(unknown), Command::Unknown);
        }
    }
}
