//! A descriptor table owns its namespace borrow and all open descriptions.
//! Duplication shares offsets. No external caller can unlink behind an open
//! description because namespace mutation is mediated by this owner.
use super::{Entry, Error, Kind, Metadata, Node, Result, Vfs, pipe::Pipe};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access { Read, Write, ReadWrite }

impl Access {
    fn readable(self) -> bool { matches!(self, Self::Read | Self::ReadWrite) }
    fn writable(self) -> bool { matches!(self, Self::Write | Self::ReadWrite) }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Open {
    pub access: Access,
    pub truncate: bool,
    pub append: bool,
}

impl Open {
    pub const READ: Self = Self { access: Access::Read, truncate: false, append: false };
    pub const READ_WRITE: Self = Self { access: Access::ReadWrite, truncate: false, append: false };
    pub const REPLACE: Self = Self { access: Access::Write, truncate: true, append: false };
}

#[derive(Clone, Copy)]
enum Object { Node(Node), PipeRead(usize), PipeWrite(usize) }

#[derive(Clone, Copy)]
struct Description {
    object: Object,
    offset: usize,
    options: Open,
    references: usize,
}

pub struct Files<'a, const M: usize, const D: usize, const P: usize, const B: usize> {
    vfs: Vfs<'a, M>,
    descriptors: [Option<usize>; D],
    descriptions: [Option<Description>; D],
    pipes: [Option<Pipe<B>>; P],
}

impl<'a, const M: usize, const D: usize, const P: usize, const B: usize> Files<'a, M, D, P, B> {
    pub fn new(vfs: Vfs<'a, M>) -> Self {
        Self { vfs, descriptors: [None; D], descriptions: [None; D], pipes: core::array::from_fn(|_| None) }
    }

    fn description_index(&self, fd: usize) -> Result<usize> {
        self.descriptors.get(fd).copied().flatten().ok_or(Error::BadDescriptor)
    }

    fn description(&self, fd: usize) -> Result<Description> {
        Ok(self.descriptions[self.description_index(fd)?].expect("descriptor owns description"))
    }

    pub fn open(&mut self, path: &str, options: Open) -> Result<usize> {
        if (options.truncate || options.append) && !options.access.writable() { return Err(Error::AccessDenied); }
        // Check both resources before truncate so exhaustion cannot destroy data.
        let fd = self.descriptors.iter().position(Option::is_none).ok_or(Error::NoSpace)?;
        let description = self.descriptions.iter().position(Option::is_none).ok_or(Error::NoSpace)?;
        let node = self.vfs.resolve(path)?;
        let kind = self.vfs.metadata(node)?.kind;
        if kind == Kind::Directory && options.access.writable() { return Err(Error::IsDirectory); }
        if (options.append || options.truncate) && kind != Kind::File { return Err(Error::NotSeekable); }
        if options.truncate { self.vfs.truncate(node)?; }
        self.descriptions[description] = Some(Description { object: Object::Node(node), offset: 0, options, references: 1 });
        self.descriptors[fd] = Some(description);
        Ok(fd)
    }

    pub fn dup(&mut self, fd: usize) -> Result<usize> {
        let index = self.description_index(fd)?;
        let free = self.descriptors.iter().position(Option::is_none).ok_or(Error::NoSpace)?;
        self.descriptions[index].as_mut().expect("live description").references += 1;
        self.descriptors[free] = Some(index);
        Ok(free)
    }

    pub fn close(&mut self, fd: usize) -> Result<()> {
        let index = self.description_index(fd)?;
        self.descriptors[fd] = None;
        let description = self.descriptions[index].as_mut().expect("live description");
        description.references -= 1;
        if description.references != 0 { return Ok(()); }
        let object = description.object;
        self.descriptions[index] = None;
        let pipe_index = match object {
            Object::Node(_) => return Ok(()),
            Object::PipeRead(i) => { self.pipes[i].as_mut().expect("live pipe").reader = false; i },
            Object::PipeWrite(i) => { self.pipes[i].as_mut().expect("live pipe").writer = false; i },
        };
        let pipe = self.pipes[pipe_index].as_ref().expect("live pipe");
        if !pipe.reader && !pipe.writer { self.pipes[pipe_index] = None; }
        Ok(())
    }

