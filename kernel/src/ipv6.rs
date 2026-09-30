//! Bounded IPv6, ICMPv6 and Neighbor Discovery core.
//!
//! This module intentionally implements only fixed-header IPv6 packets with
//! ICMPv6 payloads. Extension headers, fragmentation, routing and multicast
//! membership management remain separate work.

pub const IPV6_HEADER_BYTES: usize = 40;
pub const ICMPV6_NEXT_HEADER: u8 = 58;
pub const ICMPV6_ECHO_REQUEST: u8 = 128;
pub const ICMPV6_ECHO_REPLY: u8 = 129;
pub const ICMPV6_NEIGHBOR_SOLICITATION: u8 = 135;
pub const ICMPV6_NEIGHBOR_ADVERTISEMENT: u8 = 136;
pub const NDP_HOP_LIMIT: u8 = 255;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Length,
    Version,
    UnsupportedNextHeader,
    Checksum,
    InvalidIcmp,
    InvalidNeighborDiscovery,
    BufferTooSmall,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Header {
    pub source: [u8; 16],
    pub destination: [u8; 16],
    pub payload_len: u16,
    pub next_header: u8,
    pub hop_limit: u8,
}

impl Header {
    pub fn parse(packet: &[u8]) -> Result<(Self, &[u8]), Error> {
        if packet.len() < IPV6_HEADER_BYTES {
            return Err(Error::Length);
        }
        if packet[0] >> 4 != 6 {
            return Err(Error::Version);
        }
        let payload_len = u16::from_be_bytes([packet[4], packet[5]]);
        let end = IPV6_HEADER_BYTES
            .checked_add(usize::from(payload_len))
            .ok_or(Error::Length)?;
        if end != packet.len() {
            return Err(Error::Length);
        }
        let mut source = [0; 16];
        let mut destination = [0; 16];
        source.copy_from_slice(&packet[8..24]);
        destination.copy_from_slice(&packet[24..40]);
        Ok((
            Self {
                source,
                destination,
                payload_len,
                next_header: packet[6],
                hop_limit: packet[7],
            },
            &packet[IPV6_HEADER_BYTES..end],
        ))
    }

    pub fn write(&self, packet: &mut [u8]) -> Result<(), Error> {
        if packet.len() < IPV6_HEADER_BYTES {
            return Err(Error::BufferTooSmall);
        }
        packet[..IPV6_HEADER_BYTES].fill(0);
        packet[0] = 0x60;
        packet[4..6].copy_from_slice(&self.payload_len.to_be_bytes());
        packet[6] = self.next_header;
        packet[7] = self.hop_limit;
        packet[8..24].copy_from_slice(&self.source);
        packet[24..40].copy_from_slice(&self.destination);
        Ok(())
    }
}

fn sum_words(mut sum: u32, bytes: &[u8]) -> u32 {
    let mut chunks = bytes.chunks_exact(2);
    for chunk in &mut chunks {
        sum = sum.wrapping_add(u32::from(u16::from_be_bytes([chunk[0], chunk[1]])));
    }
    if let Some(&last) = chunks.remainder().first() {
        sum = sum.wrapping_add(u32::from(last) << 8);
    }
    sum
}

