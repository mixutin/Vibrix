//! Bounded static IPv4 NAT and port-redirection dataplane.
//!
//! Rules map one internal TCP/UDP endpoint to one externally visible endpoint.
//! Translation rewrites real IPv4 transport packets and repairs IPv4 plus
//! TCP/UDP checksums. Options and fragmented IPv4 packets fail closed.

pub const MAX_NAT_RULES: usize = 16;
const IPV4_HEADER_BYTES: usize = 20;
const TCP_MIN_BYTES: usize = 20;
const UDP_BYTES: usize = 8;
const PROTO_TCP: u8 = 6;
const PROTO_UDP: u8 = 17;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Protocol {
    Tcp,
    Udp,
}

impl Protocol {
    const fn number(self) -> u8 {
        match self {
            Self::Tcp => PROTO_TCP,
            Self::Udp => PROTO_UDP,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Endpoint {
    pub address: [u8; 4],
    pub port: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rule {
    pub protocol: Protocol,
    pub inside: Endpoint,
    pub outside: Endpoint,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Direction {
    Outbound,
    Inbound,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Capacity,
    Duplicate,
    InvalidEndpoint,
    Length,
    UnsupportedIpv4,
    Fragmented,
    UnsupportedProtocol,
    NoRule,
    InvalidTransport,
}

pub struct Table {
    rules: [Option<Rule>; MAX_NAT_RULES],
}

impl Table {
    pub const fn new() -> Self {
        Self {
            rules: [None; MAX_NAT_RULES],
        }
    }

    pub fn add(&mut self, rule: Rule) -> Result<(), Error> {
        if rule.inside.port == 0
            || rule.outside.port == 0
            || rule.inside.address == [0; 4]
            || rule.outside.address == [0; 4]
        {
            return Err(Error::InvalidEndpoint);
        }
        if self.rules.iter().flatten().any(|current| {
            current.protocol == rule.protocol
                && (current.inside == rule.inside || current.outside == rule.outside)
        }) {
            return Err(Error::Duplicate);
        }
        let slot = self
            .rules
            .iter()
            .position(Option::is_none)
            .ok_or(Error::Capacity)?;
        self.rules[slot] = Some(rule);
        Ok(())
    }

    pub fn remove(&mut self, rule: Rule) -> Result<(), Error> {
        let slot = self
            .rules
            .iter()
            .position(|current| *current == Some(rule))
            .ok_or(Error::NoRule)?;
        self.rules[slot] = None;
        Ok(())
    }

    fn matching_rule(
        &self,
        protocol: Protocol,
        endpoint: Endpoint,
        direction: Direction,
    ) -> Option<Rule> {
        self.rules.iter().flatten().copied().find(|rule| {
            rule.protocol == protocol
                && match direction {
                    Direction::Outbound => rule.inside == endpoint,
                    Direction::Inbound => rule.outside == endpoint,
                }
        })
    }

    pub fn translate(&self, packet: &mut [u8], direction: Direction) -> Result<Rule, Error> {
        let parsed = parse(packet)?;
        let endpoint = match direction {
            Direction::Outbound => Endpoint {
                address: parsed.source,
                port: parsed.source_port,
            },
            Direction::Inbound => Endpoint {
                address: parsed.destination,
                port: parsed.destination_port,
            },
        };
        let rule = self
            .matching_rule(parsed.protocol, endpoint, direction)
            .ok_or(Error::NoRule)?;

        match direction {
            Direction::Outbound => {
                packet[12..16].copy_from_slice(&rule.outside.address);
                packet[IPV4_HEADER_BYTES..IPV4_HEADER_BYTES + 2]
                    .copy_from_slice(&rule.outside.port.to_be_bytes());
            }
            Direction::Inbound => {
                packet[16..20].copy_from_slice(&rule.inside.address);
                packet[IPV4_HEADER_BYTES + 2..IPV4_HEADER_BYTES + 4]
                    .copy_from_slice(&rule.inside.port.to_be_bytes());
            }
        }

        packet[10] = 0;
        packet[11] = 0;
        let header_checksum = internet_checksum(&packet[..IPV4_HEADER_BYTES]);
        packet[10..12].copy_from_slice(&header_checksum.to_be_bytes());

        let transport = &mut packet[IPV4_HEADER_BYTES..parsed.total_len];
        match parsed.protocol {
            Protocol::Tcp => {
                transport[16] = 0;
                transport[17] = 0;
                let checksum = transport_checksum(
                    &packet[12..16],
                    &packet[16..20],
                    PROTO_TCP,
                    transport,
                );
                transport[16..18].copy_from_slice(&checksum.to_be_bytes());
            }
            Protocol::Udp => {
                let had_checksum = transport[6] != 0 || transport[7] != 0;
                if had_checksum {
                    transport[6] = 0;
                    transport[7] = 0;
                    let mut checksum = transport_checksum(
                        &packet[12..16],
                        &packet[16..20],
                        PROTO_UDP,
                        transport,
                    );
                    if checksum == 0 {
                        checksum = 0xffff;
                    }
                    transport[6..8].copy_from_slice(&checksum.to_be_bytes());
                }
            }
        }
        Ok(rule)
    }
}

impl Default for Table {
    fn default() -> Self {
        Self::new()
    }
}

struct Parsed {
    protocol: Protocol,
    source: [u8; 4],
    destination: [u8; 4],
    source_port: u16,
    destination_port: u16,
    total_len: usize,
}

fn parse(packet: &[u8]) -> Result<Parsed, Error> {
    if packet.len() < IPV4_HEADER_BYTES {
        return Err(Error::Length);
    }
    if packet[0] >> 4 != 4 || packet[0] & 0x0f != 5 {
        return Err(Error::UnsupportedIpv4);
    }
    let total_len = usize::from(u16::from_be_bytes([packet[2], packet[3]]));
    if total_len != packet.len() || total_len < IPV4_HEADER_BYTES {
        return Err(Error::Length);
    }
    let fragment = u16::from_be_bytes([packet[6], packet[7]]);
    if fragment & 0x3fff != 0 {
        return Err(Error::Fragmented);
    }
    let protocol = match packet[9] {
        PROTO_TCP => Protocol::Tcp,
        PROTO_UDP => Protocol::Udp,
        _ => return Err(Error::UnsupportedProtocol),
    };
    let transport = &packet[IPV4_HEADER_BYTES..];
    match protocol {
        Protocol::Tcp if transport.len() < TCP_MIN_BYTES => return Err(Error::InvalidTransport),
        Protocol::Udp if transport.len() < UDP_BYTES => return Err(Error::InvalidTransport),
        _ => {}
    }
    if protocol == Protocol::Udp {
        let udp_len = usize::from(u16::from_be_bytes([transport[4], transport[5]]));
        if udp_len != transport.len() || udp_len < UDP_BYTES {
            return Err(Error::InvalidTransport);
        }
    } else {
        let header_words = transport[12] >> 4;
        if header_words < 5 || usize::from(header_words) * 4 > transport.len() {
            return Err(Error::InvalidTransport);
        }
    }
    let mut source = [0; 4];
    let mut destination = [0; 4];
    source.copy_from_slice(&packet[12..16]);
    destination.copy_from_slice(&packet[16..20]);
    Ok(Parsed {
        protocol,
        source,
        destination,
        source_port: u16::from_be_bytes([transport[0], transport[1]]),
        destination_port: u16::from_be_bytes([transport[2], transport[3]]),
        total_len,
    })
}

fn add_words(mut sum: u32, bytes: &[u8]) -> u32 {
    let mut chunks = bytes.chunks_exact(2);
    for chunk in &mut chunks {
        sum = sum.wrapping_add(u32::from(u16::from_be_bytes([chunk[0], chunk[1]])));
    }
    if let Some(&byte) = chunks.remainder().first() {
        sum = sum.wrapping_add(u32::from(byte) << 8);
    }
    sum
}

fn finish_checksum(mut sum: u32) -> u16 {
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

fn internet_checksum(bytes: &[u8]) -> u16 {
    finish_checksum(add_words(0, bytes))
}

fn transport_checksum(source: &[u8], destination: &[u8], protocol: u8, transport: &[u8]) -> u16 {
    let mut sum = add_words(0, source);
    sum = add_words(sum, destination);
    sum = sum.wrapping_add(u32::from(protocol));
    sum = sum.wrapping_add(transport.len() as u32);
    finish_checksum(add_words(sum, transport))
}

fn build_udp(
    source: Endpoint,
    destination: Endpoint,
    payload: &[u8],
    packet: &mut [u8],
) -> Result<usize, Error> {
    let total = IPV4_HEADER_BYTES + UDP_BYTES + payload.len();
    if packet.len() < total || total > u16::MAX as usize {
        return Err(Error::Length);
    }
    packet[..total].fill(0);
    packet[0] = 0x45;
    packet[2..4].copy_from_slice(&(total as u16).to_be_bytes());
    packet[8] = 64;
    packet[9] = PROTO_UDP;
    packet[12..16].copy_from_slice(&source.address);
    packet[16..20].copy_from_slice(&destination.address);
    packet[20..22].copy_from_slice(&source.port.to_be_bytes());
    packet[22..24].copy_from_slice(&destination.port.to_be_bytes());
    packet[24..26].copy_from_slice(&((UDP_BYTES + payload.len()) as u16).to_be_bytes());
    packet[28..total].copy_from_slice(payload);
    let ip_checksum = internet_checksum(&packet[..IPV4_HEADER_BYTES]);
    packet[10..12].copy_from_slice(&ip_checksum.to_be_bytes());
    let mut udp_checksum = transport_checksum(
        &source.address,
        &destination.address,
        PROTO_UDP,
        &packet[20..total],
    );
    if udp_checksum == 0 {
        udp_checksum = 0xffff;
    }
    packet[26..28].copy_from_slice(&udp_checksum.to_be_bytes());
    Ok(total)
}

pub fn self_test() -> Result<(), Error> {
    let inside = Endpoint {
        address: [10, 0, 0, 2],
        port: 12345,
    };
    let outside = Endpoint {
        address: [192, 0, 2, 10],
        port: 40000,
    };
    let peer = Endpoint {
        address: [198, 51, 100, 7],
        port: 53,
    };
    let rule = Rule {
        protocol: Protocol::Udp,
        inside,
        outside,
    };
    let mut table = Table::new();
    table.add(rule)?;

    let mut packet = [0u8; 128];
    let len = build_udp(inside, peer, b"dns", &mut packet)?;
    table.translate(&mut packet[..len], Direction::Outbound)?;
    let parsed = parse(&packet[..len])?;
    if parsed.source != outside.address || parsed.source_port != outside.port {
        return Err(Error::NoRule);
    }

    let len = build_udp(peer, outside, b"reply", &mut packet)?;
    table.translate(&mut packet[..len], Direction::Inbound)?;
    let parsed = parse(&packet[..len])?;
    if parsed.destination != inside.address || parsed.destination_port != inside.port {
        return Err(Error::NoRule);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoint(a: [u8; 4], port: u16) -> Endpoint {
        Endpoint { address: a, port }
    }

    #[test]
    fn production_self_test_passes() {
        self_test().unwrap();
    }

    #[test]
    fn udp_translation_is_bidirectional_and_repairs_checksums() {
        let inside = endpoint([10, 0, 0, 2], 1111);
        let outside = endpoint([203, 0, 113, 4], 50000);
        let peer = endpoint([198, 51, 100, 9], 9000);
        let rule = Rule {
            protocol: Protocol::Udp,
            inside,
            outside,
        };
        let mut table = Table::new();
        table.add(rule).unwrap();
        let mut packet = [0u8; 96];
        let len = build_udp(inside, peer, b"hello", &mut packet).unwrap();
        table
            .translate(&mut packet[..len], Direction::Outbound)
            .unwrap();
        assert_eq!(&packet[12..16], &outside.address);
        assert_eq!(u16::from_be_bytes([packet[20], packet[21]]), outside.port);
        assert_eq!(internet_checksum(&packet[..20]), 0);
        assert_eq!(
            transport_checksum(&packet[12..16], &packet[16..20], PROTO_UDP, &packet[20..len]),
            0
        );

        let len = build_udp(peer, outside, b"world", &mut packet).unwrap();
        table
            .translate(&mut packet[..len], Direction::Inbound)
            .unwrap();
        assert_eq!(&packet[16..20], &inside.address);
        assert_eq!(u16::from_be_bytes([packet[22], packet[23]]), inside.port);
    }

    #[test]
    fn fragments_options_unknown_protocol_and_unmatched_flows_fail_closed() {
        let inside = endpoint([10, 0, 0, 2], 1111);
        let outside = endpoint([203, 0, 113, 4], 50000);
        let peer = endpoint([198, 51, 100, 9], 9000);
        let mut table = Table::new();
        table
            .add(Rule {
                protocol: Protocol::Udp,
                inside,
                outside,
            })
            .unwrap();
        let mut packet = [0u8; 64];
        let len = build_udp(inside, peer, b"x", &mut packet).unwrap();

        let mut fragmented = packet;
        fragmented[6] = 0x20;
        assert_eq!(
            table.translate(&mut fragmented[..len], Direction::Outbound),
            Err(Error::Fragmented)
        );

        let mut options = packet;
        options[0] = 0x46;
        assert_eq!(
            table.translate(&mut options[..len], Direction::Outbound),
            Err(Error::UnsupportedIpv4)
        );

        let mut unmatched = packet;
        unmatched[20..22].copy_from_slice(&2222u16.to_be_bytes());
        assert_eq!(
            table.translate(&mut unmatched[..len], Direction::Outbound),
            Err(Error::NoRule)
        );
    }

    #[test]
    fn rule_namespace_is_unique_and_bounded() {
        let mut table = Table::new();
        let rule = Rule {
            protocol: Protocol::Tcp,
            inside: endpoint([10, 0, 0, 2], 22),
            outside: endpoint([192, 0, 2, 1], 2222),
        };
        table.add(rule).unwrap();
        assert_eq!(table.add(rule), Err(Error::Duplicate));
        assert_eq!(
            table.add(Rule {
                protocol: Protocol::Tcp,
                inside: endpoint([10, 0, 0, 3], 22),
                outside: rule.outside,
            }),
            Err(Error::Duplicate)
        );
    }
}
