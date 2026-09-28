//! Inventory of a caller-reserved RAM supply, not a second firmware allocator.
//! Recycling changes numeric ownership only. The mapper must detach mappings
//! and invalidate translations before returning frames to this inventory.
use super::Error;
use super::address::PhysicalFrame;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameUse {
    Table,
    Data,
}

pub struct Frames<const N: usize> {
    bits: u8,
    physical: [u64; N],
    usage: [Option<FrameUse>; N],
    count: usize,
}

impl<const N: usize> Frames<N> {
    pub fn new(physical_bits: u8) -> Result<Self, Error> {
        PhysicalFrame::new(4096, physical_bits)?;
        Ok(Self {
            bits: physical_bits,
            physical: [0; N],
            usage: [None; N],
            count: 0,
        })
    }

    /// Register a physical number only after the caller has reserved its RAM.
    /// This pure inventory never dereferences or independently claims memory.
    pub fn register(&mut self, physical: u64) -> Result<(), Error> {
        PhysicalFrame::new(physical, self.bits)?;
        if self.contains(physical) {
            return Err(Error::DuplicateFrame);
        }
        if self.count == N {
            return Err(Error::Capacity);
        }
        self.physical[self.count] = physical;
        self.count += 1;
        Ok(())
    }

    pub fn acquire(&mut self, usage: FrameUse) -> Result<u64, Error> {
        let index = self.usage[..self.count]
            .iter()
            .position(Option::is_none)
            .ok_or(Error::OutOfFrames)?;
        self.usage[index] = Some(usage);
        Ok(self.physical[index])
    }

    /// All hardware/Rust users must already have retired this frame.
    pub fn release(&mut self, physical: u64, expected: FrameUse) -> Result<(), Error> {
        let index = self.physical[..self.count]
            .iter()
            .position(|address| *address == physical)
            .ok_or(Error::UnknownFrame)?;
        match self.usage[index] {
            None => Err(Error::DoubleFree),
            Some(actual) if actual != expected => Err(Error::WrongFrameUse),
            Some(_) => {
                self.usage[index] = None;
                Ok(())
            }
        }
    }

    pub fn contains(&self, physical: u64) -> bool {
        self.physical[..self.count].contains(&physical)
    }

    pub fn kind(&self, physical: u64) -> Option<FrameUse> {
        self.physical[..self.count]
            .iter()
            .position(|address| *address == physical)
            .and_then(|index| self.usage[index])
    }

    pub fn registered(&self) -> &[u64] {
        &self.physical[..self.count]
    }

    pub const fn physical_bits(&self) -> u8 {
        self.bits
    }

    pub const fn total(&self) -> usize {
        self.count
    }

    pub fn available(&self) -> usize {
        self.usage[..self.count]
            .iter()
            .filter(|usage| usage.is_none())
            .count()
    }

    pub fn in_use(&self, kind: FrameUse) -> usize {
        self.usage[..self.count]
            .iter()
            .filter(|usage| **usage == Some(kind))
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inventory_rejects_invalid_duplicate_and_excess_frames() {
        let mut frames = Frames::<2>::new(36).unwrap();
        assert_eq!(frames.register(0), Err(Error::InvalidFrame));
        assert_eq!(frames.register(4097), Err(Error::InvalidFrame));
        assert_eq!(frames.register(1 << 36), Err(Error::InvalidFrame));
        frames.register(4096).unwrap();
        assert_eq!(frames.register(4096), Err(Error::DuplicateFrame));
        frames.register(8192).unwrap();
        assert_eq!(frames.register(12288), Err(Error::Capacity));
        assert_eq!(frames.registered(), &[4096, 8192]);
        assert_eq!(frames.total(), 2);
        assert_eq!(frames.available(), 2);
    }

    #[test]
    fn table_and_data_ownership_are_distinct_and_recyclable() {
        let mut frames = Frames::<2>::new(48).unwrap();
        frames.register(4096).unwrap();
        frames.register(8192).unwrap();
        let table = frames.acquire(FrameUse::Table).unwrap();
        let data = frames.acquire(FrameUse::Data).unwrap();
        assert_ne!(table, data);
        assert_eq!(frames.in_use(FrameUse::Table), 1);
        assert_eq!(frames.in_use(FrameUse::Data), 1);
        assert_eq!(frames.available(), 0);
        assert_eq!(frames.acquire(FrameUse::Data), Err(Error::OutOfFrames));
        assert_eq!(
            frames.release(table, FrameUse::Data),
            Err(Error::WrongFrameUse)
        );
        assert_eq!(frames.kind(table), Some(FrameUse::Table));
        frames.release(table, FrameUse::Table).unwrap();
        assert_eq!(
            frames.release(table, FrameUse::Table),
            Err(Error::DoubleFree)
        );
        assert_eq!(frames.acquire(FrameUse::Data), Ok(table));
        assert_eq!(frames.in_use(FrameUse::Data), 2);
        assert_eq!(
            frames.release(12288, FrameUse::Data),
            Err(Error::UnknownFrame)
        );
    }

    #[test]
    fn empty_pool_and_repeated_reuse_do_not_invent_capacity() {
        let mut empty = Frames::<0>::new(48).unwrap();
        assert_eq!(empty.register(4096), Err(Error::Capacity));
        assert_eq!(empty.acquire(FrameUse::Data), Err(Error::OutOfFrames));
        let mut frames = Frames::<1>::new(48).unwrap();
        frames.register(4096).unwrap();
        for _ in 0..1000 {
            assert_eq!(frames.acquire(FrameUse::Data), Ok(4096));
            frames.release(4096, FrameUse::Data).unwrap();
        }
        assert_eq!(frames.total(), 1);
        assert_eq!(frames.available(), 1);
        assert_eq!(frames.physical_bits(), 48);
    }
}
