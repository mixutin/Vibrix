//! Bounded IPv4 header codec for the first Vibrix Internet layer.
//!
//! Scope is deliberately small: IPv4 version 4, IHL=5 (no options), no
//! fragmentation/reassembly, one complete datagram carried in an Ethernet
//! payload, and a validated header checksum.

use super::{MTU, internet_checksum};

pub const HEADER_BYTES: usize = 20;
pub const MAX_PAYLOAD: usize = MTU - HEADER_BYTES;
pub const ICMP_PROTOCOL: u8 = 1;
pub const DEFAULT_TTL: u8 = 64;

const VERSION_IHL: u8 = 0x45;
const FLAG_RESERVED: u16 = 0x8000;
const FLAG_DONT_FRAGMENT: u16 = 0x4000;
const FLAG_MORE_FRAGMENTS: u16 = 0x2000;
const FRAGMENT_OFFSET_MASK: u16 = 0x1fff;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Length,
    Version,
    HeaderLength,
    Checksum,
    Fragmented,
    ReservedFlag,
    TtlExpired,
    OutputTooSmall,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Packet<'a> {
    identification: u16,
    dont_fragment: bool,
    ttl: u8,
    protocol: u8,
    source: [u8; 4],
    destination: [u8; 4],
    payload: &'a [u8],
}

impl<'a> Packet<'a> {
    pub const fn identification(&self) -> u16 {
        self.identification
    }

    pub const fn dont_fragment(&self) -> bool {
        self.dont_fragment
    }

    pub const fn ttl(&self) -> u8 {
        self.ttl
    }

    pub const fn protocol(&self) -> u8 {
        self.protocol
    }

    pub const fn source(&self) -> [u8; 4] {
        self.source
    }

    pub const fn destination(&self) -> [u8; 4] {
        self.destination
    }

    pub const fn payload(&self) -> &'a [u8] {
        self.payload
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Header {
    pub identification: u16,
    pub dont_fragment: bool,
    pub ttl: u8,
    pub protocol: u8,
    pub source: [u8; 4],
    pub destination: [u8; 4],
}

/// Parse one IPv4 datagram. Enclosing Ethernet padding after Total Length is
/// allowed and excluded from the returned payload.
pub fn parse(input: &[u8]) -> Result<Packet<'_>, Error> {
    if !(HEADER_BYTES..=MTU).contains(&input.len()) {
        return Err(Error::Length);
    }
    if input[0] >> 4 != 4 {
        return Err(Error::Version);
    }
    if input[0] & 0x0f != 5 {
        return Err(Error::HeaderLength);
    }
    let total_length = u16::from_be_bytes([input[2], input[3]]) as usize;
    if !(HEADER_BYTES..=MTU).contains(&total_length) || total_length > input.len() {
        return Err(Error::Length);
    }
    if !internet_checksum::valid(&input[..HEADER_BYTES]) {
        return Err(Error::Checksum);
    }

    let flags_fragment = u16::from_be_bytes([input[6], input[7]]);
    if flags_fragment & FLAG_RESERVED != 0 {
        return Err(Error::ReservedFlag);
    }
    if flags_fragment & (FLAG_MORE_FRAGMENTS | FRAGMENT_OFFSET_MASK) != 0 {
        return Err(Error::Fragmented);
    }

    let ttl = input[8];
    if ttl == 0 {
        return Err(Error::TtlExpired);
    }

    let mut source = [0; 4];
    source.copy_from_slice(&input[12..16]);
    let mut destination = [0; 4];
    destination.copy_from_slice(&input[16..20]);

    Ok(Packet {
        identification: u16::from_be_bytes([input[4], input[5]]),
        dont_fragment: flags_fragment & FLAG_DONT_FRAGMENT != 0,
        ttl,
        protocol: input[9],
        source,
        destination,
        payload: &input[HEADER_BYTES..total_length],
    })
}

