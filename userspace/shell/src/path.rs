//! Bounded paths for the single-process shell. Kernel lookup validates every
//! component before the shell stores a canonical working directory.

pub const PATH_BYTES: usize = 255;

pub struct WorkingDir {
    bytes: [u8; PATH_BYTES],
    len: usize,
}

impl WorkingDir {
    pub const fn root() -> Self {
        let mut bytes = [0; PATH_BYTES];
        bytes[0] = b'/';
        Self { bytes, len: 1 }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }

    /// Call only after the kernel has successfully resolved the directory.
    pub fn set(&mut self, path: &[u8]) -> Option<()> {
        let mut bytes = [0; PATH_BYTES];
        let len = canonical(path, &mut bytes)?;
        self.bytes = bytes;
        self.len = len;
        Some(())
    }

    pub fn resolve(&self, input: &[u8], output: &mut [u8; PATH_BYTES]) -> Option<usize> {
        if input.is_empty() || input.contains(&0) {
            return None;
        }
        // Preserve dot components and trailing slashes for the VFS walk.
        // In particular, missing/../file must not become a valid file path.
        let prefix = if input.starts_with(b"/") { 0 } else { self.len };
        let separator = usize::from(prefix > 1);
        let total = prefix.checked_add(separator)?.checked_add(input.len())?;
        if total > output.len() {
            return None;
        }
        output[..prefix].copy_from_slice(&self.bytes[..prefix]);
        if separator != 0 {
            output[prefix] = b'/';
        }
        output[prefix + separator..total].copy_from_slice(input);
        Some(total)
    }
}

fn canonical(source: &[u8], output: &mut [u8; PATH_BYTES]) -> Option<usize> {
    if !source.starts_with(b"/") || source.len() > PATH_BYTES || source.contains(&0) {
        return None;
    }
    output[0] = b'/';
    let mut len = 1;
    apply_components(source, output, &mut len)?;
    Some(len)
}

/// Conservative same-file guard for the current namespace without symlinks
/// or hard links. It is not a replacement for kernel path validation.
pub fn same_path(left: &[u8], right: &[u8]) -> bool {
    let mut a = [0; PATH_BYTES];
    let mut b = [0; PATH_BYTES];
    match (canonical(left, &mut a), canonical(right, &mut b)) {
        (Some(a_len), Some(b_len)) => a[..a_len] == b[..b_len],
        _ => false,
    }
}

fn apply_components(source: &[u8], output: &mut [u8; PATH_BYTES], len: &mut usize) -> Option<()> {
    let mut cursor = 0usize;
    while cursor < source.len() {
        while cursor < source.len() && source[cursor] == b'/' {
            cursor += 1;
        }
        if cursor == source.len() {
            break;
        }
        let start = cursor;
        while cursor < source.len() && source[cursor] != b'/' {
            cursor += 1;
        }
        let component = &source[start..cursor];
        if component == b"." || component.is_empty() {
            continue;
        }
        if component == b".." {
            if *len > 1 {
                while *len > 1 && output[*len - 1] != b'/' {
                    *len -= 1;
                }
                if *len > 1 {
                    *len -= 1;
                }
            }
            continue;
        }
        if component.contains(&0) || component.len() > 31 {
            return None;
        }
        let separator = usize::from(*len > 1);
        if *len + separator + component.len() > output.len() {
            return None;
        }
        if separator != 0 {
            output[*len] = b'/';
            *len += 1;
        }
        output[*len..*len + component.len()].copy_from_slice(component);
        *len += component.len();
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_paths_preserve_components_that_can_fail() {
        let mut cwd = WorkingDir::root();
        cwd.set(b"/tmp/work").unwrap();
        for (input, expected) in [
            (&b"missing/../file"[..], &b"/tmp/work/missing/../file"[..]),
            (&b"file/"[..], &b"/tmp/work/file/"[..]),
            (&b"/welcome/."[..], &b"/welcome/."[..]),
            (&b".."[..], &b"/tmp/work/.."[..]),
        ] {
            let mut output = [0; PATH_BYTES];
            let len = cwd.resolve(input, &mut output).unwrap();
            assert_eq!(&output[..len], expected);
        }
    }

    #[test]
    fn stored_directories_and_same_file_aliases_are_canonical() {
        let mut cwd = WorkingDir::root();
        cwd.set(b"/tmp//work/../.").unwrap();
        assert_eq!(cwd.as_bytes(), b"/tmp");
        assert!(same_path(b"/tmp/a", b"/tmp/./a"));
        assert!(same_path(b"/tmp/a", b"/tmp/work/../a"));
        assert!(!same_path(b"/tmp/a", b"/tmp/b"));
        cwd.set(b"/../../").unwrap();
        assert_eq!(cwd.as_bytes(), b"/");
    }

    #[test]
    fn invalid_paths_cannot_overwrite_working_directory() {
        let mut cwd = WorkingDir::root();
        cwd.set(b"/tmp").unwrap();
        assert_eq!(cwd.set(b"relative"), None);
        assert_eq!(cwd.set(b"/bad\0name"), None);
        assert_eq!(cwd.as_bytes(), b"/tmp");
        let mut output = [0; PATH_BYTES];
        assert_eq!(cwd.resolve(b"", &mut output), None);
        assert_eq!(cwd.resolve(&[b'a'; PATH_BYTES], &mut output), None);
        assert_eq!(cwd.resolve(b"bad\0name", &mut output), None);
    }
}