fn fold_checksum(mut sum: u32) -> u16 {
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

pub fn icmpv6_checksum(source: &[u8; 16], destination: &[u8; 16], payload: &[u8]) -> u16 {
    let mut sum = sum_words(0, source);
    sum = sum_words(sum, destination);
    let length = (payload.len() as u32).to_be_bytes();
    sum = sum_words(sum, &length);
    sum = sum_words(sum, &[0, 0, 0, ICMPV6_NEXT_HEADER]);
    sum = sum_words(sum, payload);
    fold_checksum(sum)
}

fn verify_icmpv6(header: &Header, payload: &[u8]) -> Result<(), Error> {
    if header.next_header != ICMPV6_NEXT_HEADER {
        return Err(Error::UnsupportedNextHeader);
    }
    if payload.len() < 4 {
        return Err(Error::InvalidIcmp);
    }
    if icmpv6_checksum(&header.source, &header.destination, payload) != 0 {
        return Err(Error::Checksum);
    }
    Ok(())
}

/// Turn an ICMPv6 Echo Request into an Echo Reply.
pub fn echo_reply(request: &[u8], output: &mut [u8]) -> Result<usize, Error> {
    let (header, payload) = Header::parse(request)?;
    verify_icmpv6(&header, payload)?;
    if payload[0] != ICMPV6_ECHO_REQUEST || payload[1] != 0 || payload.len() < 8 {
        return Err(Error::InvalidIcmp);
    }
    let total = IPV6_HEADER_BYTES + payload.len();
    if output.len() < total {
        return Err(Error::BufferTooSmall);
    }
    let reply_header = Header {
        source: header.destination,
        destination: header.source,
        payload_len: header.payload_len,
        next_header: ICMPV6_NEXT_HEADER,
        hop_limit: 64,
    };
    reply_header.write(output)?;
    output[IPV6_HEADER_BYTES..total].copy_from_slice(payload);
    output[IPV6_HEADER_BYTES] = ICMPV6_ECHO_REPLY;
    output[IPV6_HEADER_BYTES + 2] = 0;
    output[IPV6_HEADER_BYTES + 3] = 0;
    let checksum = icmpv6_checksum(
        &reply_header.source,
        &reply_header.destination,
        &output[IPV6_HEADER_BYTES..total],
    );
    output[IPV6_HEADER_BYTES + 2..IPV6_HEADER_BYTES + 4]
        .copy_from_slice(&checksum.to_be_bytes());
    Ok(total)
}

/// Validate a Neighbor Solicitation and emit a solicited Neighbor Advertisement.
///
/// The caller supplies the local link-layer address because this core does not
/// own a NIC. The response includes one Target Link-Layer Address option.
pub fn neighbor_advertisement(
    request: &[u8],
    local: [u8; 16],
    local_mac: [u8; 6],
    output: &mut [u8],
) -> Result<usize, Error> {
    let (header, payload) = Header::parse(request)?;
    verify_icmpv6(&header, payload)?;
    if header.hop_limit != NDP_HOP_LIMIT
        || payload.len() < 24
        || payload[0] != ICMPV6_NEIGHBOR_SOLICITATION
        || payload[1] != 0
    {
        return Err(Error::InvalidNeighborDiscovery);
    }
    let mut target = [0; 16];
    target.copy_from_slice(&payload[8..24]);
    if target != local || local.iter().all(|&byte| byte == 0) {
        return Err(Error::InvalidNeighborDiscovery);
    }

    const NA_BYTES: usize = 32;
    let total = IPV6_HEADER_BYTES + NA_BYTES;
    if output.len() < total {
        return Err(Error::BufferTooSmall);
    }
    let response_header = Header {
        source: local,
        destination: header.source,
        payload_len: NA_BYTES as u16,
        next_header: ICMPV6_NEXT_HEADER,
        hop_limit: NDP_HOP_LIMIT,
    };
    response_header.write(output)?;
    let body = &mut output[IPV6_HEADER_BYTES..total];
    body.fill(0);
    body[0] = ICMPV6_NEIGHBOR_ADVERTISEMENT;
    body[4] = 0x60; // solicited + override
    body[8..24].copy_from_slice(&local);
    body[24] = 2; // Target Link-Layer Address
    body[25] = 1; // 8 bytes
    body[26..32].copy_from_slice(&local_mac);
    let checksum = icmpv6_checksum(&response_header.source, &response_header.destination, body);
    body[2..4].copy_from_slice(&checksum.to_be_bytes());
    Ok(total)
}

pub fn self_test() -> Result<(), Error> {
    let host = [0x20, 1, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1];
    let peer = [0x20, 1, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2];

    let mut echo = [0u8; 48];
    Header {
        source: peer,
        destination: host,
        payload_len: 8,
        next_header: ICMPV6_NEXT_HEADER,
        hop_limit: 32,
    }
    .write(&mut echo)?;
    echo[40] = ICMPV6_ECHO_REQUEST;
    echo[44..48].copy_from_slice(&[0x12, 0x34, 0, 1]);
    let checksum = icmpv6_checksum(&peer, &host, &echo[40..]);
    echo[42..44].copy_from_slice(&checksum.to_be_bytes());
    let mut reply = [0u8; 80];
    let len = echo_reply(&echo, &mut reply)?;
    let (reply_header, reply_payload) = Header::parse(&reply[..len])?;
    verify_icmpv6(&reply_header, reply_payload)?;
    if reply_header.source != host
        || reply_header.destination != peer
        || reply_payload[0] != ICMPV6_ECHO_REPLY
    {
        return Err(Error::InvalidIcmp);
    }

    let mut ns = [0u8; 64];
    Header {
        source: peer,
        destination: host,
        payload_len: 24,
        next_header: ICMPV6_NEXT_HEADER,
        hop_limit: NDP_HOP_LIMIT,
    }
    .write(&mut ns)?;
    ns[40] = ICMPV6_NEIGHBOR_SOLICITATION;
    ns[48..64].copy_from_slice(&host);
    let checksum = icmpv6_checksum(&peer, &host, &ns[40..]);
    ns[42..44].copy_from_slice(&checksum.to_be_bytes());
    let len = neighbor_advertisement(&ns, host, [2, 0, 0, 0, 0, 1], &mut reply)?;
    let (na_header, na) = Header::parse(&reply[..len])?;
    verify_icmpv6(&na_header, na)?;
    if na[0] != ICMPV6_NEIGHBOR_ADVERTISEMENT || &na[8..24] != host.as_slice() {
        return Err(Error::InvalidNeighborDiscovery);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_self_test_passes() {
        self_test().unwrap();
    }

    #[test]
    fn parser_rejects_bad_version_length_and_next_header() {
        let mut packet = [0u8; 40];
        assert_eq!(Header::parse(&packet), Err(Error::Version));
        packet[0] = 0x60;
        packet[4..6].copy_from_slice(&1u16.to_be_bytes());
        assert_eq!(Header::parse(&packet), Err(Error::Length));
    }

    #[test]
    fn ndp_requires_hop_limit_255_and_exact_local_target() {
        let host = [1u8; 16];
        let peer = [2u8; 16];
        let mut packet = [0u8; 64];
        let header = Header {
            source: peer,
            destination: host,
            payload_len: 24,
            next_header: ICMPV6_NEXT_HEADER,
            hop_limit: 64,
        };
        header.write(&mut packet).unwrap();
        packet[40] = ICMPV6_NEIGHBOR_SOLICITATION;
        packet[48..64].copy_from_slice(&host);
        let checksum = icmpv6_checksum(&peer, &host, &packet[40..]);
        packet[42..44].copy_from_slice(&checksum.to_be_bytes());
        assert_eq!(
            neighbor_advertisement(&packet, host, [0; 6], &mut [0u8; 80]),
            Err(Error::InvalidNeighborDiscovery)
        );
    }

    #[test]
    fn corrupt_icmpv6_checksum_fails_closed() {
        let host = [1u8; 16];
        let peer = [2u8; 16];
        let mut packet = [0u8; 48];
        Header {
            source: peer,
            destination: host,
            payload_len: 8,
            next_header: ICMPV6_NEXT_HEADER,
            hop_limit: 64,
        }
        .write(&mut packet)
        .unwrap();
        packet[40] = ICMPV6_ECHO_REQUEST;
        assert_eq!(echo_reply(&packet, &mut [0u8; 80]), Err(Error::Checksum));
    }
}
