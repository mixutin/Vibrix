//! Bounded UDP datagram codec over the Vibrix IPv4 foundation.
//!
//! This module implements RFC 768 framing/checksums only. It does not allocate
//! sockets, ports, queues, routing state or perform device I/O.

use super::ipv4;

pub const HEADER_BYTES: usize = 8;
pub const MAX_PAYLOAD: usize = ipv4::MAX_PAYLOAD - HEADER_BYTES;
pub const PROTOCOL: u8 = 17;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Length,
    Checksum,
    OutputTooSmall,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Datagram<'a> {
    source_port: u16,
    destination_port: u16,
    checksum_present: bool,
    payload: &'a [u8],
}

impl<'a> Datagram<'a> {
    pub const fn source_port(&self) -> u16 {
        self.source_port
    }

    pub const fn destination_port(&self) -> u16 {
        self.destination_port
    }

    pub const fn checksum_present(&self) -> bool {
        self.checksum_present
    }

    pub const fn payload(&self) -> &'a [u8] {
        self.payload
    }
}

fn add_word(sum: &mut u32, word: u16) {
    *sum = sum.wrapping_add(u32::from(word));
}

fn add_bytes(sum: &mut u32, bytes: &[u8]) {
    let (chunks, remainder) = bytes.as_chunks::<2>();
    for chunk in chunks {
        add_word(sum, u16::from_be_bytes(*chunk));
    }
    if let [last] = remainder {
        add_word(sum, u16::from_be_bytes([*last, 0]));
    }
}

fn folded_sum(source: [u8; 4], destination: [u8; 4], udp: &[u8]) -> u16 {
    let mut sum = 0u32;
    add_bytes(&mut sum, &source);
    add_bytes(&mut sum, &destination);
    add_word(&mut sum, PROTOCOL as u16);
    add_word(&mut sum, udp.len() as u16);
    add_bytes(&mut sum, udp);
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    sum as u16
}

fn checksum(source: [u8; 4], destination: [u8; 4], udp: &[u8]) -> u16 {
    let value = !folded_sum(source, destination, udp);
    if value == 0 { 0xffff } else { value }
}

fn checksum_valid(source: [u8; 4], destination: [u8; 4], udp: &[u8]) -> bool {
    folded_sum(source, destination, udp) == 0xffff
}

/// Decode one UDP datagram. Bytes after the UDP Length field are ignored as
/// enclosing IP/link padding. A transmitted checksum of zero is accepted and
/// reported as absent, as permitted by RFC 768. Non-zero checksums are always
/// validated against the IPv4 pseudo-header.
pub fn parse(source: [u8; 4], destination: [u8; 4], input: &[u8]) -> Result<Datagram<'_>, Error> {
    if !(HEADER_BYTES..=ipv4::MAX_PAYLOAD).contains(&input.len()) {
        return Err(Error::Length);
    }
    let length = u16::from_be_bytes([input[4], input[5]]) as usize;
    if !(HEADER_BYTES..=ipv4::MAX_PAYLOAD).contains(&length) || length > input.len() {
        return Err(Error::Length);
    }
    let checksum_field = u16::from_be_bytes([input[6], input[7]]);
    if checksum_field != 0 && !checksum_valid(source, destination, &input[..length]) {
        return Err(Error::Checksum);
    }

    Ok(Datagram {
        source_port: u16::from_be_bytes([input[0], input[1]]),
        destination_port: u16::from_be_bytes([input[2], input[3]]),
        checksum_present: checksum_field != 0,
        payload: &input[HEADER_BYTES..length],
    })
}

/// Encode one UDP datagram with checksum generation enabled.
///
/// RFC 1122 requires hosts to implement checksum generation/validation and
/// default generation to on. If the mathematical checksum is zero, RFC 768
/// requires transmitting all ones instead.
pub fn encode(
    source: [u8; 4],
    destination: [u8; 4],
    source_port: u16,
    destination_port: u16,
    payload: &[u8],
    output: &mut [u8],
) -> Result<usize, Error> {
    if payload.len() > MAX_PAYLOAD {
        return Err(Error::Length);
    }
    let length = HEADER_BYTES + payload.len();
    if output.len() < length {
        return Err(Error::OutputTooSmall);
    }

    output[..length].fill(0);
    output[0..2].copy_from_slice(&source_port.to_be_bytes());
    output[2..4].copy_from_slice(&destination_port.to_be_bytes());
    output[4..6].copy_from_slice(&(length as u16).to_be_bytes());
    output[HEADER_BYTES..length].copy_from_slice(payload);
    let value = checksum(source, destination, &output[..length]);
    output[6..8].copy_from_slice(&value.to_be_bytes());
    Ok(length)
}

