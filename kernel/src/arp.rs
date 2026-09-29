//! Ethernet/IPv4 ARP payload codec. No neighbor learning, replies or I/O.
use super::{MTU, MacAddress};

pub const PACKET_BYTES: usize = 28;
const HEADER: [u8; 6] = [0, 1, 8, 0, 6, 4];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Length,
    Format,
    Operation,
    SenderAddress,
    OutputTooSmall,
    Invariant,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum Operation {
    Request = 1,
    Reply = 2,
}

/// Protocol addresses remain uninterpreted, including the zero sender address
/// in an ARP probe. Request target MACs may be unknown. Parsing is not trust.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Packet {
    pub operation: Operation,
    pub sender_mac: MacAddress,
    pub sender_ip: [u8; 4],
    pub target_mac: [u8; 6],
    pub target_ip: [u8; 4],
}

impl Packet {
    /// Decode an Ethernet/IPv4 ARP payload, allowing enclosing link padding.
    /// The caller must first validate and dispatch the Ethernet EtherType.
    pub fn parse(input: &[u8]) -> Result<Self, Error> {
        if !(PACKET_BYTES..=MTU).contains(&input.len()) {
            return Err(Error::Length);
        }
        if input[..6] != HEADER {
            return Err(Error::Format);
        }
        let operation = match u16::from_be_bytes([input[6], input[7]]) {
            1 => Operation::Request,
            2 => Operation::Reply,
            _ => return Err(Error::Operation),
        };
        let mut sender_mac = [0; 6];
        sender_mac.copy_from_slice(&input[8..14]);
        let sender_mac = MacAddress::new(sender_mac).map_err(|_| Error::SenderAddress)?;
        let mut sender_ip = [0; 4];
        sender_ip.copy_from_slice(&input[14..18]);
        let mut target_mac = [0; 6];
        target_mac.copy_from_slice(&input[18..24]);
        let mut target_ip = [0; 4];
        target_ip.copy_from_slice(&input[24..28]);
        Ok(Self {
            operation,
            sender_mac,
            sender_ip,
            target_mac,
            target_ip,
        })
    }

    /// Write exactly 28 bytes, leaving any enclosing padding/suffix untouched.
    /// Insufficient capacity leaves the entire output unchanged. Ethernet
    /// padding must be initialized by the link encoder, not copied from here.
    pub fn write(&self, output: &mut [u8]) -> Result<usize, Error> {
        if output.len() < PACKET_BYTES {
            return Err(Error::OutputTooSmall);
        }
        output[..6].copy_from_slice(&HEADER);
        output[6..8].copy_from_slice(&(self.operation as u16).to_be_bytes());
        output[8..14].copy_from_slice(&self.sender_mac.bytes());
        output[14..18].copy_from_slice(&self.sender_ip);
        output[18..24].copy_from_slice(&self.target_mac);
        output[24..28].copy_from_slice(&self.target_ip);
        Ok(PACKET_BYTES)
    }
}

