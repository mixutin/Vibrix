#![no_std]

//! Bounded pkg-config compatible metadata parsing.
//!
//! The parser deliberately supports the portable subset needed by early Vibrix
//! ports: variable assignments, Name/Description/Version/Requires/Libs/Cflags,
//! ${var} expansion, and dotted numeric minimum-version comparison. It uses no
//! heap allocation and never executes shell syntax.

pub const MAX_VARIABLES: usize = 16;
pub const MAX_FIELDS: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    TooManyVariables,
    TooManyFields,
    InvalidLine,
    InvalidName,
    MissingVariable,
    OutputTooSmall,
    InvalidVersion,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Pair<'a> {
    key: &'a [u8],
    value: &'a [u8],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Field {
    Name,
    Description,
    Version,
    Requires,
    Libs,
    Cflags,
}

impl Field {
    fn key(self) -> &'static [u8] {
        match self {
            Self::Name => b"Name",
            Self::Description => b"Description",
            Self::Version => b"Version",
            Self::Requires => b"Requires",
            Self::Libs => b"Libs",
            Self::Cflags => b"Cflags",
        }
    }
}

pub struct Package<'a> {
    variables: [Pair<'a>; MAX_VARIABLES],
    variable_len: usize,
    fields: [Pair<'a>; MAX_FIELDS],
    field_len: usize,
}

impl<'a> Package<'a> {
    pub fn parse(input: &'a [u8]) -> Result<Self, Error> {
        let mut out = Self {
            variables: [Pair {
                key: b"",
                value: b"",
            }; MAX_VARIABLES],
            variable_len: 0,
            fields: [Pair {
                key: b"",
                value: b"",
            }; MAX_FIELDS],
            field_len: 0,
        };

        for raw in input.split(|&b| b == b'\n') {
            let line = trim(raw);
            if line.is_empty() || line[0] == b'#' {
                continue;
            }
            if let Some(eq) = line.iter().position(|&b| b == b'=') {
                let colon = line.iter().position(|&b| b == b':');
                if colon.is_none_or(|c| eq < c) {
                    let key = trim(&line[..eq]);
                    let value = trim(&line[eq + 1..]);
                    validate_name(key)?;
                    if out.variable_len == MAX_VARIABLES {
                        return Err(Error::TooManyVariables);
                    }
                    if out.variable(key).is_some() {
                        return Err(Error::InvalidLine);
                    }
                    out.variables[out.variable_len] = Pair { key, value };
                    out.variable_len += 1;
                    continue;
                }
            }

            let Some(colon) = line.iter().position(|&b| b == b':') else {
                return Err(Error::InvalidLine);
            };
            let key = trim(&line[..colon]);
            let value = trim(&line[colon + 1..]);
            validate_name(key)?;
            if out.field_len == MAX_FIELDS {
                return Err(Error::TooManyFields);
            }
            if out.raw_field(key).is_some() {
                return Err(Error::InvalidLine);
            }
            out.fields[out.field_len] = Pair { key, value };
            out.field_len += 1;
        }
        Ok(out)
    }

    pub fn variable(&self, name: &[u8]) -> Option<&'a [u8]> {
        self.variables[..self.variable_len]
            .iter()
            .find(|p| p.key == name)
            .map(|p| p.value)
    }

    pub fn raw_field(&self, name: &[u8]) -> Option<&'a [u8]> {
        self.fields[..self.field_len]
            .iter()
            .find(|p| p.key == name)
            .map(|p| p.value)
    }

    pub fn field(&self, field: Field, output: &mut [u8]) -> Result<usize, Error> {
        let raw = self.raw_field(field.key()).unwrap_or(b"");
        self.expand(raw, output)
    }

    pub fn expand(&self, raw: &[u8], output: &mut [u8]) -> Result<usize, Error> {
        let mut dst = 0;
        self.expand_into(raw, output, &mut dst, 0)?;
        Ok(dst)
    }

    fn expand_into(
        &self,
        raw: &[u8],
        output: &mut [u8],
        dst: &mut usize,
        depth: usize,
    ) -> Result<(), Error> {
        if depth >= MAX_VARIABLES {
            return Err(Error::InvalidLine);
        }

        let mut src = 0;
        while src < raw.len() {
            if raw[src] == b'
    pub fn version_at_least(&self, required: &[u8]) -> Result<bool, Error> {
        let actual = self.raw_field(b"Version").ok_or(Error::InvalidVersion)?;
        compare_versions(actual, required)
    }
}

fn copy(output: &mut [u8], dst: &mut usize, bytes: &[u8]) -> Result<(), Error> {
    let end = dst.checked_add(bytes.len()).ok_or(Error::OutputTooSmall)?;
    if end > output.len() {
        return Err(Error::OutputTooSmall);
    }
    output[*dst..end].copy_from_slice(bytes);
    *dst = end;
    Ok(())
}

fn validate_name(name: &[u8]) -> Result<(), Error> {
    if name.is_empty()
        || !name
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
    {
        return Err(Error::InvalidName);
    }
    Ok(())
}

fn trim(mut bytes: &[u8]) -> &[u8] {
    while bytes.first().is_some_and(u8::is_ascii_whitespace) {
        bytes = &bytes[1..];
    }
    while bytes.last().is_some_and(u8::is_ascii_whitespace) {
        bytes = &bytes[..bytes.len() - 1];
    }
    bytes
}

fn compare_versions(a: &[u8], b: &[u8]) -> Result<bool, Error> {
    let mut ai = a.split(|&x| x == b'.');
    let mut bi = b.split(|&x| x == b'.');
    loop {
        let av = ai.next();
        let bv = bi.next();
        if av.is_none() && bv.is_none() {
            return Ok(true);
        }
        let an = component(av.unwrap_or(b"0"))?;
        let bn = component(bv.unwrap_or(b"0"))?;
        if an != bn {
            return Ok(an > bn);
        }
    }
}

fn component(bytes: &[u8]) -> Result<u32, Error> {
    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
        return Err(Error::InvalidVersion);
    }
    let mut value = 0u32;
    for &b in bytes {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add(u32::from(b - b'0')))
            .ok_or(Error::InvalidVersion)?;
    }
    Ok(value)
}

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod tests {
    use super::*;

    const PC: &[u8] = b"prefix=/usr\nlibdir=${prefix}/lib\nincludedir=${prefix}/include\n\nName: demo\nDescription: demo library\nVersion: 2.4.1\nRequires: base >= 1.0\nLibs: -L${libdir} -ldemo\nCflags: -I${includedir}/demo\n";

    #[test]
    fn parses_and_expands_standard_fields() {
        let pkg = Package::parse(PC).unwrap();
        let mut out = [0u8; 128];
        let n = pkg.field(Field::Libs, &mut out).unwrap();
        assert_eq!(&out[..n], b"-L/usr/lib -ldemo");
        let n = pkg.field(Field::Cflags, &mut out).unwrap();
        assert_eq!(&out[..n], b"-I/usr/include/demo");
    }

    #[test]
    fn compares_numeric_versions() {
        let pkg = Package::parse(PC).unwrap();
        assert!(pkg.version_at_least(b"2.4").unwrap());
        assert!(pkg.version_at_least(b"2.4.1").unwrap());
        assert!(!pkg.version_at_least(b"2.5").unwrap());
        assert_eq!(pkg.version_at_least(b"2.a"), Err(Error::InvalidVersion));
    }

    #[test]
    fn fails_closed_on_undefined_variable_and_small_output() {
        let pkg = Package::parse(b"Name: x\nLibs: -L${missing}\n").unwrap();
        let mut out = [0u8; 8];
        assert_eq!(
            pkg.field(Field::Libs, &mut out),
            Err(Error::MissingVariable)
        );

        let pkg = Package::parse(PC).unwrap();
        assert_eq!(pkg.field(Field::Libs, &mut out), Err(Error::OutputTooSmall));
    }

    #[test]
    fn rejects_recursive_variable_cycles() {
        let pkg = Package::parse(b"a=${b}\nb=${a}\nLibs: ${a}\n").unwrap();
        let mut out = [0u8; 32];
        assert_eq!(pkg.field(Field::Libs, &mut out), Err(Error::InvalidLine));
    }

    #[test]
    fn rejects_duplicate_and_malformed_entries() {
        assert!(matches!(
            Package::parse(b"x=1\nx=2\n"),
            Err(Error::InvalidLine)
        ));
        assert!(matches!(
            Package::parse(b"bad name=value\n"),
            Err(Error::InvalidName)
        ));
        assert!(matches!(
            Package::parse(b"not-a-record\n"),
            Err(Error::InvalidLine)
        ));
    }
}
 && raw.get(src + 1) == Some(&b'{') {
                let rest = &raw[src + 2..];
                let Some(close) = rest.iter().position(|&b| b == b'}') else {
                    return Err(Error::InvalidLine);
                };
                let name = &rest[..close];
                validate_name(name)?;
                let value = self.variable(name).ok_or(Error::MissingVariable)?;
                self.expand_into(value, output, dst, depth + 1)?;
                src += 3 + close;
            } else {
                copy(output, dst, &raw[src..src + 1])?;
                src += 1;
            }
        }
        Ok(())
    }

    pub fn version_at_least(&self, required: &[u8]) -> Result<bool, Error> {
        let actual = self.raw_field(b"Version").ok_or(Error::InvalidVersion)?;
        compare_versions(actual, required)
    }
}