pub(super) fn self_test() -> Result<(), Error> {
    let source = [192, 0, 2, 10];
    let destination = [192, 0, 2, 20];
    let mut udp = [0u8; ipv4::MAX_PAYLOAD];
    let length = encode(source, destination, 49152, 53, b"vibrix", &mut udp)?;
    let datagram = parse(source, destination, &udp[..length])?;
    if datagram.source_port() != 49152
        || datagram.destination_port() != 53
        || !datagram.checksum_present()
        || datagram.payload() != b"vibrix"
    {
        return Err(Error::Checksum);
    }

    let mut ip = [0u8; super::MTU];
    let ip_length = ipv4::encode(
        ipv4::Header {
            identification: 0x7070,
            dont_fragment: true,
            ttl: ipv4::DEFAULT_TTL,
            protocol: PROTOCOL,
            source,
            destination,
        },
        &udp[..length],
        &mut ip,
    )
    .map_err(|_| Error::Length)?;
    let packet = ipv4::parse(&ip[..ip_length]).map_err(|_| Error::Length)?;
    if packet.protocol() != PROTOCOL
        || parse(packet.source(), packet.destination(), packet.payload())?.payload() != b"vibrix"
    {
        return Err(Error::Checksum);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addresses() -> ([u8; 4], [u8; 4]) {
        ([192, 0, 2, 1], [198, 51, 100, 7])
    }

    #[test]
    fn odd_length_payload_round_trip_uses_checksum() {
        let (source, destination) = addresses();
        let mut output = [0xa5; 32];
        let length = encode(source, destination, 1234, 4321, &[1, 2, 3], &mut output).unwrap();
        assert_eq!(length, 11);
        assert_eq!(&output[length..], &[0xa5; 21]);
        let datagram = parse(source, destination, &output[..length]).unwrap();
        assert_eq!(datagram.source_port(), 1234);
        assert_eq!(datagram.destination_port(), 4321);
        assert!(datagram.checksum_present());
        assert_eq!(datagram.payload(), [1, 2, 3]);
    }

    #[test]
    fn pseudo_header_addresses_are_covered() {
        let (source, destination) = addresses();
        let mut output = [0u8; 32];
        let length = encode(source, destination, 1, 2, b"x", &mut output).unwrap();
        assert_eq!(
            parse([203, 0, 113, 1], destination, &output[..length]),
            Err(Error::Checksum)
        );
        assert_eq!(
            parse(source, [203, 0, 113, 1], &output[..length]),
            Err(Error::Checksum)
        );
    }

    #[test]
    fn nonzero_corrupt_checksum_is_rejected() {
        let (source, destination) = addresses();
        let mut output = [0u8; 32];
        let length = encode(source, destination, 1, 2, b"abc", &mut output).unwrap();
        output[HEADER_BYTES] ^= 1;
        assert_eq!(
            parse(source, destination, &output[..length]),
            Err(Error::Checksum)
        );
    }

    #[test]
    fn zero_transmitted_checksum_is_reported_as_absent() {
        let (source, destination) = addresses();
        let mut raw = [0u8; HEADER_BYTES];
        raw[0..2].copy_from_slice(&1u16.to_be_bytes());
        raw[2..4].copy_from_slice(&2u16.to_be_bytes());
        raw[4..6].copy_from_slice(&(HEADER_BYTES as u16).to_be_bytes());
        let datagram = parse(source, destination, &raw).unwrap();
        assert!(!datagram.checksum_present());
        assert!(datagram.payload().is_empty());
    }

    #[test]
    fn invalid_length_and_capacity_are_transactional() {
        let (source, destination) = addresses();
        let mut output = [0xa5; HEADER_BYTES];
        assert_eq!(
            encode(
                source,
                destination,
                1,
                2,
                &[0; MAX_PAYLOAD + 1],
                &mut output
            ),
            Err(Error::Length)
        );
        assert_eq!(output, [0xa5; HEADER_BYTES]);
        assert_eq!(
            encode(source, destination, 1, 2, &[1], &mut output),
            Err(Error::OutputTooSmall)
        );
        assert_eq!(output, [0xa5; HEADER_BYTES]);

        let mut raw = [0u8; HEADER_BYTES];
        raw[4..6].copy_from_slice(&7u16.to_be_bytes());
        assert_eq!(parse(source, destination, &raw), Err(Error::Length));
    }

    #[test]
    fn production_ipv4_udp_path() {
        assert_eq!(self_test(), Ok(()));
    }
}
