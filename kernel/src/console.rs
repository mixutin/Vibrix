//! Allocation-free early kernel console core.
//!
//! Input transport is intentionally separate: PS/2 polling today and IRQ
//! delivery later can feed the same bounded line editor.

pub const LINE_CAPACITY: usize = 96;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Help,
    Clear,
    Info,
    Mem,
    Pci,
    Acpi,
    Uptime,
    Reboot,
    Unknown,
}

pub struct LineEditor {
    bytes: [u8; LINE_CAPACITY],
    len: usize,
}

impl LineEditor {
    pub const fn new() -> Self {
        Self { bytes: [0; LINE_CAPACITY], len: 0 }
    }

    pub fn feed(&mut self, byte: u8) -> Option<&str> {
        match byte {
            b'\n' | b'\r' => {
                let line = core::str::from_utf8(&self.bytes[..self.len]).unwrap_or("");
                Some(line)
            }
            8 | 127 => {
                self.len = self.len.saturating_sub(1);
                None
            }
            b' '..=b'~' if self.len < LINE_CAPACITY => {
                self.bytes[self.len] = byte;
                self.len += 1;
                None
            }
            _ => None,
        }
    }

    pub fn reset(&mut self) {
        self.bytes[..self.len].fill(0);
        self.len = 0;
    }

    pub fn len(&self) -> usize { self.len }
}

pub fn parse(line: &str) -> Command {
    match line.trim() {
        "help" => Command::Help,
        "clear" => Command::Clear,
        "info" => Command::Info,
        "mem" => Command::Mem,
        "pci" => Command::Pci,
        "acpi" => Command::Acpi,
        "uptime" => Command::Uptime,
        "reboot" => Command::Reboot,
        _ => Command::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_line_editing_and_backspace() {
        let mut e = LineEditor::new();
        assert_eq!(e.feed(b'h'), None);
        assert_eq!(e.feed(b'x'), None);
        assert_eq!(e.feed(8), None);
        assert_eq!(e.feed(b'i'), None);
        assert_eq!(e.feed(b'\n'), Some("hi"));
        e.reset();
        assert_eq!(e.len(), 0);
    }

    #[test]
    fn full_buffer_drops_extra_input_without_overflow() {
        let mut e = LineEditor::new();
        for _ in 0..LINE_CAPACITY + 20 { let _ = e.feed(b'a'); }
        assert_eq!(e.len(), LINE_CAPACITY);
        assert_eq!(e.feed(b'\n').unwrap().len(), LINE_CAPACITY);
    }

    #[test]
    fn commands_are_exact_after_whitespace_trim() {
        assert_eq!(parse(" help "), Command::Help);
        assert_eq!(parse("pci"), Command::Pci);
        assert_eq!(parse("help now"), Command::Unknown);
        assert_eq!(parse(""), Command::Unknown);
    }
}
