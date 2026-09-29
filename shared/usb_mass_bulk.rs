//! USB Mass Storage Class Bulk-Only Transport wire contract.
//!
//! Pure byte encoding/validation shared by host tests and the future native
//! xHCI bulk endpoint driver. This module performs no USB I/O.

pub const CBW_BYTES: usize = 31;
pub const CSW_BYTES: usize = 13;
pub const CBW_SIGNATURE: u32 = 0x4342_5355;
pub const CSW_SIGNATURE: u32 = 0x5342_5355;
pub const MAX_CDB_BYTES: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Out,
    In,
}

impl Direction {
    const fn flag(self) -> u8 {
        match self {
            Self::Out => 0x00,
            Self::In => 0x80,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidLun,
    InvalidCommandLength,
    InvalidSignature,
    InvalidTag,
    InvalidResidue,
    InvalidStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandBlockWrapper {
    pub tag: u32,
    pub transfer_length: u32,
    pub direction: Direction,
    pub lun: u8,
    pub command_length: u8,
    pub command: [u8; MAX_CDB_BYTES],
}

impl CommandBlockWrapper {
    pub const fn new(
        tag: u32,
        transfer_length: u32,
        direction: Direction,
        lun: u8,
        command: [u8; MAX_CDB_BYTES],
        command_length: u8,
    ) -> Result<Self, Error> {
        if lun > 0x0f {
            return Err(Error::InvalidLun);
        }
        if command_length == 0 || command_length as usize > MAX_CDB_BYTES {
            return Err(Error::InvalidCommandLength);
        }
        Ok(Self {
            tag,
            transfer_length,
            direction,
            lun,
            command_length,
            command,
        })
    }

    pub fn encode(self) -> [u8; CBW_BYTES] {
        let mut bytes = [0u8; CBW_BYTES];
        bytes[0..4].copy_from_slice(&CBW_SIGNATURE.to_le_bytes());
        bytes[4..8].copy_from_slice(&self.tag.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.transfer_length.to_le_bytes());
        bytes[12] = self.direction.flag();
        bytes[13] = self.lun;
        bytes[14] = self.command_length;
        bytes[15..31].copy_from_slice(&self.command);
        bytes
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandStatus {
    Passed,
    Failed,
    PhaseError,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandStatusWrapper {
    pub tag: u32,
    pub residue: u32,
    pub status: CommandStatus,
}

pub fn parse_csw(
    bytes: &[u8],
    expected_tag: u32,
    expected_transfer_length: u32,
) -> Result<CommandStatusWrapper, Error> {
    if bytes.len() != CSW_BYTES {
        return Err(Error::InvalidSignature);
    }
    let signature = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    if signature != CSW_SIGNATURE {
        return Err(Error::InvalidSignature);
    }
    let tag = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    if tag != expected_tag {
        return Err(Error::InvalidTag);
    }
    let residue = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
    if residue > expected_transfer_length {
        return Err(Error::InvalidResidue);
    }
    let status = match bytes[12] {
        0 => CommandStatus::Passed,
        1 => CommandStatus::Failed,
        2 => CommandStatus::PhaseError,
        _ => return Err(Error::InvalidStatus),
    };
    Ok(CommandStatusWrapper {
        tag,
        residue,
        status,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cbw_matches_bulk_only_wire_layout() {
        let mut cdb = [0u8; MAX_CDB_BYTES];
        cdb[..6].copy_from_slice(&[0x12, 0, 0, 0, 36, 0]);
        let cbw = CommandBlockWrapper::new(0x1122_3344, 36, Direction::In, 0, cdb, 6)
            .unwrap()
            .encode();
        assert_eq!(cbw.len(), 31);
        assert_eq!(&cbw[0..4], &[0x55, 0x53, 0x42, 0x43]);
        assert_eq!(&cbw[4..8], &[0x44, 0x33, 0x22, 0x11]);
        assert_eq!(&cbw[8..12], &[36, 0, 0, 0]);
        assert_eq!(cbw[12], 0x80);
        assert_eq!(cbw[13], 0);
        assert_eq!(cbw[14], 6);
        assert_eq!(&cbw[15..21], &[0x12, 0, 0, 0, 36, 0]);
    }

    #[test]
    fn cbw_rejects_reserved_lun_and_cdb_lengths() {
        assert_eq!(
            CommandBlockWrapper::new(1, 0, Direction::Out, 16, [0; 16], 6),
            Err(Error::InvalidLun)
        );
        assert_eq!(
            CommandBlockWrapper::new(1, 0, Direction::Out, 0, [0; 16], 0),
            Err(Error::InvalidCommandLength)
        );
        assert_eq!(
            CommandBlockWrapper::new(1, 0, Direction::Out, 0, [0; 16], 17),
            Err(Error::InvalidCommandLength)
        );
    }

    #[test]
    fn csw_validates_signature_tag_residue_and_status() {
        let mut bytes = [0u8; CSW_BYTES];
        bytes[0..4].copy_from_slice(&CSW_SIGNATURE.to_le_bytes());
        bytes[4..8].copy_from_slice(&7u32.to_le_bytes());
        bytes[8..12].copy_from_slice(&4u32.to_le_bytes());
        bytes[12] = 0;

        assert_eq!(
            parse_csw(&bytes, 7, 8),
            Ok(CommandStatusWrapper {
                tag: 7,
                residue: 4,
                status: CommandStatus::Passed
            })
        );

        let mut wrong = bytes;
        wrong[0] = 0;
        assert_eq!(parse_csw(&wrong, 7, 8), Err(Error::InvalidSignature));

        let mut wrong = bytes;
        wrong[4] = 8;
        assert_eq!(parse_csw(&wrong, 7, 8), Err(Error::InvalidTag));

        let mut wrong = bytes;
        wrong[8..12].copy_from_slice(&9u32.to_le_bytes());
        assert_eq!(parse_csw(&wrong, 7, 8), Err(Error::InvalidResidue));

        let mut wrong = bytes;
        wrong[12] = 3;
        assert_eq!(parse_csw(&wrong, 7, 8), Err(Error::InvalidStatus));
    }
}
