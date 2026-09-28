//! Synchronous, bounded block I/O. No device discovery, DMA or implicit persistence.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Geometry,
    Empty,
    Alignment,
    Bounds,
    ReadOnly,
    UnsupportedFlush,
    Io,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Geometry {
    sector_bytes: u32,
    sectors: u64,
}

impl Geometry {
    pub fn new(sector_bytes: u32, sectors: u64) -> Result<Self, Error> {
        if !(512..=4096).contains(&sector_bytes)
            || !sector_bytes.is_power_of_two()
            || sectors == 0
            || sectors.checked_mul(u64::from(sector_bytes)).is_none()
        {
            return Err(Error::Geometry);
        }
        Ok(Self {
            sector_bytes,
            sectors,
        })
    }

    pub const fn sector_bytes(self) -> u32 {
        self.sector_bytes
    }

    pub const fn sectors(self) -> u64 {
        self.sectors
    }

    fn offset(self, lba: u64, bytes: usize) -> Result<u64, Error> {
        if bytes == 0 {
            return Err(Error::Empty);
        }
        let bytes = u64::try_from(bytes).map_err(|_| Error::Bounds)?;
        let sector = u64::from(self.sector_bytes);
        if !bytes.is_multiple_of(sector) {
            return Err(Error::Alignment);
        }
        let end = lba.checked_add(bytes / sector).ok_or(Error::Bounds)?;
        if end > self.sectors {
            return Err(Error::Bounds);
        }
        lba.checked_mul(sector).ok_or(Error::Bounds)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Durability {
    /// RAM or a device with no established power-loss durability contract.
    Volatile,
    /// The transport implements a real ordered durability barrier.
    FlushSupported,
}

/// Implementations complete synchronously and never retain borrowed buffers.
/// Offsets and lengths supplied by BlockDevice are sector aligned and bounded.
/// An I/O error may leave partial data: callers must not treat it as atomic.
/// A successful flush must make preceding writes power-loss durable in order.
pub trait Transport {
    fn read(&mut self, offset: u64, output: &mut [u8]) -> Result<(), Error>;
    fn write(&mut self, offset: u64, input: &[u8]) -> Result<(), Error>;
    fn flush(&mut self) -> Result<(), Error>;
}

/// Owns the transport and immutable, validated media geometry. Creation does
/// not identify a boot/root device or authorize writes to a physical disk.
pub struct BlockDevice<T> {
    transport: T,
    geometry: Geometry,
    read_only: bool,
    durability: Durability,
}

impl<T: Transport> BlockDevice<T> {
    pub const fn new(
        transport: T,
        geometry: Geometry,
        read_only: bool,
        durability: Durability,
    ) -> Self {
        Self {
            transport,
            geometry,
            read_only,
            durability,
        }
    }

    pub const fn geometry(&self) -> Geometry {
        self.geometry
    }

    pub const fn durability(&self) -> Durability {
        self.durability
    }

    pub const fn is_read_only(&self) -> bool {
        self.read_only
    }

    pub fn read_blocks(&mut self, lba: u64, output: &mut [u8]) -> Result<(), Error> {
        let offset = self.geometry.offset(lba, output.len())?;
        self.transport.read(offset, output)
    }

    pub fn write_blocks(&mut self, lba: u64, input: &[u8]) -> Result<(), Error> {
        if self.read_only {
            return Err(Error::ReadOnly);
        }
        let offset = self.geometry.offset(lba, input.len())?;
        self.transport.write(offset, input)
    }

    pub fn flush(&mut self) -> Result<(), Error> {
        if self.durability != Durability::FlushSupported {
            return Err(Error::UnsupportedFlush);
        }
        self.transport.flush()
    }
}

/// Exclusive borrowed RAM backing; dropping the device releases that borrow.
pub struct Ram<'a> {
    bytes: &'a mut [u8],
}

fn range(offset: u64, length: usize) -> Result<core::ops::Range<usize>, Error> {
    let start = usize::try_from(offset).map_err(|_| Error::Bounds)?;
    let end = start.checked_add(length).ok_or(Error::Bounds)?;
    Ok(start..end)
}

impl Transport for Ram<'_> {
    fn read(&mut self, offset: u64, output: &mut [u8]) -> Result<(), Error> {
        let source = self
            .bytes
            .get(range(offset, output.len())?)
            .ok_or(Error::Bounds)?;
        output.copy_from_slice(source);
        Ok(())
    }

    fn write(&mut self, offset: u64, input: &[u8]) -> Result<(), Error> {
        let destination = self
            .bytes
            .get_mut(range(offset, input.len())?)
            .ok_or(Error::Bounds)?;
        destination.copy_from_slice(input);
        Ok(())
    }

    fn flush(&mut self) -> Result<(), Error> {
        Err(Error::UnsupportedFlush)
    }
}

