//! A fixed device namespace. These devices have no hardware side effects.
use super::{Entry, Error, Filesystem, Kind, Metadata, Name, NodeId, Result};

pub struct DevFs;

impl Filesystem for DevFs {
    fn root(&self) -> NodeId {
        NodeId(0)
    }
    fn metadata(&self, id: NodeId) -> Result<Metadata> {
        let kind = match id.0 {
            0 => Kind::Directory,
            1 | 2 => Kind::Device,
            _ => return Err(Error::StaleNode),
        };
        Ok(Metadata { kind, len: 0 })
    }
    fn lookup(&self, dir: NodeId, name: &str) -> Result<NodeId> {
        if self.metadata(dir)?.kind != Kind::Directory {
            return Err(Error::NotDirectory);
        }
        Name::new(name)?;
        match name {
            "null" => Ok(NodeId(1)),
            "zero" => Ok(NodeId(2)),
            _ => Err(Error::NotFound),
        }
    }
    fn entry(&self, dir: NodeId, index: usize) -> Result<Option<Entry>> {
        if self.metadata(dir)?.kind != Kind::Directory {
            return Err(Error::NotDirectory);
        }
        let (name, id) = match index {
            0 => ("null", NodeId(1)),
            1 => ("zero", NodeId(2)),
            _ => return Ok(None),
        };
        Ok(Some(Entry {
            name: Name::new(name)?,
            id,
            kind: Kind::Device,
        }))
    }
    fn create(&mut self, _dir: NodeId, _name: &str, _kind: Kind) -> Result<NodeId> {
        Err(Error::ReadOnly)
    }
    fn remove(&mut self, _dir: NodeId, _name: &str) -> Result<()> {
        Err(Error::ReadOnly)
    }
    fn read(&mut self, id: NodeId, _offset: usize, buffer: &mut [u8]) -> Result<usize> {
        match id.0 {
            0 => Err(Error::IsDirectory),
            1 => Ok(0),
            2 => {
                buffer.fill(0);
                Ok(buffer.len())
            }
            _ => Err(Error::StaleNode),
        }
    }
    fn write(&mut self, id: NodeId, _offset: usize, buffer: &[u8]) -> Result<usize> {
        match id.0 {
            0 => Err(Error::IsDirectory),
            1 | 2 => Ok(buffer.len()),
            _ => Err(Error::StaleNode),
        }
    }
    fn truncate(&mut self, _id: NodeId) -> Result<()> {
        Err(Error::NotSeekable)
    }
}
