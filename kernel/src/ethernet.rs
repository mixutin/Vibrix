//! Bounded untagged Ethernet-II framing. No FCS, offloads or hardware access.
use super::{ETHERNET_HEADER, MAX_FRAME, MTU, MacAddress};

pub const MIN_FRAME: usize = 60;
pub const IPV4: u16 = 0x0800;
pub const ARP: u16 = 0x0806;
pub const IPV6: u16 = 0x86dd;
pub const BROADCAST: [u8; 6] = [0xff; 6];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    FrameSize,
    UnsupportedFraming,
    SourceAddress,
    OutputTooSmall,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Packet<'a> {
    destination: [u8; 6],
    source: MacAddress,
    ether_type: u16,
    payload: &'a [u8],
}

impl<'a> Packet<'a> {
    pub const fn destination(&self) -> [u8; 6] {
        self.destination
    }

    pub const fn source(&self) -> MacAddress {
        self.source
    }

    pub const fn ether_type(&self) -> u16 {
        self.ether_type
    }

    /// Includes any link padding. The higher-level protocol must establish its
    /// own payload length; trailing zeroes are not generally safe to strip.
    pub const fn payload(&self) -> &'a [u8] {
        self.payload
    }
}

fn validate_type(ether_type: u16) -> Result<(), Error> {
    if ether_type < 0x0600 || matches!(ether_type, 0x8100 | 0x88a8) {
        return Err(Error::UnsupportedFraming);
    }
    Ok(())
}

/// Accepts the NIC contract's 14..=1514 logical bytes, excluding preamble/FCS.
/// Destination filtering belongs to the caller, not to this wire decoder.
pub fn parse(frame: &[u8]) -> Result<Packet<'_>, Error> {
    if !(ETHERNET_HEADER..=MAX_FRAME).contains(&frame.len()) {
        return Err(Error::FrameSize);
    }
    let ether_type = u16::from_be_bytes([frame[12], frame[13]]);
    validate_type(ether_type)?;
    let mut destination = [0; 6];
    destination.copy_from_slice(&frame[..6]);
    let mut source = [0; 6];
    source.copy_from_slice(&frame[6..12]);
    let source = MacAddress::new(source).map_err(|_| Error::SourceAddress)?;
    Ok(Packet {
        destination,
        source,
        ether_type,
        payload: &frame[ETHERNET_HEADER..],
    })
}

/// Emit one frame, padding short payloads with zeroes to 60 bytes without FCS.
/// Every error leaves output unchanged, including its unused suffix.
pub fn encode(
    source: MacAddress,
    destination: [u8; 6],
    ether_type: u16,
    payload: &[u8],
    output: &mut [u8],
) -> Result<usize, Error> {
    validate_type(ether_type)?;
    if payload.len() > MTU {
        return Err(Error::FrameSize);
    }
    let end = ETHERNET_HEADER + payload.len();
    let length = end.max(MIN_FRAME);
    if output.len() < length {
        return Err(Error::OutputTooSmall);
    }
    output[..length].fill(0);
    output[..6].copy_from_slice(&destination);
    output[6..12].copy_from_slice(&source.bytes());
    output[12..14].copy_from_slice(&ether_type.to_be_bytes());
    output[ETHERNET_HEADER..end].copy_from_slice(payload);
    Ok(length)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn address() -> MacAddress {
        MacAddress::new([2, 0, 0, 0, 0, 1]).unwrap()
    }

    #[test]
    fn canonical_header_and_zero_padding() {
        let mut output = [0xa5; 64];
        assert_eq!(
            encode(address(), BROADCAST, IPV4, &[1, 2, 3], &mut output),
            Ok(60)
        );
        assert_eq!(
            &output[..14],
            &[255, 255, 255, 255, 255, 255, 2, 0, 0, 0, 0, 1, 8, 0]
        );
        assert_eq!(&output[14..17], &[1, 2, 3]);
        assert_eq!(&output[17..60], &[0; 43]);
        assert_eq!(&output[60..], &[0xa5; 4]);
        let packet = parse(&output[..60]).unwrap();
        assert_eq!(packet.source(), address());
        assert_eq!(packet.destination(), BROADCAST);
        assert_eq!(packet.ether_type(), IPV4);
        assert_eq!(packet.payload().len(), 46);
    }

    #[test]
    fn every_short_output_is_unchanged() {
        for length in 0..MIN_FRAME {
            let mut output = [0xa5; MIN_FRAME];
            assert_eq!(
                encode(address(), BROADCAST, ARP, &[0; 28], &mut output[..length]),
                Err(Error::OutputTooSmall)
            );
            assert_eq!(output, [0xa5; MIN_FRAME]);
        }
    }

    #[test]
    fn unsupported_framing_and_oversized_payload_do_not_mutate_output() {
        let mut output = [0xa5; MAX_FRAME];
        for ether_type in [0, 1500, 1535, 0x8100, 0x88a8] {
            assert_eq!(
                encode(address(), BROADCAST, ether_type, &[], &mut output),
                Err(Error::UnsupportedFraming)
            );
            assert_eq!(output, [0xa5; MAX_FRAME]);
        }
        assert_eq!(
            encode(address(), BROADCAST, IPV4, &[0; MTU + 1], &mut output),
            Err(Error::FrameSize)
        );
        assert_eq!(output, [0xa5; MAX_FRAME]);
    }

    #[test]
    fn receive_rejects_bad_lengths_framing_and_source_addresses() {
        let mut frame = [0; MAX_FRAME + 1];
        encode(address(), BROADCAST, IPV4, &[], &mut frame).unwrap();
        for length in 0..ETHERNET_HEADER {
            assert_eq!(parse(&frame[..length]), Err(Error::FrameSize));
        }
        assert_eq!(parse(&frame), Err(Error::FrameSize));
        for ether_type in [0u16, 1500, 1535, 0x8100, 0x88a8] {
            frame[12..14].copy_from_slice(&ether_type.to_be_bytes());
            assert_eq!(parse(&frame[..60]), Err(Error::UnsupportedFraming));
        }
        frame[12..14].copy_from_slice(&IPV4.to_be_bytes());
        for source in [[0; 6], BROADCAST, [1, 0, 0, 0, 0, 1]] {
            frame[6..12].copy_from_slice(&source);
            assert_eq!(parse(&frame[..60]), Err(Error::SourceAddress));
        }
    }

    #[test]
    fn maximum_payload_round_trip_preserves_unused_suffix() {
        let payload = [0x5a; MTU];
        let mut output = [0xa5; MAX_FRAME + 4];
        let length = encode(address(), address().bytes(), IPV6, &payload, &mut output).unwrap();
        assert_eq!(length, MAX_FRAME);
        assert_eq!(parse(&output[..length]).unwrap().payload(), payload);
        assert_eq!(&output[length..], &[0xa5; 4]);
    }

    #[test]
    fn logical_receive_and_unknown_ethertypes_preserve_payload_bytes() {
        let mut output = [0; MIN_FRAME];
        for destination in [BROADCAST, [1, 0, 94, 0, 0, 1], address().bytes()] {
            encode(address(), destination, 0x88b5, &[], &mut output).unwrap();
            let packet = parse(&output[..ETHERNET_HEADER]).unwrap();
            assert_eq!(packet.destination(), destination);
            assert_eq!(packet.ether_type(), 0x88b5);
            assert!(packet.payload().is_empty());
        }
    }

    #[test]
    fn production_nic_loopback_uses_codec() {
        assert_eq!(super::super::self_test(), Ok(()));
    }
}