fn copy(output: &mut [u8], dst: &mut usize, bytes: &[u8]) -> Result<(), Error> {
    let end = dst.checked_add(bytes.len()).ok_or(Error::OutputTooSmall)?;
    if end > output.len() {
        return Err(Error::OutputTooSmall);
    }
    output[*dst..end].copy_from_slice(bytes);
    *dst = end;
    Ok(())
}

fn validate_name(name: &[u8]) -> Result<(), Error> {
    if name.is_empty()
        || !name
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
    {
        return Err(Error::InvalidName);
    }
    Ok(())
}

fn trim(mut bytes: &[u8]) -> &[u8] {
    while bytes.first().is_some_and(u8::is_ascii_whitespace) {
        bytes = &bytes[1..];
    }
    while bytes.last().is_some_and(u8::is_ascii_whitespace) {
        bytes = &bytes[..bytes.len() - 1];
    }
    bytes
}

fn compare_versions(a: &[u8], b: &[u8]) -> Result<bool, Error> {
    let mut ai = a.split(|&x| x == b'.');
    let mut bi = b.split(|&x| x == b'.');
    loop {
        let av = ai.next();
        let bv = bi.next();
        if av.is_none() && bv.is_none() {
            return Ok(true);
        }
        let an = component(av.unwrap_or(b"0"))?;
        let bn = component(bv.unwrap_or(b"0"))?;
        if an != bn {
            return Ok(an > bn);
        }
    }
}

