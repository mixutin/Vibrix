//! Checked, exclusively borrowed partition views. No partition discovery or
//! root selection is performed; callers must validate the partition table.
use super::{BlockDevice, Durability, Error, Geometry, Transport, ram_device};

pub struct Partition<'a, T> {
    parent: &'a mut BlockDevice<T>,
    first_lba: u64,
    geometry: Geometry,
}

impl<T: Transport> BlockDevice<T> {
    /// Restrict access to a nonempty subrange of this device. The borrow keeps
    /// the parent inaccessible until the view is released. Read-only policy
    /// and durability cannot be upgraded by creating or nesting a view.
    pub fn partition(
        &mut self,
        first_lba: u64,
        sectors: u64,
    ) -> Result<BlockDevice<Partition<'_, T>>, Error> {
        let geometry = Geometry::new(self.geometry().sector_bytes(), sectors)?;
        let end = first_lba.checked_add(sectors).ok_or(Error::Bounds)?;
        if end > self.geometry().sectors() {
            return Err(Error::Bounds);
        }
        let read_only = self.is_read_only();
        let durability = self.durability();
        Ok(BlockDevice::new(
            Partition {
                parent: self,
                first_lba,
                geometry,
            },
            geometry,
            read_only,
            durability,
        ))
    }
}

impl<T: Transport> Partition<'_, T> {
    fn parent_lba(&self, offset: u64, bytes: usize) -> Result<u64, Error> {
        let sector = u64::from(self.geometry.sector_bytes());
        if !offset.is_multiple_of(sector) {
            return Err(Error::Alignment);
        }
        let lba = offset / sector;
        self.geometry.offset(lba, bytes)?;
        self.first_lba.checked_add(lba).ok_or(Error::Bounds)
    }
}

impl<T: Transport> Transport for Partition<'_, T> {
    fn read(&mut self, offset: u64, output: &mut [u8]) -> Result<(), Error> {
        let lba = self.parent_lba(offset, output.len())?;
        self.parent.read_blocks(lba, output)
    }

    fn write(&mut self, offset: u64, input: &[u8]) -> Result<(), Error> {
        let lba = self.parent_lba(offset, input.len())?;
        self.parent.write_blocks(lba, input)
    }

    fn flush(&mut self) -> Result<(), Error> {
        // A hardware flush is device-wide, not a partition-scoped barrier.
        self.parent.flush()
    }
}

