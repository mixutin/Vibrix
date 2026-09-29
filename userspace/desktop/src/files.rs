//! Bounded read-only file browser over real native VFS syscalls.
use vibrix_shell::{
    path::{PATH_BYTES, WorkingDir},
    system::System,
};
use vibrix_syscall::abi;
pub const ENTRIES: usize = 16;
pub struct Files {
    pub cwd: WorkingDir,
    pub entries: [abi::DirEntry; ENTRIES],
    pub count: usize,
    pub selected: usize,
    pub preview: [u8; 1024],
    pub preview_len: usize,
    pub message: &'static [u8],
}
impl Default for Files {
    fn default() -> Self {
        Self::new()
    }
}
impl Files {
    pub const fn new() -> Self {
        Self {
            cwd: WorkingDir::root(),
            entries: [abi::DirEntry::EMPTY; ENTRIES],
            count: 0,
            selected: 0,
            preview: [0; 1024],
            preview_len: 0,
            message: b"Select a file or directory. RAM only.",
        }
    }
    pub fn refresh(&mut self, io: &mut dyn System) {
        self.count = 0;
        self.selected = 0;
        self.preview_len = 0;
        self.message = b"Select a file or directory. RAM only.";
        for index in 0..ENTRIES {
            let mut entry = abi::DirEntry::EMPTY;
            match io.read_dir(self.cwd.as_bytes(), index as u64, &mut entry) {
                Ok(true) if usize::from(entry.name_len) <= entry.name.len() => {
                    self.entries[index] = entry;
                    self.count += 1;
                }
                Ok(false) => return,
                _ => {
                    self.message = b"Directory read failed.";
                    return;
                }
            }
        }
        self.message = b"Showing first 16 entries; use terminal ls for more.";
    }
    pub fn parent(&mut self, io: &mut dyn System) {
        let mut path = [0; PATH_BYTES];
        if let Some(len) = self.cwd.resolve(b"..", &mut path) {
            let _ = self.cwd.set(&path[..len]);
        }
        self.refresh(io);
    }
    pub fn open(&mut self, index: usize, io: &mut dyn System) {
        if index >= self.count {
            return;
        }
        self.selected = index;
        let entry = self.entries[index];
        let mut path = [0; PATH_BYTES];
        let Some(len) = self
            .cwd
            .resolve(&entry.name[..usize::from(entry.name_len)], &mut path)
        else {
            self.message = b"Path too long.";
            return;
        };
        if entry.kind == abi::ENTRY_DIRECTORY {
            let _ = self.cwd.set(&path[..len]);
            self.refresh(io);
            return;
        }
        self.preview_len = 0;
        if entry.kind != abi::ENTRY_FILE {
            self.message = b"Device nodes are not opened in the file viewer.";
            return;
        }
        let Ok(fd) = io.open(&path[..len], abi::OPEN_READ) else {
            self.message = b"File open failed.";
            return;
        };
        self.message = b"Read-only preview: first 1024 bytes, ASCII display.";
        while self.preview_len < self.preview.len() {
            match io.read(fd, &mut self.preview[self.preview_len..]) {
                Ok(0) => break,
                Ok(count) if count <= self.preview.len() - self.preview_len => {
                    self.preview_len += count
                }
                _ => {
                    self.message = b"File read failed.";
                    break;
                }
            }
        }
        if io.close(fd).is_err() {
            self.message = b"File close failed.";
        }
    }
}