/// Encode one no-options, non-fragmented IPv4 datagram.
///
/// All validation occurs before output mutation. The checksum field is emitted
/// from a zeroed header and covers only the fixed 20-byte header.
pub fn encode(header: Header, payload: &[u8], output: &mut [u8]) -> Result<usize, Error> {
    if header.ttl == 0 {
        return Err(Error::TtlExpired);
    }
    if payload.len() > MAX_PAYLOAD {
        return Err(Error::Length);
    }
    let total_length = HEADER_BYTES + payload.len();
    if output.len() < total_length {
        return Err(Error::OutputTooSmall);
    }

    output[..total_length].fill(0);
    output[0] = VERSION_IHL;
    output[2..4].copy_from_slice(&(total_length as u16).to_be_bytes());
    output[4..6].copy_from_slice(&header.identification.to_be_bytes());
    let flags = if header.dont_fragment {
        FLAG_DONT_FRAGMENT
    } else {
        0
    };
    output[6..8].copy_from_slice(&flags.to_be_bytes());
    output[8] = header.ttl;
    output[9] = header.protocol;
    output[12..16].copy_from_slice(&header.source);
    output[16..20].copy_from_slice(&header.destination);
    let checksum = internet_checksum::checksum(&output[..HEADER_BYTES]);
    output[10..12].copy_from_slice(&checksum.to_be_bytes());
    output[HEADER_BYTES..total_length].copy_from_slice(payload);
    Ok(total_length)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header() -> Header {
        Header {
            identification: 0x1234,
            dont_fragment: true,
            ttl: 64,
            protocol: ICMP_PROTOCOL,
            source: [192, 0, 2, 10],
            destination: [192, 0, 2, 20],
        }
    }

    #[test]
    fn round_trip_excludes_link_padding_and_checks_header() {
        let mut output = [0xa5; 64];
        let length = encode(header(), &[1, 2, 3, 4], &mut output).unwrap();
        assert_eq!(length, 24);
        assert_eq!(&output[length..], &[0xa5; 40]);
        let mut padded = [0u8; 46];
        padded[..length].copy_from_slice(&output[..length]);
        let packet = parse(&padded).unwrap();
        assert_eq!(packet.identification(), 0x1234);
        assert!(packet.dont_fragment());
        assert_eq!(packet.ttl(), 64);
        assert_eq!(packet.protocol(), ICMP_PROTOCOL);
        assert_eq!(packet.source(), [192, 0, 2, 10]);
        assert_eq!(packet.destination(), [192, 0, 2, 20]);
        assert_eq!(packet.payload(), [1, 2, 3, 4]);
    }

    #[test]
    fn malformed_header_is_rejected() {
        let mut output = [0u8; 64];
        let length = encode(header(), &[1, 2, 3], &mut output).unwrap();

        let mut bad = output;
        bad[0] = 0x65;
        assert_eq!(parse(&bad[..length]), Err(Error::Version));

        let mut bad = output;
        bad[0] = 0x46;
        assert_eq!(parse(&bad[..length]), Err(Error::HeaderLength));

        let mut bad = output;
        bad[8] ^= 1;
        assert_eq!(parse(&bad[..length]), Err(Error::Checksum));

        let mut bad = output;
        bad[6..8].copy_from_slice(&FLAG_MORE_FRAGMENTS.to_be_bytes());
        // Recompute the checksum so fragmentation is the actual rejection.
        bad[10..12].fill(0);
        let sum = internet_checksum::checksum(&bad[..HEADER_BYTES]);
        bad[10..12].copy_from_slice(&sum.to_be_bytes());
        assert_eq!(parse(&bad[..length]), Err(Error::Fragmented));
    }

    #[test]
    fn invalid_lengths_and_ttl_leave_output_unchanged() {
        let mut output = [0xa5; 32];
        let mut zero_ttl = header();
        zero_ttl.ttl = 0;
        assert_eq!(
            encode(zero_ttl, &[], &mut output),
            Err(Error::TtlExpired)
        );
        assert_eq!(output, [0xa5; 32]);

        assert_eq!(
            encode(header(), &[0; MAX_PAYLOAD + 1], &mut output),
            Err(Error::Length)
        );
        assert_eq!(output, [0xa5; 32]);

        assert_eq!(
            encode(header(), &[1; 13], &mut output[..32]),
            Err(Error::OutputTooSmall)
        );
        assert_eq!(output, [0xa5; 32]);
    }

    #[test]
    fn zero_ttl_and_reserved_flag_fail_after_valid_checksum() {
        let mut output = [0u8; HEADER_BYTES];
        encode(header(), &[], &mut output).unwrap();

        output[8] = 0;
        output[10..12].fill(0);
        let sum = internet_checksum::checksum(&output);
        output[10..12].copy_from_slice(&sum.to_be_bytes());
        assert_eq!(parse(&output), Err(Error::TtlExpired));

        encode(header(), &[], &mut output).unwrap();
        output[6..8].copy_from_slice(&(FLAG_RESERVED | FLAG_DONT_FRAGMENT).to_be_bytes());
        output[10..12].fill(0);
        let sum = internet_checksum::checksum(&output);
        output[10..12].copy_from_slice(&sum.to_be_bytes());
        assert_eq!(parse(&output), Err(Error::ReservedFlag));
    }
}