pub(super) fn self_test() -> Result<(), Error> {
    let sender_mac = MacAddress::new([2, 0, 0, 0, 0, 1]).map_err(|_| Error::Invariant)?;
    let mut packet = Packet {
        operation: Operation::Request,
        sender_mac,
        sender_ip: [0; 4],
        target_mac: [0; 6],
        target_ip: [192, 0, 2, 2],
    };
    let mut output = [0xa5; 46];
    for operation in [Operation::Request, Operation::Reply] {
        packet.operation = operation;
        if packet.write(&mut output)? != PACKET_BYTES
            || Packet::parse(&output)? != packet
            || output[PACKET_BYTES..].iter().any(|byte| *byte != 0xa5)
        {
            return Err(Error::Invariant);
        }
    }
    let mut short = [0x5a; PACKET_BYTES - 1];
    if packet.write(&mut short) != Err(Error::OutputTooSmall)
        || short != [0x5a; PACKET_BYTES - 1]
    {
        return Err(Error::Invariant);
    }
    output[4] = 255;
    if Packet::parse(&output) != Err(Error::Format) {
        return Err(Error::Invariant);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> Packet {
        Packet {
            operation: Operation::Request,
            sender_mac: MacAddress::new([2, 0, 0, 0, 0, 1]).unwrap(),
            sender_ip: [192, 0, 2, 1],
            target_mac: [0; 6],
            target_ip: [192, 0, 2, 2],
        }
    }

    #[test]
    fn production_codec_proof() {
        assert_eq!(self_test(), Ok(()));
    }

    #[test]
    fn canonical_request_wire_fixture() {
        let expected = [
            0, 1, 8, 0, 6, 4, 0, 1, 2, 0, 0, 0, 0, 1, 192, 0, 2, 1, 0, 0, 0, 0, 0, 0, 192, 0,
            2, 2,
        ];
        let mut output = [0xa5; PACKET_BYTES];
        assert_eq!(request().write(&mut output), Ok(PACKET_BYTES));
        assert_eq!(output, expected);
        assert_eq!(Packet::parse(&expected), Ok(request()));
    }

    #[test]
    fn reply_opcode_and_address_order() {
        let request = request();
        let reply = Packet {
            operation: Operation::Reply,
            sender_mac: MacAddress::new([2, 0, 0, 0, 0, 2]).unwrap(),
            sender_ip: request.target_ip,
            target_mac: request.sender_mac.bytes(),
            target_ip: request.sender_ip,
        };
        let mut output = [0; PACKET_BYTES];
        reply.write(&mut output).unwrap();
        assert_eq!(&output[6..8], &[0, 2]);
        assert_eq!(&output[8..14], &[2, 0, 0, 0, 0, 2]);
        assert_eq!(&output[14..18], &[192, 0, 2, 2]);
        assert_eq!(&output[18..24], &[2, 0, 0, 0, 0, 1]);
        assert_eq!(&output[24..28], &[192, 0, 2, 1]);
        assert_eq!(Packet::parse(&output), Ok(reply));
    }

    #[test]
    fn all_truncated_inputs_and_oversized_payload_are_rejected() {
        let mut input = [0; PACKET_BYTES];
        request().write(&mut input).unwrap();
        for length in 0..PACKET_BYTES {
            assert_eq!(Packet::parse(&input[..length]), Err(Error::Length));
        }
        assert_eq!(Packet::parse(&[0; MTU + 1]), Err(Error::Length));
    }

    #[test]
    fn incorrect_protocol_and_address_lengths_fail_before_address_access() {
        let mut input = [0; PACKET_BYTES];
        request().write(&mut input).unwrap();
        for index in 0..HEADER.len() {
            for value in 0..=u8::MAX {
                if value != HEADER[index] {
                    let mut malformed = input;
                    malformed[index] = value;
                    assert_eq!(Packet::parse(&malformed), Err(Error::Format));
                }
            }
        }
        for operation in [0u16, 3, 256, 512, u16::MAX] {
            input[6..8].copy_from_slice(&operation.to_be_bytes());
            assert_eq!(Packet::parse(&input), Err(Error::Operation));
        }
    }

    #[test]
    fn every_short_output_is_unchanged() {
        for length in 0..PACKET_BYTES {
            let mut output = [0xa5; PACKET_BYTES];
            assert_eq!(
                request().write(&mut output[..length]),
                Err(Error::OutputTooSmall)
            );
            assert_eq!(output, [0xa5; PACKET_BYTES]);
        }
    }

    #[test]
    fn padding_probe_sender_and_unknown_target_mac_are_preserved() {
        for target_mac in [[0; 6], [0xff; 6], [2, 0, 0, 0, 0, 2]] {
            let probe = Packet {
                sender_ip: [0; 4],
                target_mac,
                ..request()
            };
            let mut output = [0xa5; MTU];
            probe.write(&mut output).unwrap();
            assert_eq!(Packet::parse(&output[..46]), Ok(probe));
            assert_eq!(Packet::parse(&output), Ok(probe));
            assert!(output[PACKET_BYTES..].iter().all(|byte| *byte == 0xa5));
        }
    }

    #[test]
    fn invalid_sender_macs_are_rejected_without_changing_input() {
        let mut input = [0; PACKET_BYTES];
        request().write(&mut input).unwrap();
        for sender in [[0; 6], [0xff; 6], [1, 0, 0, 0, 0, 1]] {
            input[8..14].copy_from_slice(&sender);
            let before = input;
            assert_eq!(Packet::parse(&input), Err(Error::SenderAddress));
            assert_eq!(input, before);
        }
    }
}
