//! ICMPv4 Echo Request/Reply support over the bounded Vibrix IPv4 layer.
//!
//! Unknown ICMP types are not handled here. The responder is local-unicast
//! only to avoid broadcast echo amplification and does not perform routing.

use super::{MTU, MacAddress, ethernet, internet_checksum, ipv4};

pub const HEADER_BYTES: usize = 8;
pub const ECHO_REPLY: u8 = 0;
pub const ECHO_REQUEST: u8 = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Length,
    Type,
    Code,
    Checksum,
    OutputTooSmall,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    Reply,
    Request,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Echo<'a> {
    kind: Kind,
    identifier: u16,
    sequence: u16,
    data: &'a [u8],
}

impl<'a> Echo<'a> {
    pub const fn kind(&self) -> Kind {
        self.kind
    }

    pub const fn identifier(&self) -> u16 {
        self.identifier
    }

    pub const fn sequence(&self) -> u16 {
        self.sequence
    }

    pub const fn data(&self) -> &'a [u8] {
        self.data
    }
}

pub fn parse_echo(input: &[u8]) -> Result<Echo<'_>, Error> {
    if !(HEADER_BYTES..=ipv4::MAX_PAYLOAD).contains(&input.len()) {
        return Err(Error::Length);
    }
    let kind = match input[0] {
        ECHO_REPLY => Kind::Reply,
        ECHO_REQUEST => Kind::Request,
        _ => return Err(Error::Type),
    };
    if input[1] != 0 {
        return Err(Error::Code);
    }
    if !internet_checksum::valid(input) {
        return Err(Error::Checksum);
    }
    Ok(Echo {
        kind,
        identifier: u16::from_be_bytes([input[4], input[5]]),
        sequence: u16::from_be_bytes([input[6], input[7]]),
        data: &input[HEADER_BYTES..],
    })
}

