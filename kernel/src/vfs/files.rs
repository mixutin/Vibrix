//! A descriptor table owns its namespace borrow and all open descriptions.
//! Duplication shares offsets. No external caller can unlink behind an open
//! description because namespace mutation is mediated by this owner.
use super::{Entry, Error, Kind, Metadata, Node, Result, Vfs, pipe::Pipe};

#[cfg(test)]
#[path = "dup2_tests.rs"]
mod dup2_tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    Read,
    Write,
    ReadWrite,
}

impl Access {
    fn readable(self) -> bool {
        matches!(self, Self::Read | Self::ReadWrite)
    }
    fn writable(self) -> bool {
        matches!(self, Self::Write | Self::ReadWrite)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Open {
    pub access: Access,
    pub truncate: bool,
    pub append: bool,
}

impl Open {
    pub const READ: Self = Self {
        access: Access::Read,
        truncate: false,
        append: false,
    };
    pub const READ_WRITE: Self = Self {
        access: Access::ReadWrite,
        truncate: false,
        append: false,
    };
    pub const REPLACE: Self = Self {
        access: Access::Write,
        truncate: true,
        append: false,
    };
}

#[derive(Clone, Copy)]
enum Object {
    Node(Node),
    PipeRead(usize),
    PipeWrite(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdvisoryLock {
    Shared,
    Exclusive,
}

#[derive(Clone, Copy)]
struct Description {
    object: Object,
    offset: usize,
    options: Open,
    references: usize,
    lock: Option<AdvisoryLock>,
}

pub const RIGHT_READ: u8 = 1 << 0;
pub const RIGHT_WRITE: u8 = 1 << 1;
pub const RIGHT_SEEK: u8 = 1 << 2;
pub const RIGHTS_ALL: u8 = RIGHT_READ | RIGHT_WRITE | RIGHT_SEEK;

pub struct Files<'a, const M: usize, const D: usize, const P: usize, const B: usize> {
    vfs: Vfs<'a, M>,
    descriptors: [Option<usize>; D],
    descriptor_rights: [u8; D],
    descriptions: [Option<Description>; D],
    pipes: [Option<Pipe<B>>; P],
}

impl<'a, const M: usize, const D: usize, const P: usize, const B: usize> Files<'a, M, D, P, B> {
    pub fn new(vfs: Vfs<'a, M>) -> Self {
        Self {
            vfs,
            descriptors: [None; D],
            descriptor_rights: [0; D],
            descriptions: [None; D],
            pipes: core::array::from_fn(|_| None),
        }
    }

    fn description_index(&self, fd: usize) -> Result<usize> {
        self.descriptors
            .get(fd)
            .copied()
            .flatten()
            .ok_or(Error::BadDescriptor)
    }

    fn description(&self, fd: usize) -> Result<Description> {
        Ok(self.descriptions[self.description_index(fd)?].expect("descriptor owns description"))
    }

    pub fn open(&mut self, path: &str, options: Open) -> Result<usize> {
        if (options.truncate || options.append) && !options.access.writable() {
            return Err(Error::AccessDenied);
        }
        // Check both resources before truncate so exhaustion cannot destroy data.
        let fd = self
            .descriptors
            .iter()
            .position(Option::is_none)
            .ok_or(Error::NoSpace)?;
        let description = self
            .descriptions
            .iter()
            .position(Option::is_none)
            .ok_or(Error::NoSpace)?;
        let node = self.vfs.resolve(path)?;
        let kind = self.vfs.metadata(node)?.kind;
        if kind == Kind::Directory && options.access.writable() {
            return Err(Error::IsDirectory);
        }
        if (options.append || options.truncate) && kind != Kind::File {
            return Err(Error::NotSeekable);
        }
        if options.truncate {
            self.vfs.truncate(node)?;
        }
        self.descriptions[description] = Some(Description {
            object: Object::Node(node),
            offset: 0,
            options,
            references: 1,
            lock: None,
        });
        self.descriptors[fd] = Some(description);
        self.descriptor_rights[fd] = match options.access {
            Access::Read => RIGHT_READ | RIGHT_SEEK,
            Access::Write => RIGHT_WRITE | RIGHT_SEEK,
            Access::ReadWrite => RIGHTS_ALL,
        };
        Ok(fd)
    }

    pub fn rights(&self, fd: usize) -> Result<u8> {
        self.description_index(fd)?;
        Ok(self.descriptor_rights[fd])
    }

    pub fn restrict_rights(&mut self, fd: usize, rights: u8) -> Result<()> {
        self.description_index(fd)?;
        if rights & !RIGHTS_ALL != 0 || rights & !self.descriptor_rights[fd] != 0 {
            return Err(Error::AccessDenied);
        }
        self.descriptor_rights[fd] = rights;
        Ok(())
    }

    pub fn dup(&mut self, fd: usize) -> Result<usize> {
        let index = self.description_index(fd)?;
        let free = self
            .descriptors
            .iter()
            .position(Option::is_none)
            .ok_or(Error::NoSpace)?;
        self.descriptions[index]
            .as_mut()
            .expect("live description")
            .references += 1;
        self.descriptors[free] = Some(index);
        self.descriptor_rights[free] = self.descriptor_rights[fd];
        Ok(free)
    }

    /// Rebind an exact descriptor to the source's open description. Validation
    /// happens before closing the target. Exclusive table ownership makes the
    /// close/rebind indivisible to other callers; no spare slot is required.
    pub fn dup2(&mut self, source: usize, target: usize) -> Result<usize> {
        let index = self.description_index(source)?;
        if target >= D {
            return Err(Error::BadDescriptor);
        }
        if self.descriptors[target] == Some(index) {
            return Ok(target);
        }
        let references = self.descriptions[index]
            .as_ref()
            .expect("live description")
            .references
            .checked_add(1)
            .ok_or(Error::NoSpace)?;
        if self.descriptors[target].is_some() {
            self.close(target)?;
        }
        self.descriptions[index]
            .as_mut()
            .expect("source remains live")
            .references = references;
        self.descriptors[target] = Some(index);
        self.descriptor_rights[target] = self.descriptor_rights[source];
        Ok(target)
    }

    /// Acquire a non-blocking advisory lock on a regular file.
    ///
    /// Locks are attached to the shared open description, so duplicated
    /// descriptors share one lock and the lock is released only when the final
    /// reference closes. They are advisory: read/write syscalls do not consult
    /// them automatically.
    pub fn lock(&mut self, fd: usize, requested: AdvisoryLock) -> Result<()> {
        let index = self.description_index(fd)?;
        let description = self.descriptions[index].expect("live description");
        let node = match description.object {
            Object::Node(node) if self.vfs.metadata(node)?.kind == Kind::File => node,
            _ => return Err(Error::NotSeekable),
        };
        if description.lock == Some(requested) {
            return Ok(());
        }

        for (other_index, other) in self.descriptions.iter().enumerate() {
            if other_index == index {
                continue;
            }
            let Some(other) = other else {
                continue;
            };
            if !matches!(other.object, Object::Node(other_node) if other_node == node) {
                continue;
            }
            match (requested, other.lock) {
                (AdvisoryLock::Shared, Some(AdvisoryLock::Exclusive))
                | (AdvisoryLock::Exclusive, Some(_)) => return Err(Error::WouldBlock),
                _ => {}
            }
        }

        self.descriptions[index].as_mut().expect("live description").lock = Some(requested);
        Ok(())
    }

    pub fn unlock(&mut self, fd: usize) -> Result<()> {
        let index = self.description_index(fd)?;
        self.descriptions[index].as_mut().expect("live description").lock = None;
        Ok(())
    }

    pub fn advisory_lock(&self, fd: usize) -> Result<Option<AdvisoryLock>> {
        Ok(self.descriptions[self.description_index(fd)?]
            .expect("live description")
            .lock)
    }

    pub fn close(&mut self, fd: usize) -> Result<()> {
        let index = self.description_index(fd)?;
        self.descriptors[fd] = None;
        self.descriptor_rights[fd] = 0;
        let description = self.descriptions[index].as_mut().expect("live description");
        description.references -= 1;
        if description.references != 0 {
            return Ok(());
        }
        let object = description.object;
        self.descriptions[index] = None;
        let pipe_index = match object {
            Object::Node(_) => return Ok(()),
            Object::PipeRead(i) => {
                self.pipes[i].as_mut().expect("live pipe").reader = false;
                i
            }
            Object::PipeWrite(i) => {
                self.pipes[i].as_mut().expect("live pipe").writer = false;
                i
            }
        };
        let pipe = self.pipes[pipe_index].as_ref().expect("live pipe");
        if !pipe.reader && !pipe.writer {
            self.pipes[pipe_index] = None;
        }
        Ok(())
    }

    pub fn read(&mut self, fd: usize, buffer: &mut [u8]) -> Result<usize> {
        let description = self.description(fd)?;
        if self.descriptor_rights[fd] & RIGHT_READ == 0 || !description.options.access.readable() {
            return Err(Error::AccessDenied);
        }
        match description.object {
            Object::PipeRead(i) => self.pipes[i].as_mut().expect("live pipe").read(buffer),
            Object::PipeWrite(_) => Err(Error::AccessDenied),
            Object::Node(node) => {
                let kind = self.vfs.metadata(node)?.kind;
                if kind == Kind::Directory {
                    return Err(Error::IsDirectory);
                }
                let limit = if kind == Kind::File {
                    buffer.len().min(usize::MAX - description.offset)
                } else {
                    buffer.len()
                };
                let count = self
                    .vfs
                    .read(node, description.offset, &mut buffer[..limit])?;
                if kind == Kind::File {
                    let index = self.description_index(fd)?;
                    self.descriptions[index]
                        .as_mut()
                        .expect("live description")
                        .offset += count;
                }
                Ok(count)
            }
        }
    }

    pub fn write(&mut self, fd: usize, buffer: &[u8]) -> Result<usize> {
        let description = self.description(fd)?;
        if self.descriptor_rights[fd] & RIGHT_WRITE == 0 || !description.options.access.writable() {
            return Err(Error::AccessDenied);
        }
        match description.object {
            Object::PipeWrite(i) => self.pipes[i].as_mut().expect("live pipe").write(buffer),
            Object::PipeRead(_) => Err(Error::AccessDenied),
            Object::Node(node) => {
                let metadata = self.vfs.metadata(node)?;
                if metadata.kind == Kind::Directory {
                    return Err(Error::IsDirectory);
                }
                if buffer.is_empty() {
                    return Ok(0);
                }
                let offset = if description.options.append {
                    metadata.len
                } else {
                    description.offset
                };
                if metadata.kind == Kind::File {
                    offset
                        .checked_add(buffer.len())
                        .ok_or(Error::InvalidOffset)?;
                }
                let count = self.vfs.write(node, offset, buffer)?;
                if metadata.kind == Kind::File {
                    let index = self.description_index(fd)?;
                    self.descriptions[index]
                        .as_mut()
                        .expect("live description")
                        .offset = offset + count;
                }
                Ok(count)
            }
        }
    }

    pub fn seek(&mut self, fd: usize, offset: usize) -> Result<()> {
        let index = self.description_index(fd)?;
        let description = self.descriptions[index].expect("live description");
        match description.object {
            Object::Node(node) if self.vfs.metadata(node)?.kind == Kind::File => {}
            _ => return Err(Error::NotSeekable),
        }
        if self.descriptor_rights[fd] & RIGHT_SEEK == 0 {
            return Err(Error::AccessDenied);
        }
        self.descriptions[index]
            .as_mut()
            .expect("live description")
            .offset = offset;
        Ok(())
    }

    pub fn pipe(&mut self) -> Result<(usize, usize)> {
        if B == 0 {
            return Err(Error::NoSpace);
        }
        let mut fds = self
            .descriptors
            .iter()
            .enumerate()
            .filter(|(_, d)| d.is_none())
            .map(|(i, _)| i);
        let read_fd = fds.next().ok_or(Error::NoSpace)?;
        let write_fd = fds.next().ok_or(Error::NoSpace)?;
        let mut descriptions = self
            .descriptions
            .iter()
            .enumerate()
            .filter(|(_, d)| d.is_none())
            .map(|(i, _)| i);
        let read_description = descriptions.next().ok_or(Error::NoSpace)?;
        let write_description = descriptions.next().ok_or(Error::NoSpace)?;
        let pipe = self
            .pipes
            .iter()
            .position(Option::is_none)
            .ok_or(Error::NoSpace)?;
        // All reservations succeeded before any table mutation.
        self.pipes[pipe] = Some(Pipe::new());
        self.descriptions[read_description] = Some(Description {
            object: Object::PipeRead(pipe),
            offset: 0,
            options: Open::READ,
            references: 1,
            lock: None,
        });
        self.descriptions[write_description] = Some(Description {
            object: Object::PipeWrite(pipe),
            offset: 0,
            options: Open {
                access: Access::Write,
                truncate: false,
                append: false,
            },
            references: 1,
            lock: None,
        });
        self.descriptors[read_fd] = Some(read_description);
        self.descriptor_rights[read_fd] = RIGHT_READ;
        self.descriptors[write_fd] = Some(write_description);
        self.descriptor_rights[write_fd] = RIGHT_WRITE;
        Ok((read_fd, write_fd))
    }

    pub fn create(&mut self, path: &str) -> Result<()> {
        self.vfs.create(path, Kind::File).map(|_| ())
    }
    pub fn mkdir(&mut self, path: &str) -> Result<()> {
        self.vfs.create(path, Kind::Directory).map(|_| ())
    }

    pub fn remove(&mut self, path: &str) -> Result<()> {
        let node = self.vfs.resolve(path)?;
        if self
            .descriptions
            .iter()
            .flatten()
            .any(|d| matches!(d.object, Object::Node(open) if open == node))
        {
            return Err(Error::Busy);
        }
        self.vfs.remove(path)
    }

    pub fn metadata(&self, path: &str) -> Result<Metadata> {
        self.vfs.metadata(self.vfs.resolve(path)?)
    }

    pub fn entry(&self, path: &str, index: usize) -> Result<Option<Entry>> {
        self.vfs.entry(self.vfs.resolve(path)?, index)
    }

    /// Kernel-owned device drivers use this to inject bytes into a mounted
    /// device while ordinary consumers continue to read through descriptors.
    pub fn device_input(&mut self, path: &str, byte: u8) -> Result<()> {
        let node = self.vfs.resolve(path)?;
        if self.vfs.metadata(node)?.kind != Kind::Device {
            return Err(Error::Unsupported);
        }
        self.vfs.device_input(node, byte)
    }

    /// Drain output produced by a mounted device to a kernel-owned hardware
    /// sink. This is intentionally not a userspace descriptor operation.
    pub fn device_output(&mut self, path: &str, buffer: &mut [u8]) -> Result<usize> {
        let node = self.vfs.resolve(path)?;
        if self.vfs.metadata(node)?.kind != Kind::Device {
            return Err(Error::Unsupported);
        }
        self.vfs.device_output(node, buffer)
    }
}


#[cfg(test)]
mod advisory_lock_tests {
    use super::*;
    use crate::vfs::{Kind, Vfs, memfs::MemFs};

    #[test]
    fn shared_locks_coexist_and_exclusive_conflicts() {
        let mut root = MemFs::<8, 64>::new().unwrap();
        let mut vfs = Vfs::<1>::new(&mut root).unwrap();
        vfs.create("/file", Kind::File).unwrap();
        let mut files = Files::<1, 8, 1, 16>::new(vfs);

        let first = files.open("/file", Open::READ_WRITE).unwrap();
        let second = files.open("/file", Open::READ_WRITE).unwrap();
        let third = files.open("/file", Open::READ_WRITE).unwrap();

        files.lock(first, AdvisoryLock::Shared).unwrap();
        files.lock(second, AdvisoryLock::Shared).unwrap();
        assert_eq!(
            files.lock(third, AdvisoryLock::Exclusive),
            Err(Error::WouldBlock)
        );
        assert_eq!(
            files.lock(first, AdvisoryLock::Exclusive),
            Err(Error::WouldBlock)
        );

        files.unlock(second).unwrap();
        files.unlock(first).unwrap();
        files.lock(first, AdvisoryLock::Exclusive).unwrap();
        assert_eq!(
            files.lock(second, AdvisoryLock::Shared),
            Err(Error::WouldBlock)
        );
    }

    #[test]
    fn duplicate_descriptors_share_lock_until_final_close() {
        let mut root = MemFs::<8, 64>::new().unwrap();
        let mut vfs = Vfs::<1>::new(&mut root).unwrap();
        vfs.create("/file", Kind::File).unwrap();
        let mut files = Files::<1, 8, 1, 16>::new(vfs);

        let owner = files.open("/file", Open::READ_WRITE).unwrap();
        files.lock(owner, AdvisoryLock::Exclusive).unwrap();
        let duplicate = files.dup(owner).unwrap();
        assert_eq!(
            files.advisory_lock(duplicate),
            Ok(Some(AdvisoryLock::Exclusive))
        );

        let contender = files.open("/file", Open::READ_WRITE).unwrap();
        assert_eq!(
            files.lock(contender, AdvisoryLock::Shared),
            Err(Error::WouldBlock)
        );

        files.close(owner).unwrap();
        assert_eq!(
            files.lock(contender, AdvisoryLock::Shared),
            Err(Error::WouldBlock)
        );
        files.close(duplicate).unwrap();
        files.lock(contender, AdvisoryLock::Exclusive).unwrap();
    }

    #[test]
    fn pipes_and_directories_are_not_lockable_files() {
        let mut root = MemFs::<8, 64>::new().unwrap();
        let vfs = Vfs::<1>::new(&mut root).unwrap();
        let mut files = Files::<1, 8, 1, 16>::new(vfs);
        let directory = files.open("/", Open::READ).unwrap();
        assert_eq!(
            files.lock(directory, AdvisoryLock::Shared),
            Err(Error::NotSeekable)
        );
        let (reader, _) = files.pipe().unwrap();
        assert_eq!(
            files.lock(reader, AdvisoryLock::Shared),
            Err(Error::NotSeekable)
        );
    }
}
