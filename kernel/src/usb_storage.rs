//! Bounded USB Mass Storage Class descriptor policy.
//!
//! The native xHCI transport uses this parser to select exactly one
//! interface implementing MSC/SCSI transparent/Bulk-Only Transport and one
//! Bulk-IN plus one Bulk-OUT endpoint. It performs no MMIO or DMA itself.

pub const MASS_STORAGE_CLASS: u8 = 0x08;
pub const SCSI_TRANSPARENT_SUBCLASS: u8 = 0x06;
pub const BULK_ONLY_PROTOCOL: u8 = 0x50;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Malformed,
    Missing,
    Ambiguous,
    Unsupported,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Endpoint {
    pub address: u8,
    pub max_packet: u16,
}

impl Endpoint {
    pub const fn is_in(self) -> bool {
        self.address & 0x80 != 0
    }

    pub const fn number(self) -> u8 {
        self.address & 0x0f
    }

    pub const fn context_index(self) -> u8 {
        self.number() * 2 + if self.is_in() { 1 } else { 0 }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BulkInterface {
    pub configuration: u8,
    pub interface: u8,
    pub bulk_in: Endpoint,
    pub bulk_out: Endpoint,
}

pub fn bulk_interface(bytes: &[u8]) -> Result<BulkInterface, Error> {
    if bytes.len() < 9 || bytes[0] < 9 || bytes[1] != 0x02 {
        return Err(Error::Malformed);
    }
    let total = usize::from(u16::from_le_bytes([bytes[2], bytes[3]]));
    if total < 9 || total > bytes.len() {
        return Err(Error::Malformed);
    }
    let configuration = bytes[5];
    if configuration == 0 {
        return Err(Error::Malformed);
    }

    let mut selected = None;
    let mut active = false;
    let mut bulk_in = None;
    let mut bulk_out = None;
    let mut offset = usize::from(bytes[0]);

    while offset < total {
        if offset + 2 > total {
            return Err(Error::Malformed);
        }
        let length = usize::from(bytes[offset]);
        let kind = bytes[offset + 1];
        if length < 2 || offset.checked_add(length).is_none_or(|end| end > total) {
            return Err(Error::Malformed);
        }

        match kind {
            0x04 => {
                if length < 9 {
                    return Err(Error::Malformed);
                }
                active = false;
                let alternate = bytes[offset + 3];
                let class = bytes[offset + 5];
                let subclass = bytes[offset + 6];
                let protocol = bytes[offset + 7];
                if alternate == 0
                    && class == MASS_STORAGE_CLASS
                    && subclass == SCSI_TRANSPARENT_SUBCLASS
                    && protocol == BULK_ONLY_PROTOCOL
                {
                    if selected.is_some() {
                        return Err(Error::Ambiguous);
                    }
                    selected = Some(bytes[offset + 2]);
                    bulk_in = None;
                    bulk_out = None;
                    active = true;
                }
            }
            0x05 if active => {
                if length < 7 {
                    return Err(Error::Malformed);
                }
                let address = bytes[offset + 2];
                let attributes = bytes[offset + 3] & 0x03;
                let raw_packet = u16::from_le_bytes([bytes[offset + 4], bytes[offset + 5]]);
                let max_packet = raw_packet & 0x07ff;
                let number = address & 0x0f;
                if attributes != 0x02 {
                    // The selected interface may contain a non-bulk endpoint,
                    // but this first bounded transport does not support it.
                    return Err(Error::Unsupported);
                }
                if number == 0 || max_packet == 0 || max_packet > 1024 {
                    return Err(Error::Malformed);
                }
                let endpoint = Endpoint {
                    address,
                    max_packet,
                };
                if endpoint.is_in() {
                    if bulk_in.replace(endpoint).is_some() {
                        return Err(Error::Ambiguous);
                    }
                } else if bulk_out.replace(endpoint).is_some() {
                    return Err(Error::Ambiguous);
                }
            }
            _ => {}
        }

        offset += length;
    }

    let interface = selected.ok_or(Error::Missing)?;
    Ok(BulkInterface {
        configuration,
        interface,
        bulk_in: bulk_in.ok_or(Error::Missing)?,
        bulk_out: bulk_out.ok_or(Error::Missing)?,
    })
}

pub fn self_test() -> Result<(), Error> {
    let descriptor = [
        9, 2, 32, 0, 1, 1, 0, 0x80, 50, // configuration
        9, 4, 0, 0, 2, 0x08, 0x06, 0x50, 0, // MSC interface
        7, 5, 0x02, 0x02, 0x00, 0x02, 0, // bulk OUT, 512 bytes
        7, 5, 0x81, 0x02, 0x00, 0x02, 0, // bulk IN, 512 bytes
    ];
    let interface = bulk_interface(&descriptor)?;
    if interface.configuration != 1
        || interface.interface != 0
        || interface.bulk_out.address != 0x02
        || interface.bulk_out.max_packet != 512
        || interface.bulk_out.context_index() != 4
        || interface.bulk_in.address != 0x81
        || interface.bulk_in.max_packet != 512
        || interface.bulk_in.context_index() != 3
    {
        return Err(Error::Malformed);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor() -> [u8; 32] {
        [
            9, 2, 32, 0, 1, 1, 0, 0x80, 50, 9, 4, 0, 0, 2, 0x08, 0x06, 0x50, 0, 7, 5, 0x02, 0x02,
            0x00, 0x02, 0, 7, 5, 0x81, 0x02, 0x00, 0x02, 0,
        ]
    }

    #[test]
    fn selects_scsi_bot_bulk_pair() {
        let interface = bulk_interface(&descriptor()).unwrap();
        assert_eq!(interface.configuration, 1);
        assert_eq!(interface.interface, 0);
        assert_eq!(interface.bulk_out.address, 0x02);
        assert_eq!(interface.bulk_out.context_index(), 4);
        assert_eq!(interface.bulk_in.address, 0x81);
        assert_eq!(interface.bulk_in.context_index(), 3);
        assert_eq!(interface.bulk_in.max_packet, 512);
    }

    #[test]
    fn rejects_missing_wrong_and_duplicate_endpoints() {
        let mut missing = descriptor();
        missing[27] = 0x03;
        assert_eq!(bulk_interface(&missing), Err(Error::Ambiguous));

        let mut wrong_class = descriptor();
        wrong_class[14] = 0x03;
        assert_eq!(bulk_interface(&wrong_class), Err(Error::Missing));

        let mut duplicate = descriptor();
        duplicate[20] = 0x82;
        assert_eq!(bulk_interface(&duplicate), Err(Error::Ambiguous));
    }

    #[test]
    fn rejects_malformed_lengths_and_endpoint_zero() {
        let mut short = descriptor();
        short[0] = 1;
        assert_eq!(bulk_interface(&short), Err(Error::Malformed));

        let mut ep0 = descriptor();
        ep0[20] = 0x00;
        assert_eq!(bulk_interface(&ep0), Err(Error::Malformed));

        let mut oversized = descriptor();
        oversized[2] = 33;
        assert_eq!(bulk_interface(&oversized), Err(Error::Malformed));
    }

    #[test]
    fn production_self_test_passes() {
        self_test().unwrap();
    }
}