    pub fn read(&mut self, fd: usize, buffer: &mut [u8]) -> Result<usize> {
        let description = self.description(fd)?;
        if !description.options.access.readable() { return Err(Error::AccessDenied); }
        match description.object {
            Object::PipeRead(i) => self.pipes[i].as_mut().expect("live pipe").read(buffer),
            Object::PipeWrite(_) => Err(Error::AccessDenied),
            Object::Node(node) => {
                let kind = self.vfs.metadata(node)?.kind;
                if kind == Kind::Directory { return Err(Error::IsDirectory); }
                let limit = if kind == Kind::File { buffer.len().min(usize::MAX - description.offset) } else { buffer.len() };
                let count = self.vfs.read(node, description.offset, &mut buffer[..limit])?;
                if kind == Kind::File {
                    let index = self.description_index(fd)?;
                    self.descriptions[index].as_mut().expect("live description").offset += count;
                }
                Ok(count)
            },
        }
    }

    pub fn write(&mut self, fd: usize, buffer: &[u8]) -> Result<usize> {
        let description = self.description(fd)?;
        if !description.options.access.writable() { return Err(Error::AccessDenied); }
        match description.object {
            Object::PipeWrite(i) => self.pipes[i].as_mut().expect("live pipe").write(buffer),
            Object::PipeRead(_) => Err(Error::AccessDenied),
            Object::Node(node) => {
                let metadata = self.vfs.metadata(node)?;
                if metadata.kind == Kind::Directory { return Err(Error::IsDirectory); }
                let offset = if description.options.append { metadata.len } else { description.offset };
                if metadata.kind == Kind::File { offset.checked_add(buffer.len()).ok_or(Error::InvalidOffset)?; }
                let count = self.vfs.write(node, offset, buffer)?;
                if metadata.kind == Kind::File {
                    let index = self.description_index(fd)?;
                    self.descriptions[index].as_mut().expect("live description").offset = offset + count;
                }
                Ok(count)
            },
        }
    }

    pub fn seek(&mut self, fd: usize, offset: usize) -> Result<()> {
        let index = self.description_index(fd)?;
        let description = self.descriptions[index].expect("live description");
        match description.object {
            Object::Node(node) if self.vfs.metadata(node)?.kind == Kind::File => {},
            _ => return Err(Error::NotSeekable),
        }
        self.descriptions[index].as_mut().expect("live description").offset = offset;
        Ok(())
    }

    pub fn pipe(&mut self) -> Result<(usize, usize)> {
        if B == 0 { return Err(Error::NoSpace); }
        let mut fds = self.descriptors.iter().enumerate().filter(|(_, d)| d.is_none()).map(|(i, _)| i);
        let read_fd = fds.next().ok_or(Error::NoSpace)?;
        let write_fd = fds.next().ok_or(Error::NoSpace)?;
        let mut descriptions = self.descriptions.iter().enumerate().filter(|(_, d)| d.is_none()).map(|(i, _)| i);
        let read_description = descriptions.next().ok_or(Error::NoSpace)?;
        let write_description = descriptions.next().ok_or(Error::NoSpace)?;
        let pipe = self.pipes.iter().position(Option::is_none).ok_or(Error::NoSpace)?;
        // All reservations succeeded before any table mutation.
        self.pipes[pipe] = Some(Pipe::new());
        self.descriptions[read_description] = Some(Description { object: Object::PipeRead(pipe), offset: 0, options: Open::READ, references: 1 });
        self.descriptions[write_description] = Some(Description { object: Object::PipeWrite(pipe), offset: 0, options: Open { access: Access::Write, truncate: false, append: false }, references: 1 });
        self.descriptors[read_fd] = Some(read_description);
        self.descriptors[write_fd] = Some(write_description);
        Ok((read_fd, write_fd))
    }

    pub fn create(&mut self, path: &str) -> Result<()> { self.vfs.create(path, Kind::File).map(|_| ()) }
    pub fn mkdir(&mut self, path: &str) -> Result<()> { self.vfs.create(path, Kind::Directory).map(|_| ()) }

    pub fn remove(&mut self, path: &str) -> Result<()> {
        let node = self.vfs.resolve(path)?;
        if self.descriptions.iter().flatten().any(|d| matches!(d.object, Object::Node(open) if open == node)) { return Err(Error::Busy); }
        self.vfs.remove(path)
    }

    pub fn metadata(&self, path: &str) -> Result<Metadata> { self.vfs.metadata(self.vfs.resolve(path)?) }

    pub fn entry(&self, path: &str, index: usize) -> Result<Option<Entry>> { self.vfs.entry(self.vfs.resolve(path)?, index) }
}
