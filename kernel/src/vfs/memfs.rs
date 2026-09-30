//! Volatile bootstrap files. Slot reuse increments identities; a wrapped
//! generation retires the slot permanently instead of reviving stale handles.
use super::{Entry, Error, Filesystem, Kind, Metadata, Name, NodeId, Result};

#[derive(Clone, Copy)]
struct Inode<const B: usize> {
    parent: NodeId,
    name: Name,
    kind: Kind,
    len: usize,
    flags: u8,
    data: [u8; B],
}

pub struct MemFs<const N: usize, const B: usize> {
    nodes: [Option<Inode<B>>; N],
    generations: [u32; N],
}

impl<const N: usize, const B: usize> MemFs<N, B> {
    pub const fn new() -> Result<Self> {
        if N == 0 || N > u32::MAX as usize {
            return Err(Error::NoSpace);
        }
        let mut fs = Self {
            nodes: [None; N],
            generations: [1; N],
        };
        let name = match Name::new("root") {
            Ok(name) => name,
            Err(error) => return Err(error),
        };
        fs.nodes[0] = Some(Inode {
            parent: NodeId(1u64 << 32),
            name,
            kind: Kind::Directory,
            len: 0,
            flags: 0,
            data: [0; B],
        });
        Ok(fs)
    }

    fn id(&self, index: usize) -> NodeId {
        NodeId((u64::from(self.generations[index]) << 32) | index as u64)
    }

    fn index(&self, id: NodeId) -> Result<usize> {
        let index = (id.0 & 0xffff_ffff) as usize;
        if index >= N
            || self.generations[index] != (id.0 >> 32) as u32
            || self.nodes[index].is_none()
        {
            return Err(Error::StaleNode);
        }
        Ok(index)
    }

    fn node(&self, id: NodeId) -> Result<&Inode<B>> {
        Ok(self.nodes[self.index(id)?].as_ref().expect("live inode"))
    }

    fn directory(&self, id: NodeId) -> Result<()> {
        if self.node(id)?.kind != Kind::Directory {
            return Err(Error::NotDirectory);
        }
        Ok(())
    }
}

impl<const N: usize, const B: usize> Filesystem for MemFs<N, B> {
    fn root(&self) -> NodeId {
        self.id(0)
    }

    fn metadata(&self, id: NodeId) -> Result<Metadata> {
        let node = self.node(id)?;
        Ok(Metadata {
            kind: node.kind,
            len: node.len,
        })
    }

    fn lookup(&self, dir: NodeId, name: &str) -> Result<NodeId> {
        self.directory(dir)?;
        let name = Name::new(name)?;
        self.nodes
            .iter()
            .enumerate()
            .skip(1)
            .find_map(|(i, node)| {
                node.as_ref()
                    .filter(|node| node.parent == dir && node.name == name)
                    .map(|_| self.id(i))
            })
            .ok_or(Error::NotFound)
    }

    fn entry(&self, dir: NodeId, index: usize) -> Result<Option<Entry>> {
        self.directory(dir)?;
        Ok(self
            .nodes
            .iter()
            .enumerate()
            .skip(1)
            .filter_map(|(i, node)| {
                node.as_ref()
                    .filter(|node| node.parent == dir)
                    .map(|node| Entry {
                        name: node.name,
                        id: self.id(i),
                        kind: node.kind,
                    })
            })
            .nth(index))
    }

    fn create(&mut self, dir: NodeId, name: &str, kind: Kind) -> Result<NodeId> {
        self.directory(dir)?;
        let validated = Name::new(name)?;
        if kind == Kind::Device {
            return Err(Error::Unsupported);
        }
        match self.lookup(dir, name) {
            Ok(_) => return Err(Error::Exists),
            Err(Error::NotFound) => {}
            Err(e) => return Err(e),
        }
        let index = (1..N)
            .find(|&i| self.nodes[i].is_none() && self.generations[i] != 0)
            .ok_or(Error::NoSpace)?;
        self.nodes[index] = Some(Inode {
            parent: dir,
            name: validated,
            kind,
            len: 0,
            flags: 0,
            data: [0; B],
        });
        Ok(self.id(index))
    }

