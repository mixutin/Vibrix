#![no_std]

pub mod path;

pub const LINE_BYTES: usize = 256;
pub const MAX_ARGS: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Builtin {
    Cat,
    Echo,
    Ls,
    Pwd,
    Cd,
    Mkdir,
    Cp,
    Mv,
    Rm,
    Ps,
    Kill,
    Exit,
    Help,
}

impl Builtin {
    pub fn parse(word: &[u8]) -> Option<Self> {
        Some(match word {
            b"cat" => Self::Cat,
            b"echo" => Self::Echo,
            b"ls" => Self::Ls,
            b"pwd" => Self::Pwd,
            b"cd" => Self::Cd,
            b"mkdir" => Self::Mkdir,
            b"cp" => Self::Cp,
            b"mv" => Self::Mv,
            b"rm" => Self::Rm,
            b"ps" => Self::Ps,
            b"kill" => Self::Kill,
            b"exit" => Self::Exit,
            b"help" => Self::Help,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseError {
    TooManyArguments,
}

#[derive(Clone, Copy)]
pub struct Command<'a> {
    pub builtin: Option<Builtin>,
    pub args: [&'a [u8]; MAX_ARGS],
    pub argc: usize,
}

impl<'a> Command<'a> {
    pub fn parse(line: &'a [u8]) -> Result<Self, ParseError> {
        let mut args = [&[][..]; MAX_ARGS];
        let mut argc = 0usize;
        let mut cursor = 0usize;

        while cursor < line.len() {
            while cursor < line.len() && line[cursor].is_ascii_whitespace() {
                cursor += 1;
            }
            if cursor == line.len() {
                break;
            }
            if argc == MAX_ARGS {
                return Err(ParseError::TooManyArguments);
            }
            let start = cursor;
            while cursor < line.len() && !line[cursor].is_ascii_whitespace() {
                cursor += 1;
            }
            args[argc] = &line[start..cursor];
            argc += 1;
        }

        let builtin = (argc != 0).then(|| Builtin::parse(args[0])).flatten();
        Ok(Self {
            builtin,
            args,
            argc,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_roadmap_utility_name() {
        for (name, expected) in [
            (&b"cat"[..], Builtin::Cat),
            (&b"echo"[..], Builtin::Echo),
            (&b"ls"[..], Builtin::Ls),
            (&b"pwd"[..], Builtin::Pwd),
            (&b"cd"[..], Builtin::Cd),
            (&b"mkdir"[..], Builtin::Mkdir),
            (&b"cp"[..], Builtin::Cp),
            (&b"mv"[..], Builtin::Mv),
            (&b"rm"[..], Builtin::Rm),
            (&b"ps"[..], Builtin::Ps),
            (&b"kill"[..], Builtin::Kill),
        ] {
            assert_eq!(Builtin::parse(name), Some(expected));
        }
    }

    #[test]
    fn splits_bounded_ascii_arguments_without_allocation() {
        let command = Command::parse(b"echo   hello world").unwrap();
        assert_eq!(command.builtin, Some(Builtin::Echo));
        assert_eq!(command.argc, 3);
        assert_eq!(command.args[1], b"hello");
        assert_eq!(command.args[2], b"world");
    }

    #[test]
    fn unknown_and_empty_lines_are_not_misclassified() {
        let unknown = Command::parse(b"wat").unwrap();
        assert_eq!(unknown.builtin, None);
        assert_eq!(unknown.argc, 1);
        let empty = Command::parse(b"  \t ").unwrap();
        assert_eq!(empty.builtin, None);
        assert_eq!(empty.argc, 0);
    }

    #[test]
    fn rejects_argument_overflow_before_overwriting_storage() {
        let line = b"a a a a a a a a a a a a a a a a a";
        assert!(matches!(
            Command::parse(line),
            Err(ParseError::TooManyArguments)
        ));
    }
}