fn component(bytes: &[u8]) -> Result<u32, Error> {
    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
        return Err(Error::InvalidVersion);
    }
    let mut value = 0u32;
    for &b in bytes {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add(u32::from(b - b'0')))
            .ok_or(Error::InvalidVersion)?;
    }
    Ok(value)
}

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod tests {
    use super::*;

    const PC: &[u8] = b"prefix=/usr\nlibdir=${prefix}/lib\nincludedir=${prefix}/include\n\nName: demo\nDescription: demo library\nVersion: 2.4.1\nRequires: base >= 1.0\nLibs: -L${libdir} -ldemo\nCflags: -I${includedir}/demo\n";

    #[test]
    fn parses_and_expands_standard_fields() {
        let pkg = Package::parse(PC).unwrap();
        let mut out = [0u8; 128];
        let n = pkg.field(Field::Libs, &mut out).unwrap();
        assert_eq!(&out[..n], b"-L/usr/lib -ldemo");
        let n = pkg.field(Field::Cflags, &mut out).unwrap();
        assert_eq!(&out[..n], b"-I/usr/include/demo");
    }

    #[test]
    fn compares_numeric_versions() {
        let pkg = Package::parse(PC).unwrap();
        assert!(pkg.version_at_least(b"2.4").unwrap());
        assert!(pkg.version_at_least(b"2.4.1").unwrap());
        assert!(!pkg.version_at_least(b"2.5").unwrap());
        assert_eq!(pkg.version_at_least(b"2.a"), Err(Error::InvalidVersion));
    }

    #[test]
    fn fails_closed_on_undefined_variable_and_small_output() {
        let pkg = Package::parse(b"Name: x\nLibs: -L${missing}\n").unwrap();
        let mut out = [0u8; 8];
        assert_eq!(
            pkg.field(Field::Libs, &mut out),
            Err(Error::MissingVariable)
        );

        let pkg = Package::parse(PC).unwrap();
        assert_eq!(pkg.field(Field::Libs, &mut out), Err(Error::OutputTooSmall));
    }

    #[test]
    fn rejects_duplicate_and_malformed_entries() {
        assert!(matches!(
            Package::parse(b"x=1\nx=2\n"),
            Err(Error::InvalidLine)
        ));
        assert!(matches!(
            Package::parse(b"bad name=value\n"),
            Err(Error::InvalidName)
        ));
        assert!(matches!(
            Package::parse(b"not-a-record\n"),
            Err(Error::InvalidLine)
        ));
    }
}
