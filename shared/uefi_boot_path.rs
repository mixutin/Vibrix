//! Bounded parsing of the UEFI device path for the loaded Vibrix image.
//!
//! The parser copies only firmware-neutral values. It never retains firmware
//! pointers or treats topology as persistent device identity.

pub const DEVICE_PATH_END_TYPE: u8 = 0x7f;
pub const DEVICE_PATH_END_ENTIRE_SUBTYPE: u8 = 0xff;
pub const MESSAGING_TYPE: u8 = 0x03;
pub const USB_SUBTYPE: u8 = 0x05;
pub const MEDIA_TYPE: u8 = 0x04;
pub const HARD_DRIVE_SUBTYPE: u8 = 0x01;
pub const GPT_SIGNATURE_TYPE: u8 = 0x02;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BootPartitionPath {
    pub partition_number: u32,
    pub partition_start_lba: u64,
    pub partition_size_lba: u64,
    pub partition_guid: [u8; 16],
    pub usb_seen: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Empty,
    Truncated,
    InvalidNodeLength,
    MissingEnd,
    MultipleHardDriveNodes,
    MissingHardDriveNode,
    NotUsb,
    NotGpt,
    EmptyPartitionGuid,
    InvalidPartition,
}

fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().expect("validated node field"))
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("validated node field"))
}

fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().expect("validated node field"))
}

/// Parse a copied UEFI device-path byte sequence.
///
/// The caller must first bound the firmware path by walking nodes until the
/// End Entire node while Boot Services are live; host tests feed the same wire
/// bytes directly.
pub fn parse(bytes: &[u8]) -> Result<BootPartitionPath, Error> {
    if bytes.is_empty() {
        return Err(Error::Empty);
    }
    let mut offset = 0usize;
    let mut usb_seen = false;
    let mut hard_drive = None;
    let mut ended = false;

    while offset < bytes.len() {
        if bytes.len() - offset < 4 {
            return Err(Error::Truncated);
        }
        let ty = bytes[offset];
        let subtype = bytes[offset + 1];
        let len = usize::from(u16_at(bytes, offset + 2));
        if len < 4 || len > bytes.len() - offset {
            return Err(Error::InvalidNodeLength);
        }

        let node = &bytes[offset..offset + len];
        if ty == DEVICE_PATH_END_TYPE && subtype == DEVICE_PATH_END_ENTIRE_SUBTYPE {
            if len != 4 || offset + len != bytes.len() {
                return Err(Error::MissingEnd);
            }
            ended = true;
            break;
        }

        if ty == MESSAGING_TYPE && subtype == USB_SUBTYPE {
            // USB messaging device path node is six bytes in UEFI.
            if len < 6 {
                return Err(Error::InvalidNodeLength);
            }
            usb_seen = true;
        }

        if ty == MEDIA_TYPE && subtype == HARD_DRIVE_SUBTYPE {
            // UEFI HARDDRIVE_DEVICE_PATH is 42 bytes:
            // header, partition number/start/size, 16-byte signature,
            // MBR type and signature type.
            if len != 42 {
                return Err(Error::InvalidNodeLength);
            }
            if hard_drive.is_some() {
                return Err(Error::MultipleHardDriveNodes);
            }
            let partition_number = u32_at(node, 4);
            let partition_start_lba = u64_at(node, 8);
            let partition_size_lba = u64_at(node, 16);
            let partition_guid: [u8; 16] =
                node[24..40].try_into().expect("fixed UEFI partition signature");
            let signature_type = node[41];
            if signature_type != GPT_SIGNATURE_TYPE {
                return Err(Error::NotGpt);
            }
            if partition_number == 0 || partition_size_lba == 0 {
                return Err(Error::InvalidPartition);
            }
            if partition_guid == [0; 16] {
                return Err(Error::EmptyPartitionGuid);
            }
            hard_drive = Some(BootPartitionPath {
                partition_number,
                partition_start_lba,
                partition_size_lba,
                partition_guid,
                usb_seen: false,
            });
        }
        offset += len;
    }

    if !ended {
        return Err(Error::MissingEnd);
    }
    if !usb_seen {
        return Err(Error::NotUsb);
    }
    let mut result = hard_drive.ok_or(Error::MissingHardDriveNode)?;
    result.usb_seen = true;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(usb: bool, signature_type: u8) -> std::vec::Vec<u8> {
        let mut bytes = std::vec::Vec::new();
        if usb {
            bytes.extend_from_slice(&[MESSAGING_TYPE, USB_SUBTYPE, 6, 0, 1, 2]);
        } else {
            bytes.extend_from_slice(&[0x01, 0x01, 4, 0]);
        }
        let mut hd = [0u8; 42];
        hd[0] = MEDIA_TYPE;
        hd[1] = HARD_DRIVE_SUBTYPE;
        hd[2..4].copy_from_slice(&42u16.to_le_bytes());
        hd[4..8].copy_from_slice(&1u32.to_le_bytes());
        hd[8..16].copy_from_slice(&2048u64.to_le_bytes());
        hd[16..24].copy_from_slice(&65536u64.to_le_bytes());
        hd[24..40].copy_from_slice(&[0x42; 16]);
        hd[40] = 0x02; // GPT partition format
        hd[41] = signature_type;
        bytes.extend_from_slice(&hd);
        bytes.extend_from_slice(&[
            DEVICE_PATH_END_TYPE,
            DEVICE_PATH_END_ENTIRE_SUBTYPE,
            4,
            0,
        ]);
        bytes
    }

    #[test]
    fn extracts_usb_gpt_partition_identity() {
        let parsed = parse(&path(true, GPT_SIGNATURE_TYPE)).unwrap();
        assert!(parsed.usb_seen);
        assert_eq!(parsed.partition_number, 1);
        assert_eq!(parsed.partition_start_lba, 2048);
        assert_eq!(parsed.partition_size_lba, 65536);
        assert_eq!(parsed.partition_guid, [0x42; 16]);
    }

    #[test]
    fn rejects_non_usb_or_non_gpt_origins() {
        assert_eq!(
            parse(&path(false, GPT_SIGNATURE_TYPE)),
            Err(Error::NotUsb)
        );
        assert_eq!(parse(&path(true, 1)), Err(Error::NotGpt));
    }

    #[test]
    fn malformed_and_ambiguous_paths_fail_closed() {
        let mut truncated = path(true, GPT_SIGNATURE_TYPE);
        truncated.pop();
        assert_eq!(parse(&truncated), Err(Error::InvalidNodeLength));

        let mut duplicate = path(true, GPT_SIGNATURE_TYPE);
        let end = duplicate.split_off(duplicate.len() - 4);
        duplicate.extend_from_slice(&path(true, GPT_SIGNATURE_TYPE)[6..48]);
        duplicate.extend_from_slice(&end);
        assert_eq!(parse(&duplicate), Err(Error::MultipleHardDriveNodes));

        let mut zero_guid = path(true, GPT_SIGNATURE_TYPE);
        zero_guid[6 + 24..6 + 40].fill(0);
        assert_eq!(parse(&zero_guid), Err(Error::EmptyPartitionGuid));
    }
}
