//! Bounded SCSI transparent command subset for Vibrix block devices.
//!
//! Command builders are allocation-free and endian-explicit. Transport is
//! intentionally separate: USB Mass Storage BOT will carry these CDBs.

pub const MAX_CDB_BYTES: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    ZeroTransfer,
    InvalidResponse,
    CapacityOverflow,
    InvalidBlockLength,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Command {
    pub bytes: [u8; MAX_CDB_BYTES],
    pub length: u8,
}

const fn command(bytes: [u8; MAX_CDB_BYTES], length: u8) -> Command {
    Command { bytes, length }
}

pub const fn test_unit_ready() -> Command {
    let bytes = [0u8; MAX_CDB_BYTES];
    command(bytes, 6)
}

pub const fn inquiry(allocation_length: u8) -> Command {
    let mut bytes = [0u8; MAX_CDB_BYTES];
    bytes[0] = 0x12;
    bytes[4] = allocation_length;
    command(bytes, 6)
}

pub const fn request_sense(allocation_length: u8) -> Command {
    let mut bytes = [0u8; MAX_CDB_BYTES];
    bytes[0] = 0x03;
    bytes[4] = allocation_length;
    command(bytes, 6)
}

pub const fn read_capacity_10() -> Command {
    let mut bytes = [0u8; MAX_CDB_BYTES];
    bytes[0] = 0x25;
    command(bytes, 10)
}

pub const fn read_10(lba: u32, blocks: u16) -> Result<Command, Error> {
    if blocks == 0 {
        return Err(Error::ZeroTransfer);
    }
    let mut bytes = [0u8; MAX_CDB_BYTES];
    bytes[0] = 0x28;
    let lba = lba.to_be_bytes();
    bytes[2] = lba[0];
    bytes[3] = lba[1];
    bytes[4] = lba[2];
    bytes[5] = lba[3];
    let count = blocks.to_be_bytes();
    bytes[7] = count[0];
    bytes[8] = count[1];
    Ok(command(bytes, 10))
}

pub const fn write_10(lba: u32, blocks: u16) -> Result<Command, Error> {
    if blocks == 0 {
        return Err(Error::ZeroTransfer);
    }
    let mut bytes = [0u8; MAX_CDB_BYTES];
    bytes[0] = 0x2a;
    let lba = lba.to_be_bytes();
    bytes[2] = lba[0];
    bytes[3] = lba[1];
    bytes[4] = lba[2];
    bytes[5] = lba[3];
    let count = blocks.to_be_bytes();
    bytes[7] = count[0];
    bytes[8] = count[1];
    Ok(command(bytes, 10))
}

pub const fn synchronize_cache_10() -> Command {
    let mut bytes = [0u8; MAX_CDB_BYTES];
    bytes[0] = 0x35;
    command(bytes, 10)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capacity10 {
    pub last_lba: u32,
    pub block_bytes: u32,
}

impl Capacity10 {
    pub const fn blocks(self) -> Result<u64, Error> {
        match (self.last_lba as u64).checked_add(1) {
            Some(blocks) => Ok(blocks),
            None => Err(Error::CapacityOverflow),
        }
    }

    pub const fn bytes(self) -> Result<u64, Error> {
        let blocks = match self.blocks() {
            Ok(blocks) => blocks,
            Err(error) => return Err(error),
        };
        match blocks.checked_mul(self.block_bytes as u64) {
            Some(bytes) => Ok(bytes),
            None => Err(Error::CapacityOverflow),
        }
    }
}

pub fn parse_read_capacity_10(bytes: &[u8]) -> Result<Capacity10, Error> {
    if bytes.len() != 8 {
        return Err(Error::InvalidResponse);
    }
    let last_lba = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let block_bytes = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    if block_bytes < 512 || !block_bytes.is_power_of_two() {
        return Err(Error::InvalidBlockLength);
    }
    Ok(Capacity10 {
        last_lba,
        block_bytes,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FixedSense {
    pub deferred: bool,
    pub key: u8,
    pub asc: u8,
    pub ascq: u8,
}

pub fn parse_fixed_sense(bytes: &[u8]) -> Result<FixedSense, Error> {
    if bytes.len() < 14 {
        return Err(Error::InvalidResponse);
    }
    let response = bytes[0] & 0x7f;
    if response != 0x70 && response != 0x71 {
        return Err(Error::InvalidResponse);
    }
    Ok(FixedSense {
        deferred: response == 0x71,
        key: bytes[2] & 0x0f,
        asc: bytes[12],
        ascq: bytes[13],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_opcodes_and_big_endian_fields_are_exact() {
        assert_eq!(&test_unit_ready().bytes[..6], &[0, 0, 0, 0, 0, 0]);
        assert_eq!(&inquiry(36).bytes[..6], &[0x12, 0, 0, 0, 36, 0]);
        assert_eq!(&request_sense(18).bytes[..6], &[0x03, 0, 0, 0, 18, 0]);
        assert_eq!(read_capacity_10().bytes[0], 0x25);

        let read = read_10(0x1122_3344, 0x5566).unwrap();
        assert_eq!(
            &read.bytes[..10],
            &[0x28, 0, 0x11, 0x22, 0x33, 0x44, 0, 0x55, 0x66, 0]
        );

        let write = write_10(0xaabb_ccdd, 2).unwrap();
        assert_eq!(
            &write.bytes[..10],
            &[0x2a, 0, 0xaa, 0xbb, 0xcc, 0xdd, 0, 0, 2, 0]
        );
        assert_eq!(synchronize_cache_10().bytes[0], 0x35);

        assert_eq!(read_10(0, 0), Err(Error::ZeroTransfer));
        assert_eq!(write_10(0, 0), Err(Error::ZeroTransfer));
    }

    #[test]
    fn read_capacity_is_big_endian_and_bounded() {
        let capacity = parse_read_capacity_10(&[0, 0, 0x0f, 0xff, 0, 0, 2, 0]).unwrap();
        assert_eq!(capacity.last_lba, 4095);
        assert_eq!(capacity.block_bytes, 512);
        assert_eq!(capacity.blocks(), Ok(4096));
        assert_eq!(capacity.bytes(), Ok(2 * 1024 * 1024));

        assert_eq!(
            parse_read_capacity_10(&[0, 0, 0, 1, 0, 0, 0, 0]),
            Err(Error::InvalidBlockLength)
        );
        assert_eq!(parse_read_capacity_10(&[0; 7]), Err(Error::InvalidResponse));
    }

    #[test]
    fn fixed_sense_extracts_key_asc_and_ascq() {
        let mut sense = [0u8; 18];
        sense[0] = 0x70;
        sense[2] = 0x05;
        sense[12] = 0x20;
        sense[13] = 0x00;
        assert_eq!(
            parse_fixed_sense(&sense),
            Ok(FixedSense {
                deferred: false,
                key: 5,
                asc: 0x20,
                ascq: 0
            })
        );

        sense[0] = 0x72;
        assert_eq!(parse_fixed_sense(&sense), Err(Error::InvalidResponse));
    }
}