pub fn encode_echo(
    kind: Kind,
    identifier: u16,
    sequence: u16,
    data: &[u8],
    output: &mut [u8],
) -> Result<usize, Error> {
    let length = HEADER_BYTES
        .checked_add(data.len())
        .ok_or(Error::Length)?;
    if length > ipv4::MAX_PAYLOAD {
        return Err(Error::Length);
    }
    if output.len() < length {
        return Err(Error::OutputTooSmall);
    }
    output[..length].fill(0);
    output[0] = match kind {
        Kind::Reply => ECHO_REPLY,
        Kind::Request => ECHO_REQUEST,
    };
    output[4..6].copy_from_slice(&identifier.to_be_bytes());
    output[6..8].copy_from_slice(&sequence.to_be_bytes());
    output[HEADER_BYTES..length].copy_from_slice(data);
    let checksum = internet_checksum::checksum(&output[..length]);
    output[2..4].copy_from_slice(&checksum.to_be_bytes());
    Ok(length)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RespondError {
    Ethernet(ethernet::Error),
    Ipv4(ipv4::Error),
    Icmp(Error),
}

/// Build a local-unicast ICMP Echo Reply for one complete Ethernet frame.
///
/// `Ok(None)` means the valid-enough frame is not an echo request for this
/// MAC/IP, including broadcast/multicast destination, non-IPv4 EtherType,
/// another IPv4 protocol or an Echo Reply. Malformed IPv4/ICMP is reported.
pub fn echo_reply_frame(
    local_mac: MacAddress,
    local_ip: [u8; 4],
    request_frame: &[u8],
    output: &mut [u8],
) -> Result<Option<usize>, RespondError> {
    let frame = ethernet::parse(request_frame).map_err(RespondError::Ethernet)?;
    if frame.ether_type() != ethernet::IPV4 || frame.destination() != local_mac.bytes() {
        return Ok(None);
    }

    let packet = ipv4::parse(frame.payload()).map_err(RespondError::Ipv4)?;
    if packet.destination() != local_ip || packet.protocol() != ipv4::ICMP_PROTOCOL {
        return Ok(None);
    }

    let echo = match parse_echo(packet.payload()) {
        Ok(echo) => echo,
        Err(Error::Type) => return Ok(None),
        Err(error) => return Err(RespondError::Icmp(error)),
    };
    if echo.kind() != Kind::Request {
        return Ok(None);
    }

    let mut icmp_bytes = [0u8; ipv4::MAX_PAYLOAD];
    let icmp_length = encode_echo(
        Kind::Reply,
        echo.identifier(),
        echo.sequence(),
        echo.data(),
        &mut icmp_bytes,
    )
    .map_err(RespondError::Icmp)?;

    let mut ip_bytes = [0u8; MTU];
    let ip_length = ipv4::encode(
        ipv4::Header {
            identification: packet.identification(),
            dont_fragment: true,
            ttl: ipv4::DEFAULT_TTL,
            protocol: ipv4::ICMP_PROTOCOL,
            source: local_ip,
            destination: packet.source(),
        },
        &icmp_bytes[..icmp_length],
        &mut ip_bytes,
    )
    .map_err(RespondError::Ipv4)?;

    ethernet::encode(
        local_mac,
        frame.source().bytes(),
        ethernet::IPV4,
        &ip_bytes[..ip_length],
        output,
    )
    .map(Some)
    .map_err(RespondError::Ethernet)
}

pub(super) fn self_test() -> Result<(), RespondError> {
    let local_mac = MacAddress::new([2, 0, 0, 0, 0, 1])
        .map_err(|_| RespondError::Ethernet(ethernet::Error::SourceAddress))?;
    let remote_mac = MacAddress::new([2, 0, 0, 0, 0, 2])
        .map_err(|_| RespondError::Ethernet(ethernet::Error::SourceAddress))?;
    let local_ip = [192, 0, 2, 1];
    let remote_ip = [192, 0, 2, 2];

    let mut request_icmp = [0u8; ipv4::MAX_PAYLOAD];
    let request_icmp_length =
        encode_echo(Kind::Request, 0x1234, 7, b"vibrix", &mut request_icmp)
            .map_err(RespondError::Icmp)?;
    let mut request_ip = [0u8; MTU];
    let request_ip_length = ipv4::encode(
        ipv4::Header {
            identification: 0x4567,
            dont_fragment: true,
            ttl: 31,
            protocol: ipv4::ICMP_PROTOCOL,
            source: remote_ip,
            destination: local_ip,
        },
        &request_icmp[..request_icmp_length],
        &mut request_ip,
    )
    .map_err(RespondError::Ipv4)?;
    let mut request_frame = [0u8; super::MAX_FRAME];
    let request_frame_length = ethernet::encode(
        remote_mac,
        local_mac.bytes(),
        ethernet::IPV4,
        &request_ip[..request_ip_length],
        &mut request_frame,
    )
    .map_err(RespondError::Ethernet)?;

    let mut reply_frame = [0xa5; super::MAX_FRAME];
    let reply_length = echo_reply_frame(
        local_mac,
        local_ip,
        &request_frame[..request_frame_length],
        &mut reply_frame,
    )?
    .ok_or(RespondError::Icmp(Error::Type))?;

    let frame = ethernet::parse(&reply_frame[..reply_length]).map_err(RespondError::Ethernet)?;
    if frame.source() != local_mac || frame.destination() != remote_mac.bytes() {
        return Err(RespondError::Ethernet(ethernet::Error::SourceAddress));
    }
    let packet = ipv4::parse(frame.payload()).map_err(RespondError::Ipv4)?;
    if packet.source() != local_ip
        || packet.destination() != remote_ip
        || packet.protocol() != ipv4::ICMP_PROTOCOL
        || packet.ttl() != ipv4::DEFAULT_TTL
    {
        return Err(RespondError::Ipv4(ipv4::Error::Version));
    }
    let echo = parse_echo(packet.payload()).map_err(RespondError::Icmp)?;
    if echo.kind() != Kind::Reply
        || echo.identifier() != 0x1234
        || echo.sequence() != 7
        || echo.data() != b"vibrix"
    {
        return Err(RespondError::Icmp(Error::Type));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mac(bytes: [u8; 6]) -> MacAddress {
        MacAddress::new(bytes).unwrap()
    }

    #[test]
    fn canonical_empty_echo_request_checksum() {
        let mut output = [0u8; HEADER_BYTES];
        assert_eq!(
            encode_echo(Kind::Request, 0, 0, &[], &mut output),
            Ok(HEADER_BYTES)
        );
        assert_eq!(output, [8, 0, 0xf7, 0xff, 0, 0, 0, 0]);
        let echo = parse_echo(&output).unwrap();
        assert_eq!(echo.kind(), Kind::Request);
        assert_eq!(echo.identifier(), 0);
        assert_eq!(echo.sequence(), 0);
        assert!(echo.data().is_empty());
    }

    #[test]
    fn odd_data_round_trip_and_corruption_detection() {
        let mut output = [0xa5; 32];
        let length = encode_echo(Kind::Reply, 0xabcd, 9, &[1, 2, 3], &mut output).unwrap();
        assert_eq!(&output[length..], &[0xa5; 21]);
        let echo = parse_echo(&output[..length]).unwrap();
        assert_eq!(echo.kind(), Kind::Reply);
        assert_eq!(echo.identifier(), 0xabcd);
        assert_eq!(echo.sequence(), 9);
        assert_eq!(echo.data(), [1, 2, 3]);

        output[length - 1] ^= 1;
        assert_eq!(parse_echo(&output[..length]), Err(Error::Checksum));
    }

    #[test]
    fn output_errors_are_transactional() {
        let mut output = [0xa5; 8];
        assert_eq!(
            encode_echo(Kind::Request, 1, 2, &[3], &mut output),
            Err(Error::OutputTooSmall)
        );
        assert_eq!(output, [0xa5; 8]);
        let oversized = [0u8; ipv4::MAX_PAYLOAD - HEADER_BYTES + 1];
        assert_eq!(
            encode_echo(Kind::Request, 1, 2, &oversized, &mut output),
            Err(Error::Length)
        );
        assert_eq!(output, [0xa5; 8]);
    }

    #[test]
    fn responder_reverses_addresses_and_preserves_echo_data() {
        assert_eq!(self_test(), Ok(()));
    }

    #[test]
    fn responder_ignores_broadcast_and_unrelated_protocols_without_mutation() {
        let local_mac = mac([2, 0, 0, 0, 0, 1]);
        let remote_mac = mac([2, 0, 0, 0, 0, 2]);
        let mut ip = [0u8; MTU];
        let ip_length = ipv4::encode(
            ipv4::Header {
                identification: 1,
                dont_fragment: true,
                ttl: 64,
                protocol: 17,
                source: [192, 0, 2, 2],
                destination: [192, 0, 2, 1],
            },
            &[1, 2, 3],
            &mut ip,
        )
        .unwrap();

        let mut frame = [0u8; super::super::MAX_FRAME];
        let length = ethernet::encode(
            remote_mac,
            local_mac.bytes(),
            ethernet::IPV4,
            &ip[..ip_length],
            &mut frame,
        )
        .unwrap();
        let mut output = [0xa5; super::super::MAX_FRAME];
        assert_eq!(
            echo_reply_frame(
                local_mac,
                [192, 0, 2, 1],
                &frame[..length],
                &mut output
            ),
            Ok(None)
        );
        assert_eq!(output, [0xa5; super::super::MAX_FRAME]);

        frame[..6].copy_from_slice(&ethernet::BROADCAST);
        assert_eq!(
            echo_reply_frame(
                local_mac,
                [192, 0, 2, 1],
                &frame[..length],
                &mut output
            ),
            Ok(None)
        );
        assert_eq!(output, [0xa5; super::super::MAX_FRAME]);
    }
}
