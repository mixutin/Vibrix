//! Allocation-free bootstrap namespace. All mutation requires one exclusive
//! owner; no interrupt handler or userspace pointer may enter this API.
pub mod console;
pub mod devfs;
pub mod files;
pub mod memfs;
mod pipe;
mod proof;
#[cfg(test)]
mod tests;
pub mod vibrixfs;

pub use proof::self_test;

pub const NAME_MAX: usize = 31;
pub const PATH_MAX: usize = 255;
pub const DEPTH_MAX: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidPath,
    NameTooLong,
    NotFound,
    Exists,
    NotDirectory,
    IsDirectory,
    NotEmpty,
    NoSpace,
    StaleNode,
    ReadOnly,
    BadDescriptor,
    AccessDenied,
    NotSeekable,
    InvalidOffset,
    WouldBlock,
    BrokenPipe,
    Busy,
    Unsupported,
    BackendContract,
}

pub type Result<T> = core::result::Result<T, Error>;

/// Filesystem-local identity, with generation supplied by the backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    File,
    Directory,
    Device,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Metadata {
    pub kind: Kind,
    pub len: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Name {
    bytes: [u8; NAME_MAX],
    len: usize,
}

impl Name {
    pub const fn new(name: &str) -> Result<Self> {
        let bytes = name.as_bytes();
        if bytes.is_empty()
            || (bytes.len() == 1 && bytes[0] == b'.')
            || (bytes.len() == 2 && bytes[0] == b'.' && bytes[1] == b'.')
        {
            return Err(Error::InvalidPath);
        }
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] == b'/' || bytes[index] == 0 {
                return Err(Error::InvalidPath);
            }
            index += 1;
        }
        if name.len() > NAME_MAX {
            return Err(Error::NameTooLong);
        }
        let mut result = Self {
            bytes: [0; NAME_MAX],
            len: name.len(),
        };
        index = 0;
        while index < bytes.len() {
            result.bytes[index] = bytes[index];
            index += 1;
        }
        Ok(result)
    }

    pub fn as_str(&self) -> &str {
        // Only valid UTF-8 can construct a Name; its bytes are private.
        core::str::from_utf8(&self.bytes[..self.len]).expect("validated name")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: Name,
    pub id: NodeId,
    pub kind: Kind,
}

/// Internal kernel contract, not a userspace ABI. Backends return at most the
/// supplied buffer length and must reject failed writes before mutating data.
pub trait Filesystem {
    fn root(&self) -> NodeId;
    fn metadata(&self, id: NodeId) -> Result<Metadata>;
    fn lookup(&self, dir: NodeId, name: &str) -> Result<NodeId>;
    fn entry(&self, dir: NodeId, index: usize) -> Result<Option<Entry>>;
    fn create(&mut self, dir: NodeId, name: &str, kind: Kind) -> Result<NodeId>;
    fn remove(&mut self, dir: NodeId, name: &str) -> Result<()>;
    fn read(&mut self, id: NodeId, offset: usize, buffer: &mut [u8]) -> Result<usize>;
    fn write(&mut self, id: NodeId, offset: usize, buffer: &[u8]) -> Result<usize>;
    fn truncate(&mut self, id: NodeId) -> Result<()>;

    /// Inject one byte from a kernel-owned device driver into a device node.
    /// Regular filesystems reject this by default.
    fn device_input(&mut self, _id: NodeId, _byte: u8) -> Result<()> {
        Err(Error::Unsupported)
    }

    /// Drain device-produced output for a kernel-owned hardware sink.
    /// Regular filesystems reject this by default.
    fn device_output(&mut self, _id: NodeId, _buffer: &mut [u8]) -> Result<usize> {
        Err(Error::Unsupported)
    }
}

/// Mount IDs are private and mount slots are never recycled while this VFS
/// exists. Node IDs carry the filesystem's own generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Node {
    mount: usize,
    id: NodeId,
}

struct Mount<'a> {
    covered: Option<Node>,
    fs: &'a mut dyn Filesystem,
}

pub struct Vfs<'a, const M: usize> {
    mounts: [Option<Mount<'a>>; M],
}

impl<'a, const M: usize> Vfs<'a, M> {
    pub fn new(root: &'a mut dyn Filesystem) -> Result<Self> {
        if M == 0 {
            return Err(Error::NoSpace);
        }
        if root.metadata(root.root())?.kind != Kind::Directory {
            return Err(Error::NotDirectory);
        }
        let mut mounts = core::array::from_fn(|_| None);
        mounts[0] = Some(Mount {
            covered: None,
            fs: root,
        });
        Ok(Self { mounts })
    }

    fn fs(&self, node: Node) -> Result<&dyn Filesystem> {
        self.mounts
            .get(node.mount)
            .and_then(Option::as_ref)
            .map(|mount| &*mount.fs)
            .ok_or(Error::StaleNode)
    }

    fn fs_mut(&mut self, node: Node) -> Result<&mut (dyn Filesystem + 'a)> {
        match self.mounts.get_mut(node.mount).and_then(Option::as_mut) {
            Some(mount) => Ok(&mut *mount.fs),
            None => Err(Error::StaleNode),
        }
    }

    fn root(&self) -> Node {
        Node {
            mount: 0,
            id: self.mounts[0].as_ref().expect("root mount").fs.root(),
        }
    }

