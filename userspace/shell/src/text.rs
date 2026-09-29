//! Byte-oriented, allocation-free text utilities. Limits fail before output.

use crate::manual::Builtin;
use crate::runtime::{Error, Result};
use crate::system::parse_number;

pub const TEXT_BYTES: usize = 1024;
pub const TEXT_LINES: usize = 128;

pub struct Options<'a> {
    pub kind: Builtin,
    pub file: Option<&'a [u8]>,
    count: usize,
    pattern: &'a [u8],
    insensitive: bool,
    numbered: bool,
    invert: bool,
    reverse: bool,
    unique: bool,
    counts: bool,
    metric: u8,
}

impl<'a> Options<'a> {
    pub fn parse(kind: Builtin, args: &[&'a [u8]]) -> Result<Self> {
        let mut options = Self {
            kind,
            file: None,
            count: 10,
            pattern: b"",
            insensitive: false,
            numbered: false,
            invert: false,
            reverse: false,
            unique: false,
            counts: false,
            metric: 0,
        };
        let mut index = 0;
        while let Some(&arg) = args.get(index) {
            if arg == b"--" {
                index += 1;
                break;
            }
            if !arg.starts_with(b"-") || arg.len() == 1 {
                break;
            }
            match (kind, arg) {
                (Builtin::Head | Builtin::Tail, b"-n") => {
                    index += 1;
                    options.count = args
                        .get(index)
                        .and_then(|value| parse_number(value))
                        .and_then(|value| usize::try_from(value).ok())
                        .ok_or(Error::Usage)?;
                }
                (Builtin::Grep, b"-i") => options.insensitive = true,
                (Builtin::Grep, b"-n") => options.numbered = true,
                (Builtin::Grep, b"-v") => options.invert = true,
                (Builtin::Sort, b"-r") => options.reverse = true,
                (Builtin::Sort, b"-u") => options.unique = true,
                (Builtin::Uniq, b"-c") => options.counts = true,
                (Builtin::Wc, b"-l" | b"-w" | b"-c") if options.metric == 0 => {
                    options.metric = arg[1];
                }
                _ => return Err(Error::Usage),
            }
            index += 1;
        }
        if kind == Builtin::Grep {
            options.pattern = args.get(index).copied().ok_or(Error::Usage)?;
            index += 1;
        }
        options.file = args.get(index).copied();
        if args.len() > index + 1 {
            return Err(Error::Usage);
        }
        Ok(options)
    }

    pub fn render(&self, data: &[u8], mut emit: impl FnMut(&[u8]) -> Result<()>) -> Result<u8> {
        if data.len() > TEXT_BYTES {
            return Err(Error::Message(b"text input exceeds 1024 bytes"));
        }
        match self.kind {
            Builtin::Head => {
                for line in lines(data).take(self.count) {
                    emit(line)?;
                }
            }
            Builtin::Tail => {
                let skip = lines(data).count().saturating_sub(self.count);
                for line in lines(data).skip(skip) {
                    emit(line)?;
                }
            }
            Builtin::Wc => {
                let mut words = 0;
                let mut inside = false;
                for &byte in data {
                    let word = !byte.is_ascii_whitespace();
                    if word && !inside {
                        words += 1;
                    }
                    inside = word;
                }
                let counts = [
                    data.iter().filter(|&&byte| byte == b'\n').count(),
                    words,
                    data.len(),
                ];
                if self.metric == 0 {
                    for (index, count) in counts.into_iter().enumerate() {
                        if index != 0 {
                            emit(b" ")?;
                        }
                        decimal(count, &mut emit)?;
                    }
                } else {
                    let index = match self.metric {
                        b'l' => 0,
                        b'w' => 1,
                        _ => 2,
                    };
                    decimal(counts[index], &mut emit)?;
                }
                emit(b"\n")?;
            }
            Builtin::Grep => {
                let mut matched = false;
                for (index, line) in lines(data).enumerate() {
                    let selected = contains(body(line), self.pattern, self.insensitive) != self.invert;
                    if selected {
                        matched = true;
                        if self.numbered {
                            decimal(index + 1, &mut emit)?;
                            emit(b":")?;
                        }
                        emit(body(line))?;
                        emit(b"\n")?;
                    }
                }
                return Ok(u8::from(!matched));
            }
            Builtin::Sort | Builtin::Uniq => {
                let mut rows = [&b""[..]; TEXT_LINES];
                let mut len = 0;
                for line in lines(data) {
                    if len == TEXT_LINES {
                        return Err(Error::Message(b"sort/uniq input exceeds 128 lines"));
                    }
                    rows[len] = body(line);
                    len += 1;
                }
                let rows = &mut rows[..len];
                if self.kind == Builtin::Sort {
                    rows.sort_unstable();
                    if self.reverse {
                        rows.reverse();
                    }
                }
                let mut index = 0;
                while index < rows.len() {
                    let start = index;
                    index += 1;
                    if self.kind == Builtin::Uniq || self.unique {
                        while index < rows.len() && rows[index] == rows[start] {
                            index += 1;
                        }
                    }
                    if self.counts {
                        decimal(index - start, &mut emit)?;
                        emit(b" ")?;
                    }
                    emit(rows[start])?;
                    emit(b"\n")?;
                }
            }
            Builtin::Nl => {
                for (index, line) in lines(data).enumerate() {
                    decimal(index + 1, &mut emit)?;
                    emit(b"\t")?;
                    emit(line)?;
                }
            }
            Builtin::Hexdump => {
                const HEX: &[u8] = b"0123456789abcdef";
                for (index, row) in data.chunks(16).enumerate() {
                    let mut out = [b' '; 58];
                    let offset = index * 16;
                    for (digit, slot) in out[..8].iter_mut().enumerate() {
                        *slot = HEX[(offset >> ((7 - digit) * 4)) & 15];
                    }
                    out[8] = b':';
                    for (column, &byte) in row.iter().enumerate() {
                        out[10 + column * 3] = HEX[usize::from(byte >> 4)];
                        out[11 + column * 3] = HEX[usize::from(byte & 15)];
                    }
                    let end = 9 + row.len() * 3;
                    out[end] = b'\n';
                    emit(&out[..=end])?;
                }
            }
            _ => return Err(Error::Usage),
        }
        Ok(0)
    }
}

fn lines(data: &[u8]) -> impl Iterator<Item = &[u8]> {
    data.split_inclusive(|&byte| byte == b'\n')
}

fn body(line: &[u8]) -> &[u8] {
    line.strip_suffix(b"\n").unwrap_or(line)
}

fn decimal(value: usize, emit: &mut impl FnMut(&[u8]) -> Result<()>) -> Result<()> {
    let mut result = Ok(());
    crate::fetch::decimal(value as u64, &mut |bytes| {
        if result.is_ok() {
            result = emit(bytes);
        }
    });
    result
}

pub fn contains(haystack: &[u8], needle: &[u8], insensitive: bool) -> bool {
    needle.is_empty()
        || haystack.windows(needle.len()).any(|part| {
            if insensitive {
                part.eq_ignore_ascii_case(needle)
            } else {
                part == needle
            }
        })
}

fn trim_slashes(mut path: &[u8]) -> &[u8] {
    while path.len() > 1 && path.ends_with(b"/") {
        path = &path[..path.len() - 1];
    }
    path
}

pub fn basename(path: &[u8]) -> &[u8] {
    let path = trim_slashes(path);
    if path.is_empty() {
        return b".";
    }
    if path == b"/" {
        return path;
    }
    path.rsplit(|&byte| byte == b'/').next().unwrap_or(b".")
}

pub fn dirname(path: &[u8]) -> &[u8] {
    let path = trim_slashes(path);
    match path.iter().rposition(|&byte| byte == b'/') {
        Some(0) => b"/",
        Some(index) => trim_slashes(&path[..index]),
        None => b".",
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use self::std::vec::Vec;
    use super::*;

    fn output(kind: Builtin, args: &[&[u8]], data: &[u8]) -> (Result<u8>, Vec<u8>) {
        let options = Options::parse(kind, args).unwrap();
        let mut bytes = Vec::new();
        let result = options.render(data, |part| {
            bytes.extend_from_slice(part);
            Ok(())
        });
        (result, bytes)
    }

    #[test]
    fn head_tail_and_wc_handle_empty_and_unterminated_lines() {
        assert_eq!(output(Builtin::Head, &[b"-n", b"0"], b"a\nb").1, b"");
        assert_eq!(output(Builtin::Tail, &[b"-n", b"1"], b"a\nb").1, b"b");
        assert_eq!(output(Builtin::Head, &[b"-n", b"1"], b"a\nb").1, b"a\n");
        assert_eq!(output(Builtin::Wc, &[], b"a b\nc").1, b"1 3 5\n");
        assert_eq!(output(Builtin::Wc, &[], b"").1, b"0 0 0\n");
    }

    #[test]
    fn grep_is_literal_and_has_distinct_match_status() {
        assert_eq!(output(Builtin::Grep, &[b"-i", b"-n", b"a."], b"A.\nabc").1, b"1:A.\n");
        assert_eq!(output(Builtin::Grep, &[b"missing"], b"a").0, Ok(1));
        assert_eq!(output(Builtin::Grep, &[b""], b"").0, Ok(1));
        assert_eq!(output(Builtin::Grep, &[b"-v", b"a"], b"a\nb").1, b"b\n");
    }

    #[test]
    fn sorting_uniqueness_and_numbering_use_documented_byte_semantics() {
        assert_eq!(output(Builtin::Sort, &[b"-u"], b"b\na\nb\n").1, b"a\nb\n");
        assert_eq!(output(Builtin::Sort, &[b"-r"], b"a\nb").1, b"b\na\n");
        assert_eq!(output(Builtin::Uniq, &[b"-c"], b"a\na\nb\na").1, b"2 a\n1 b\n1 a\n");
        assert_eq!(output(Builtin::Nl, &[], b"\na").1, b"1\t\n2\ta");
        assert_eq!(output(Builtin::Hexdump, &[], &[0, 255]).1, b"00000000: 00 ff\n");
    }

    #[test]
    fn oversized_input_and_rows_fail_before_output() {
        let (result, bytes) = output(Builtin::Sort, &[], &[b'\n'; TEXT_LINES + 1]);
        assert!(result.is_err());
        assert!(bytes.is_empty());
        let (result, bytes) = output(Builtin::Head, &[], &[b'x'; TEXT_BYTES + 1]);
        assert!(result.is_err());
        assert!(bytes.is_empty());
        assert!(Options::parse(Builtin::Head, &[b"-n", b"18446744073709551616"]).is_err());
        assert!(Options::parse(Builtin::Wc, &[b"-z"]).is_err());
    }

    #[test]
    fn path_components_handle_roots_and_trailing_slashes() {
        assert_eq!(basename(b"/tmp/file///"), b"file");
        assert_eq!(basename(b"///"), b"/");
        assert_eq!(basename(b""), b".");
        assert_eq!(dirname(b"file"), b".");
        assert_eq!(dirname(b"/file"), b"/");
        assert_eq!(dirname(b"/tmp//file/"), b"/tmp");
        assert_eq!(dirname(b"///"), b"/");
    }
}