pub fn ram_device(
    bytes: &mut [u8],
    sector_bytes: u32,
    read_only: bool,
) -> Result<BlockDevice<Ram<'_>>, Error> {
    let sector = usize::try_from(sector_bytes).map_err(|_| Error::Geometry)?;
    if sector == 0 || !bytes.len().is_multiple_of(sector) {
        return Err(Error::Geometry);
    }
    let sectors = u64::try_from(bytes.len() / sector).map_err(|_| Error::Geometry)?;
    let geometry = Geometry::new(sector_bytes, sectors)?;
    Ok(BlockDevice::new(
        Ram { bytes },
        geometry,
        read_only,
        Durability::Volatile,
    ))
}

/// Runs the same production interface on host and in the QEMU kernel probe.
pub fn self_test() -> Result<(), Error> {
    let mut storage = [0u8; 1024];
    let mut output = [0u8; 512];
    let mut disk = ram_device(&mut storage, 512, false)?;
    disk.write_blocks(1, &[0xa5; 512])?;
    disk.read_blocks(1, &mut output)?;
    if output != [0xa5; 512]
        || disk.write_blocks(2, &[0; 512]) != Err(Error::Bounds)
        || disk.flush() != Err(Error::UnsupportedFlush)
    {
        return Err(Error::Io);
    }
    disk.read_blocks(0, &mut output)?;
    if output != [0; 512] {
        return Err(Error::Io);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_round_trip() {
        assert_eq!(self_test(), Ok(()));
    }

    #[test]
    fn geometry_and_integer_bounds() {
        for sector in [0, 1, 511, 513, 8192] {
            assert_eq!(Geometry::new(sector, 2), Err(Error::Geometry));
        }
        assert_eq!(Geometry::new(512, 0), Err(Error::Geometry));
        assert_eq!(Geometry::new(4096, u64::MAX), Err(Error::Geometry));
        let geometry = Geometry::new(4096, 2).unwrap();
        assert_eq!(geometry.offset(1, 4096), Ok(4096));
        assert_eq!(geometry.offset(2, 4096), Err(Error::Bounds));
        assert_eq!(geometry.offset(u64::MAX, 4096), Err(Error::Bounds));
        assert_eq!(geometry.offset(0, 1), Err(Error::Alignment));
        assert_eq!(geometry.offset(0, 0), Err(Error::Empty));
    }

    #[test]
    fn four_kib_sectors_and_read_only() {
        let mut backing = [0x3c; 8192];
        let mut output = [0u8; 4096];
        let mut disk = ram_device(&mut backing, 4096, true).unwrap();
        assert!(disk.is_read_only());
        assert_eq!(disk.geometry().sectors(), 2);
        assert_eq!(disk.geometry().sector_bytes(), 4096);
        assert_eq!(disk.durability(), Durability::Volatile);
        assert_eq!(disk.write_blocks(0, &[0; 4096]), Err(Error::ReadOnly));
        disk.read_blocks(1, &mut output).unwrap();
        assert_eq!(output, [0x3c; 4096]);
    }

    #[test]
    fn invalid_requests_do_not_reach_transport() {
        struct Forbidden;
        impl Transport for Forbidden {
            fn read(&mut self, _: u64, _: &mut [u8]) -> Result<(), Error> {
                panic!("invalid read reached transport")
            }
            fn write(&mut self, _: u64, _: &[u8]) -> Result<(), Error> {
                panic!("invalid write reached transport")
            }
            fn flush(&mut self) -> Result<(), Error> {
                panic!("volatile flush reached transport")
            }
        }
        let mut disk = BlockDevice::new(
            Forbidden,
            Geometry::new(512, 1).unwrap(),
            false,
            Durability::Volatile,
        );
        assert_eq!(disk.read_blocks(0, &mut []), Err(Error::Empty));
        assert_eq!(disk.read_blocks(1, &mut [0; 512]), Err(Error::Bounds));
        assert_eq!(disk.write_blocks(0, &[0; 513]), Err(Error::Alignment));
        assert_eq!(disk.flush(), Err(Error::UnsupportedFlush));
    }

    #[test]
    fn flush_and_io_failures_are_not_hidden() {
        struct Failing;
        impl Transport for Failing {
            fn read(&mut self, _: u64, _: &mut [u8]) -> Result<(), Error> {
                Err(Error::Io)
            }
            fn write(&mut self, _: u64, _: &[u8]) -> Result<(), Error> {
                Err(Error::Io)
            }
            fn flush(&mut self) -> Result<(), Error> {
                Err(Error::Io)
            }
        }
        let mut disk = BlockDevice::new(
            Failing,
            Geometry::new(512, 1).unwrap(),
            false,
            Durability::FlushSupported,
        );
        assert_eq!(disk.read_blocks(0, &mut [0; 512]), Err(Error::Io));
        assert_eq!(disk.write_blocks(0, &[0; 512]), Err(Error::Io));
        assert_eq!(disk.flush(), Err(Error::Io));
    }
}