pub(super) fn self_test() -> Result<(), Error> {
    let mut storage = [0x3c; 2048];
    let mut disk = ram_device(&mut storage, 512, false)?;
    {
        let mut view = disk.partition(1, 2)?;
        view.write_blocks(0, &[0xa5; 1024])?;
        let mut output = [0; 1024];
        view.read_blocks(0, &mut output)?;
        if output != [0xa5; 1024]
            || view.write_blocks(2, &[0; 512]) != Err(Error::Bounds)
            || view.durability() != Durability::Volatile
            || view.flush() != Err(Error::UnsupportedFlush)
        {
            return Err(Error::Io);
        }
    }
    let mut output = [0; 512];
    for lba in [0, 3] {
        disk.read_blocks(lba, &mut output)?;
        if output != [0x3c; 512] {
            return Err(Error::Io);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_partition_round_trip() {
        assert_eq!(self_test(), Ok(()));
    }

    #[test]
    fn four_kib_sectors_and_nested_views() {
        let mut bytes = [0; 16384];
        let mut disk = ram_device(&mut bytes, 4096, false).unwrap();
        {
            let mut view = disk.partition(1, 3).unwrap();
            let mut nested = view.partition(1, 1).unwrap();
            assert_eq!(nested.geometry(), Geometry::new(4096, 1).unwrap());
            nested.write_blocks(0, &[0x5a; 4096]).unwrap();
            assert_eq!(nested.read_blocks(1, &mut [0; 4096]), Err(Error::Bounds));
        }
        let mut output = [0; 4096];
        for lba in 0..4 {
            disk.read_blocks(lba, &mut output).unwrap();
            assert_eq!(output, [if lba == 2 { 0x5a } else { 0 }; 4096]);
        }
    }

    #[test]
    fn invalid_partition_geometry_is_rejected() {
        let mut bytes = [0; 2048];
        let mut disk = ram_device(&mut bytes, 512, false).unwrap();
        assert!(matches!(disk.partition(0, 0), Err(Error::Geometry)));
        assert!(matches!(disk.partition(4, 1), Err(Error::Bounds)));
        assert!(matches!(disk.partition(3, 2), Err(Error::Bounds)));
        assert!(matches!(disk.partition(u64::MAX, 1), Err(Error::Bounds)));
        assert!(matches!(disk.partition(0, u64::MAX), Err(Error::Geometry)));
        assert!(disk.partition(0, 4).is_ok());
    }

    struct Forbidden;

    impl Transport for Forbidden {
        fn read(&mut self, _: u64, _: &mut [u8]) -> Result<(), Error> {
            panic!("rejected partition read reached transport")
        }
        fn write(&mut self, _: u64, _: &[u8]) -> Result<(), Error> {
            panic!("rejected partition write reached transport")
        }
        fn flush(&mut self) -> Result<(), Error> {
            panic!("volatile partition flush reached transport")
        }
    }

    #[test]
    fn rejected_io_never_reaches_parent_transport() {
        let mut disk = BlockDevice::new(
            Forbidden,
            Geometry::new(512, 8).unwrap(),
            false,
            Durability::Volatile,
        );
        let mut view = disk.partition(2, 2).unwrap();
        assert_eq!(view.read_blocks(0, &mut []), Err(Error::Empty));
        assert_eq!(view.write_blocks(0, &[0; 513]), Err(Error::Alignment));
        assert_eq!(view.read_blocks(1, &mut [0; 1024]), Err(Error::Bounds));
        assert_eq!(view.write_blocks(u64::MAX, &[0; 512]), Err(Error::Bounds));
        assert_eq!(view.flush(), Err(Error::UnsupportedFlush));
    }

    #[test]
    fn views_preserve_read_only_and_volatile_policy() {
        let mut disk = BlockDevice::new(
            Forbidden,
            Geometry::new(512, 8).unwrap(),
            true,
            Durability::Volatile,
        );
        let mut view = disk.partition(2, 2).unwrap();
        let mut nested = view.partition(1, 1).unwrap();
        assert!(nested.is_read_only());
        assert_eq!(nested.durability(), Durability::Volatile);
        assert_eq!(nested.write_blocks(0, &[0; 512]), Err(Error::ReadOnly));
        assert_eq!(nested.flush(), Err(Error::UnsupportedFlush));
    }

    #[test]
    fn transport_errors_and_device_wide_flush_are_preserved() {
        struct Failing;
        impl Transport for Failing {
            fn read(&mut self, offset: u64, _: &mut [u8]) -> Result<(), Error> {
                assert_eq!(offset, 1024);
                Err(Error::Io)
            }
            fn write(&mut self, offset: u64, _: &[u8]) -> Result<(), Error> {
                assert_eq!(offset, 1024);
                Err(Error::Io)
            }
            fn flush(&mut self) -> Result<(), Error> {
                Err(Error::Io)
            }
        }
        let mut disk = BlockDevice::new(
            Failing,
            Geometry::new(512, 8).unwrap(),
            false,
            Durability::FlushSupported,
        );
        let mut view = disk.partition(2, 2).unwrap();
        assert_eq!(view.durability(), Durability::FlushSupported);
        assert_eq!(view.read_blocks(0, &mut [0; 512]), Err(Error::Io));
        assert_eq!(view.write_blocks(0, &[0; 512]), Err(Error::Io));
        assert_eq!(view.flush(), Err(Error::Io));
    }
}