    fn cross_mount(&self, node: Node) -> Node {
        for (index, mount) in self.mounts.iter().enumerate() {
            if let Some(mount) = mount
                && mount.covered == Some(node)
            {
                return Node {
                    mount: index,
                    id: mount.fs.root(),
                };
            }
        }
        node
    }

    pub fn mount(&mut self, path: &str, fs: &'a mut dyn Filesystem) -> Result<()> {
        let covered = self.resolve(path)?;
        if self.metadata(covered)?.kind != Kind::Directory
            || fs.metadata(fs.root())?.kind != Kind::Directory
        {
            return Err(Error::NotDirectory);
        }
        // Do not allow covering a mount root, including namespace root.
        if self.fs(covered)?.root() == covered.id {
            return Err(Error::Busy);
        }
        let slot = self
            .mounts
            .iter()
            .position(Option::is_none)
            .ok_or(Error::NoSpace)?;
        self.mounts[slot] = Some(Mount {
            covered: Some(covered),
            fs,
        });
        Ok(())
    }

    pub fn resolve(&self, path: &str) -> Result<Node> {
        self.walk(path).map(|(node, _)| node)
    }

    fn walk(&self, path: &str) -> Result<(Node, usize)> {
        validate_path(path)?;
        let mut ancestors = [self.root(); DEPTH_MAX + 1];
        let mut depth = 0;
        for component in path.split('/').filter(|part| !part.is_empty()) {
            let current = ancestors[depth];
            if self.metadata(current)?.kind != Kind::Directory {
                return Err(Error::NotDirectory);
            }
            match component {
                "." => {}
                ".." => depth = depth.saturating_sub(1),
                name => {
                    Name::new(name)?;
                    if depth == DEPTH_MAX {
                        return Err(Error::NameTooLong);
                    }
                    let child = self.fs(current)?.lookup(current.id, name)?;
                    depth += 1;
                    ancestors[depth] = self.cross_mount(Node {
                        mount: current.mount,
                        id: child,
                    });
                }
            }
        }
        let node = ancestors[depth];
        if path.ends_with('/') && self.metadata(node)?.kind != Kind::Directory {
            return Err(Error::NotDirectory);
        }
        Ok((node, depth))
    }

    fn parent<'p>(&self, path: &'p str) -> Result<(Node, &'p str, usize)> {
        validate_path(path)?;
        let path = path.trim_end_matches('/');
        let (prefix, name) = path.rsplit_once('/').ok_or(Error::InvalidPath)?;
        Name::new(name)?;
        let (parent, depth) = self.walk(if prefix.is_empty() { "/" } else { prefix })?;
        if self.metadata(parent)?.kind != Kind::Directory {
            return Err(Error::NotDirectory);
        }
        Ok((parent, name, depth))
    }

    pub fn create(&mut self, path: &str, kind: Kind) -> Result<Node> {
        if path.ends_with('/') && kind != Kind::Directory {
            return Err(Error::NotDirectory);
        }
        let (parent, name, depth) = self.parent(path)?;
        if depth == DEPTH_MAX {
            return Err(Error::NameTooLong);
        }
        let id = self.fs_mut(parent)?.create(parent.id, name, kind)?;
        Ok(Node {
            mount: parent.mount,
            id,
        })
    }

    pub fn remove(&mut self, path: &str) -> Result<()> {
        let resolved = self.resolve(path)?;
        if resolved == self.root() {
            return Err(Error::Busy);
        }
        let (parent, name, _) = self.parent(path)?;
        let id = self.fs(parent)?.lookup(parent.id, name)?;
        let covered = Node {
            mount: parent.mount,
            id,
        };
        if self
            .mounts
            .iter()
            .flatten()
            .any(|mount| mount.covered == Some(covered))
        {
            return Err(Error::Busy);
        }
        self.fs_mut(parent)?.remove(parent.id, name)
    }

    pub fn metadata(&self, node: Node) -> Result<Metadata> {
        self.fs(node)?.metadata(node.id)
    }

    pub fn entry(&self, node: Node, index: usize) -> Result<Option<Entry>> {
        self.fs(node)?.entry(node.id, index)
    }

    pub fn read(&mut self, node: Node, offset: usize, buffer: &mut [u8]) -> Result<usize> {
        let limit = buffer.len();
        let count = self.fs_mut(node)?.read(node.id, offset, buffer)?;
        if count > limit {
            return Err(Error::BackendContract);
        }
        Ok(count)
    }

    pub fn write(&mut self, node: Node, offset: usize, buffer: &[u8]) -> Result<usize> {
        let count = self.fs_mut(node)?.write(node.id, offset, buffer)?;
        if count > buffer.len() {
            return Err(Error::BackendContract);
        }
        Ok(count)
    }

    pub fn truncate(&mut self, node: Node) -> Result<()> {
        self.fs_mut(node)?.truncate(node.id)
    }

    pub fn device_input(&mut self, node: Node, byte: u8) -> Result<()> {
        self.fs_mut(node)?.device_input(node.id, byte)
    }

    pub fn device_output(&mut self, node: Node, buffer: &mut [u8]) -> Result<usize> {
        let limit = buffer.len();
        let count = self.fs_mut(node)?.device_output(node.id, buffer)?;
        if count > limit {
            return Err(Error::BackendContract);
        }
        Ok(count)
    }
}

fn validate_path(path: &str) -> Result<()> {
    if !path.starts_with('/') || path.contains('\0') {
        return Err(Error::InvalidPath);
    }
    if path.len() > PATH_MAX {
        return Err(Error::NameTooLong);
    }
    Ok(())
}
