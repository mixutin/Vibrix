//! Fixed-storage tokenization. Parsing finishes before any filesystem mutation.

use crate::manual::Builtin;

pub const LINE_BYTES: usize = 256;
pub const MAX_ARGS: usize = 16;
pub const INPUT_LIMIT: usize = 254;

type Span = (u16, u16);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseError {
    TooManyArguments,
    TooLong,
    Nul,
    UnterminatedQuote,
    TrailingEscape,
    UnsupportedSyntax,
    Redirection,
}

impl ParseError {
    pub fn message(self) -> &'static [u8] {
        match self {
            Self::TooManyArguments => b"too many arguments (maximum 16)",
            Self::TooLong => b"input too long; command not executed (maximum 254 bytes)",
            Self::Nul => b"NUL is not accepted",
            Self::UnterminatedQuote => b"unterminated quote",
            Self::TrailingEscape => b"trailing escape",
            Self::UnsupportedSyntax => b"unsupported shell operator; see man shell",
            Self::Redirection => b"invalid or repeated redirection",
        }
    }
}

#[derive(Clone)]
pub struct Command {
    bytes: [u8; LINE_BYTES],
    spans: [Span; MAX_ARGS],
    pub argc: usize,
    pub builtin: Option<Builtin>,
    input: Option<Span>,
    output: Option<Span>,
}

impl Command {
    pub fn parse(line: &[u8]) -> Result<Self, ParseError> {
        // The current TTY silently discards bytes after 255. Refusing a full
        // canonical line prevents an incomplete prefix from causing a write.
        if line.len() > INPUT_LIMIT {
            return Err(ParseError::TooLong);
        }
        if line.contains(&0) {
            return Err(ParseError::Nul);
        }
        let mut command = Self {
            bytes: [0; LINE_BYTES],
            spans: [(0, 0); MAX_ARGS],
            argc: 0,
            builtin: None,
            input: None,
            output: None,
        };
        let mut cursor = 0;
        let mut used = 0;
        let mut redirect = None;
        while cursor < line.len() {
            while cursor < line.len() && line[cursor].is_ascii_whitespace() {
                cursor += 1;
            }
            if cursor == line.len() || line[cursor] == b'#' {
                break;
            }
            if matches!(line[cursor], b'<' | b'>') {
                let kind = line[cursor];
                if redirect.is_some()
                    || (kind == b'<' && command.input.is_some())
                    || (kind == b'>' && command.output.is_some())
                {
                    return Err(ParseError::Redirection);
                }
                redirect = Some(kind);
                cursor += 1;
                continue;
            }
            let start = used;
            let mut quote = None;
            while cursor < line.len() {
                let byte = line[cursor];
                if quote.is_none() && (byte.is_ascii_whitespace() || matches!(byte, b'<' | b'>')) {
                    break;
                }
                cursor += 1;
                if quote == Some(b'\'') {
                    if byte == b'\'' {
                        quote = None;
                    } else {
                        command.bytes[used] = byte;
                        used += 1;
                    }
                    continue;
                }
                if byte == b'\\' {
                    let next = *line.get(cursor).ok_or(ParseError::TrailingEscape)?;
                    command.bytes[used] = next;
                    used += 1;
                    cursor += 1;
                } else if quote == Some(b'"') {
                    if byte == b'"' {
                        quote = None;
                    } else {
                        command.bytes[used] = byte;
                        used += 1;
                    }
                } else if matches!(byte, b'\'' | b'"') {
                    quote = Some(byte);
                } else if matches!(
                    byte,
                    b'|' | b'&' | b';' | b'`' | b'$' | b'(' | b')' | b'*' | b'?' | b'[' | b']'
                ) {
                    return Err(ParseError::UnsupportedSyntax);
                } else {
                    command.bytes[used] = byte;
                    used += 1;
                }
            }
            if quote.is_some() {
                return Err(ParseError::UnterminatedQuote);
            }
            let span = (start as u16, used as u16);
            if let Some(kind) = redirect.take() {
                if start == used {
                    return Err(ParseError::Redirection);
                }
                if kind == b'<' {
                    command.input = Some(span);
                } else {
                    command.output = Some(span);
                }
            } else {
                if command.argc == MAX_ARGS {
                    return Err(ParseError::TooManyArguments);
                }
                command.spans[command.argc] = span;
                command.argc += 1;
            }
        }
        if redirect.is_some()
            || (command.argc == 0 && (command.input.is_some() || command.output.is_some()))
        {
            return Err(ParseError::Redirection);
        }
        command.builtin = Builtin::parse(command.arg(0));
        Ok(command)
    }

    fn bytes_at(&self, span: Span) -> &[u8] {
        &self.bytes[usize::from(span.0)..usize::from(span.1)]
    }

    pub fn arg(&self, index: usize) -> &[u8] {
        if index >= self.argc {
            return b"";
        }
        self.bytes_at(self.spans[index])
    }

    pub fn argv(&self) -> [&[u8]; MAX_ARGS] {
        core::array::from_fn(|index| self.arg(index))
    }

    pub fn input(&self) -> Option<&[u8]> {
        self.input.map(|span| self.bytes_at(span))
    }

    pub fn output(&self) -> Option<&[u8]> {
        self.output.map(|span| self.bytes_at(span))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_escapes_fragments_and_empty_arguments_are_preserved() {
        let command = Command::parse(br#"echo 'one two' "three four" a\ b x"y"z '' """#).unwrap();
        assert_eq!(command.argc, 7);
        assert_eq!(command.arg(1), b"one two");
        assert_eq!(command.arg(2), b"three four");
        assert_eq!(command.arg(3), b"a b");
        assert_eq!(command.arg(4), b"xyz");
        assert_eq!(command.arg(5), b"");
        assert_eq!(command.arg(6), b"");
    }

    #[test]
    fn redirections_are_separate_from_argv_and_quoted_operators_are_literal() {
        let command = Command::parse(br#"echo '>'<"a b">out # comment"#).unwrap();
        assert_eq!(command.argc, 2);
        assert_eq!(command.arg(1), b">");
        assert_eq!(command.input(), Some(&b"a b"[..]));
        assert_eq!(command.output(), Some(&b"out"[..]));
        assert_eq!(Command::parse(b"echo a#b").unwrap().arg(1), b"a#b");
        assert_eq!(Command::parse(b" # comment").unwrap().argc, 0);
    }

    #[test]
    fn malformed_input_never_produces_a_partial_command() {
        for line in [
            &b"echo 'open"[..],
            &b"echo \"open"[..],
            &b"echo trailing\\"[..],
            &b"echo x >> out"[..],
            &b"echo >"[..],
            &b"echo > ''"[..],
            &b"> out"[..],
            &b"cat < a < b"[..],
            &b"echo a | cat"[..],
            &b"echo a; rm file"[..],
            &b"echo $HOME"[..],
            &b"echo x\0y"[..],
            &b"a a a a a a a a a a a a a a a a a"[..],
        ] {
            assert!(Command::parse(line).is_err(), "{line:?}");
        }
    }

    #[test]
    fn saturated_tty_lines_are_rejected_not_truncated() {
        assert!(Command::parse(&[b'x'; INPUT_LIMIT]).is_ok());
        assert!(matches!(
            Command::parse(&[b'x'; 255]),
            Err(ParseError::TooLong)
        ));
        assert!(matches!(
            Command::parse(&[b'x'; 300]),
            Err(ParseError::TooLong)
        ));
    }
}
