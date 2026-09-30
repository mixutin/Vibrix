#![no_std]

//! Fixed-capacity text editing core for native Vibrix developer tooling.
//!
//! The core intentionally separates editing state from terminal and filesystem
//! I/O. Callers can load one bounded UTF-8/byte buffer, move a byte cursor,
//! insert/delete bytes and save the resulting slice through their own VFS path.

pub const TEXT_BYTES: usize = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    TooLarge,
    Full,
    Cursor,
}

pub struct Editor {
    bytes: [u8; TEXT_BYTES],
    len: usize,
    cursor: usize,
    dirty: bool,
}

impl Editor {
    pub const fn new() -> Self {
        Self {
            bytes: [0; TEXT_BYTES],
            len: 0,
            cursor: 0,
            dirty: false,
        }
    }

    pub fn load(&mut self, input: &[u8]) -> Result<(), Error> {
        if input.len() > TEXT_BYTES {
            return Err(Error::TooLarge);
        }
        self.bytes = [0; TEXT_BYTES];
        self.bytes[..input.len()].copy_from_slice(input);
        self.len = input.len();
        self.cursor = 0;
        self.dirty = false;
        Ok(())
    }

    pub fn text(&self) -> &[u8] {
        &self.bytes[..self.len]
    }

    pub const fn cursor(&self) -> usize {
        self.cursor
    }

    pub const fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn set_cursor(&mut self, cursor: usize) -> Result<(), Error> {
        if cursor > self.len {
            return Err(Error::Cursor);
        }
        self.cursor = cursor;
        Ok(())
    }

    pub fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn move_right(&mut self) {
        if self.cursor < self.len {
            self.cursor += 1;
        }
    }

    pub fn insert(&mut self, input: &[u8]) -> Result<(), Error> {
        let end = self.len.checked_add(input.len()).ok_or(Error::Full)?;
        if end > TEXT_BYTES {
            return Err(Error::Full);
        }
        if input.is_empty() {
            return Ok(());
        }

        self.bytes.copy_within(self.cursor..self.len, self.cursor + input.len());
        self.bytes[self.cursor..self.cursor + input.len()].copy_from_slice(input);
        self.cursor += input.len();
        self.len = end;
        self.dirty = true;
        Ok(())
    }

    pub fn backspace(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        let remove = self.cursor - 1;
        self.bytes.copy_within(self.cursor..self.len, remove);
        self.len -= 1;
        self.cursor -= 1;
        self.bytes[self.len] = 0;
        self.dirty = true;
        true
    }

    pub fn delete(&mut self) -> bool {
        if self.cursor == self.len {
            return false;
        }
        self.bytes.copy_within(self.cursor + 1..self.len, self.cursor);
        self.len -= 1;
        self.bytes[self.len] = 0;
        self.dirty = true;
        true
    }

    pub fn mark_saved(&mut self) {
        self.dirty = false;
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_insert_delete_and_dirty_lifecycle() {
        let mut editor = Editor::new();
        editor.load(b"hello").unwrap();
        assert_eq!(editor.text(), b"hello");
        assert!(!editor.is_dirty());

        editor.set_cursor(5).unwrap();
        editor.insert(b" world").unwrap();
        assert_eq!(editor.text(), b"hello world");
        assert_eq!(editor.cursor(), 11);
        assert!(editor.is_dirty());

        editor.move_left();
        assert!(editor.backspace());
        assert_eq!(editor.text(), b"hello word");
        assert!(editor.delete());
        assert_eq!(editor.text(), b"hello wor");
        editor.mark_saved();
        assert!(!editor.is_dirty());
    }

    #[test]
    fn failed_capacity_and_cursor_changes_are_transactional() {
        let mut editor = Editor::new();
        editor.load(b"abc").unwrap();
        let before = *editor.text().first().unwrap();
        assert_eq!(editor.set_cursor(4), Err(Error::Cursor));
        assert_eq!(editor.cursor(), 0);
        assert_eq!(*editor.text().first().unwrap(), before);

        let full = [b'x'; TEXT_BYTES];
        editor.load(&full).unwrap();
        assert_eq!(editor.insert(b"x"), Err(Error::Full));
        assert_eq!(editor.text(), full.as_slice());
        assert!(!editor.is_dirty());
    }

    #[test]
    fn boundaries_do_not_underflow_or_overrun() {
        let mut editor = Editor::new();
        editor.load(b"x").unwrap();
        editor.move_left();
        assert_eq!(editor.cursor(), 0);
        assert!(!editor.backspace());
        editor.move_right();
        editor.move_right();
        assert_eq!(editor.cursor(), 1);
        assert!(!editor.delete());
    }
}