    fn rename(&mut self, from_dir: NodeId, from: &str, to_dir: NodeId, to: &str) -> Result<()> {
        self.directory(from_dir)?;
        self.directory(to_dir)?;
        let new_name = Name::new(to)?;
        let source = self.lookup(from_dir, from)?;
        let source_index = self.index(source)?;
        let source_node = *self.nodes[source_index].as_ref().expect("live source");
        if source_node.flags & (super::FLAG_IMMUTABLE | super::FLAG_APPEND_ONLY) != 0 {
            return Err(Error::AccessDenied);
        }

        if source_node.kind == Kind::Directory {
            let mut cursor = to_dir;
            loop {
                if cursor == source {
                    return Err(Error::InvalidPath);
                }
                if cursor == self.root() {
                    break;
                }
                cursor = self.node(cursor)?.parent;
            }
        }

        let destination = match self.lookup(to_dir, to) {
            Ok(id) if id == source => return Ok(()),
            Ok(id) => Some(id),
            Err(Error::NotFound) => None,
            Err(error) => return Err(error),
        };

        let destination_index = if let Some(id) = destination {
            let index = self.index(id)?;
            let node = self.nodes[index].as_ref().expect("live destination");
            if node.flags & (super::FLAG_IMMUTABLE | super::FLAG_APPEND_ONLY) != 0 {
                return Err(Error::AccessDenied);
            }
            if source_node.kind != node.kind {
                return Err(if source_node.kind == Kind::Directory {
                    Error::NotDirectory
                } else {
                    Error::IsDirectory
                });
            }
            if node.kind == Kind::Directory && self.nodes.iter().flatten().any(|child| child.parent == id) {
                return Err(Error::NotEmpty);
            }
            Some(index)
        } else {
            None
        };

        // Every fallible validation is complete. Exclusive filesystem ownership
        // makes the replacement and source relink one indivisible namespace step.
        if let Some(index) = destination_index {
            self.nodes[index] = None;
            self.generations[index] = self.generations[index].checked_add(1).unwrap_or(0);
        }
        let node = self.nodes[source_index].as_mut().expect("validated live source");
        node.parent = to_dir;
        node.name = new_name;
        Ok(())
    }

    fn remove(&mut self, dir: NodeId, name: &str) -> Result<()> {
        let id = self.lookup(dir, name)?;
        let flags = self.node(id)?.flags;
        if flags & (super::FLAG_IMMUTABLE | super::FLAG_APPEND_ONLY) != 0 {
            return Err(Error::AccessDenied);
        }
        if self.nodes.iter().flatten().any(|node| node.parent == id) {
            return Err(Error::NotEmpty);
        }
        let index = self.index(id)?;
        self.nodes[index] = None;
        self.generations[index] = self.generations[index].checked_add(1).unwrap_or(0);
        Ok(())
    }

    fn read(&mut self, id: NodeId, offset: usize, buffer: &mut [u8]) -> Result<usize> {
        let node = self.node(id)?;
        if node.kind != Kind::File {
            return Err(Error::IsDirectory);
        }
        if offset >= node.len {
            return Ok(0);
        }
        let count = buffer.len().min(node.len - offset);
        buffer[..count].copy_from_slice(&node.data[offset..offset + count]);
        Ok(count)
    }

    fn write(&mut self, id: NodeId, offset: usize, buffer: &[u8]) -> Result<usize> {
        let index = self.index(id)?;
        let node = self.nodes[index].as_mut().expect("live inode");
        if node.kind != Kind::File {
            return Err(Error::IsDirectory);
        }
        if node.flags & super::FLAG_IMMUTABLE != 0 {
            return Err(Error::AccessDenied);
        }
        if node.flags & super::FLAG_APPEND_ONLY != 0 && offset != node.len {
            return Err(Error::AccessDenied);
        }
        if buffer.is_empty() {
            return Ok(0);
        }
        let end = offset
            .checked_add(buffer.len())
            .filter(|&end| end <= B)
            .ok_or(Error::NoSpace)?;
        if offset > node.len {
            node.data[node.len..offset].fill(0);
        }
        node.data[offset..end].copy_from_slice(buffer);
        node.len = node.len.max(end);
        Ok(buffer.len())
    }

    fn truncate(&mut self, id: NodeId) -> Result<()> {
        let index = self.index(id)?;
        let node = self.nodes[index].as_mut().expect("live inode");
        if node.kind != Kind::File {
            return Err(Error::IsDirectory);
        }
        if node.flags & (super::FLAG_IMMUTABLE | super::FLAG_APPEND_ONLY) != 0 {
            return Err(Error::AccessDenied);
        }
        node.data.fill(0);
        node.len = 0;
        Ok(())
    }

    fn file_flags(&self, id: NodeId) -> Result<u8> {
        Ok(self.node(id)?.flags)
    }

    fn set_file_flags(&mut self, id: NodeId, flags: u8) -> Result<()> {
        if flags & !super::FILE_FLAGS_ALL != 0 {
            return Err(Error::Unsupported);
        }
        let index = self.index(id)?;
        let node = self.nodes[index].as_mut().expect("live inode");
        if node.kind != Kind::File {
            return Err(Error::IsDirectory);
        }
        // Once immutable is set it can only be cleared by an explicit flag
        // operation; ordinary mutation paths cannot bypass it.
        node.flags = flags;
        Ok(())
    }
}
