//! Bounded DHCPv4 client state and wire handling.
//!
//! This implements RFC 2131 client-side discovery, selecting, requesting,
//! bound, renewing and rebinding policy over the existing UDP layer. It owns no
//! clock, socket, route, NIC or random source: callers provide the transaction
//! identifier, elapsed lease age and transport.

use super::{MacAddress, udp};

pub const CLIENT_PORT: u16 = 68;
pub const SERVER_PORT: u16 = 67;
pub const FIXED_BYTES: usize = 236;
pub const COOKIE_BYTES: usize = 4;
pub const OPTIONS_OFFSET: usize = FIXED_BYTES + COOKIE_BYTES;
pub const MIN_MESSAGE_BYTES: usize = OPTIONS_OFFSET + 4;
pub const MAX_MESSAGE_BYTES: usize = udp::MAX_PAYLOAD;

const BOOTREQUEST: u8 = 1;
const BOOTREPLY: u8 = 2;
const HTYPE_ETHERNET: u8 = 1;
const HLEN_ETHERNET: u8 = 6;
const BROADCAST_FLAG: u16 = 0x8000;
const MAGIC_COOKIE: [u8; 4] = [99, 130, 83, 99];

const OPTION_PAD: u8 = 0;
const OPTION_SUBNET_MASK: u8 = 1;
const OPTION_ROUTER: u8 = 3;
const OPTION_DNS: u8 = 6;
const OPTION_REQUESTED_IP: u8 = 50;
const OPTION_LEASE_TIME: u8 = 51;
const OPTION_OVERLOAD: u8 = 52;
const OPTION_MESSAGE_TYPE: u8 = 53;
const OPTION_SERVER_IDENTIFIER: u8 = 54;
const OPTION_PARAMETER_REQUEST_LIST: u8 = 55;
const OPTION_RENEWAL_TIME: u8 = 58;
const OPTION_REBINDING_TIME: u8 = 59;
const OPTION_END: u8 = 255;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum MessageType {
    Discover = 1,
    Offer = 2,
    Request = 3,
    Decline = 4,
    Ack = 5,
    Nak = 6,
    Release = 7,
    Inform = 8,
}

impl MessageType {
    fn parse(value: u8) -> Result<Self, Error> {
        match value {
            1 => Ok(Self::Discover),
            2 => Ok(Self::Offer),
            3 => Ok(Self::Request),
            4 => Ok(Self::Decline),
            5 => Ok(Self::Ack),
            6 => Ok(Self::Nak),
            7 => Ok(Self::Release),
            8 => Ok(Self::Inform),
            _ => Err(Error::Option),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Length,
    Header,
    Cookie,
    Option,
    OptionOverload,
    MissingOption,
    Transaction,
    Client,
    State,
    Lease,
    OutputTooSmall,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Offer {
    pub address: [u8; 4],
    pub server: [u8; 4],
    pub lease_seconds: u32,
    pub subnet_mask: Option<[u8; 4]>,
    pub router: Option<[u8; 4]>,
    pub dns: Option<[u8; 4]>,
    pub renewal_seconds: u32,
    pub rebinding_seconds: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Lease {
    pub address: [u8; 4],
    pub server: [u8; 4],
    pub lease_seconds: u32,
    pub renewal_seconds: u32,
    pub rebinding_seconds: u32,
    pub subnet_mask: Option<[u8; 4]>,
    pub router: Option<[u8; 4]>,
    pub dns: Option<[u8; 4]>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum State {
    Init,
    Selecting,
    Requesting {
        address: [u8; 4],
        server: [u8; 4],
    },
    Bound(Lease),
    Renewing(Lease),
    Rebinding(Lease),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Event {
    Bound(Lease),
    Restart,
    Expired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Reply {
    message_type: MessageType,
    xid: u32,
    address: [u8; 4],
    client_mac: [u8; 6],
    server: Option<[u8; 4]>,
    lease_seconds: Option<u32>,
    renewal_seconds: Option<u32>,
    rebinding_seconds: Option<u32>,
    subnet_mask: Option<[u8; 4]>,
    router: Option<[u8; 4]>,
    dns: Option<[u8; 4]>,
}

fn read_u32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

fn option_ipv4(value: &[u8]) -> Result<[u8; 4], Error> {
    value.try_into().map_err(|_| Error::Option)
}

fn set_once<T: Copy>(slot: &mut Option<T>, value: T) -> Result<(), Error> {
    if slot.replace(value).is_some() {
        return Err(Error::Option);
    }
    Ok(())
}

fn parse_reply(input: &[u8]) -> Result<Reply, Error> {
    if input.len() < MIN_MESSAGE_BYTES || input.len() > MAX_MESSAGE_BYTES {
        return Err(Error::Length);
    }
    if input[0] != BOOTREPLY || input[1] != HTYPE_ETHERNET || input[2] != HLEN_ETHERNET {
        return Err(Error::Header);
    }
    if input[236..240] != MAGIC_COOKIE {
        return Err(Error::Cookie);
    }

    let xid = read_u32(&input[4..8]);
    let address = option_ipv4(&input[16..20])?;
    let mut client_mac = [0u8; 6];
    client_mac.copy_from_slice(&input[28..34]);

    let mut message_type = None;
    let mut server = None;
    let mut lease_seconds = None;
    let mut renewal_seconds = None;
    let mut rebinding_seconds = None;
    let mut subnet_mask = None;
    let mut router = None;
    let mut dns = None;
    let mut cursor = OPTIONS_OFFSET;
    let mut end_seen = false;

    while cursor < input.len() {
        let code = input[cursor];
        cursor += 1;
        match code {
            OPTION_PAD => continue,
            OPTION_END => {
                end_seen = true;
                break;
            }
            _ => {}
        }
        let length = *input.get(cursor).ok_or(Error::Option)? as usize;
        cursor += 1;
        let end = cursor.checked_add(length).ok_or(Error::Option)?;
        let value = input.get(cursor..end).ok_or(Error::Option)?;
        cursor = end;

        match code {
            OPTION_MESSAGE_TYPE => {
                if value.len() != 1 {
                    return Err(Error::Option);
                }
                set_once(&mut message_type, MessageType::parse(value[0])?)?;
            }
            OPTION_SERVER_IDENTIFIER => set_once(&mut server, option_ipv4(value)?)?,
            OPTION_LEASE_TIME => {
                if value.len() != 4 {
                    return Err(Error::Option);
                }
                set_once(&mut lease_seconds, read_u32(value))?;
            }
            OPTION_RENEWAL_TIME => {
                if value.len() != 4 {
                    return Err(Error::Option);
                }
                set_once(&mut renewal_seconds, read_u32(value))?;
            }
            OPTION_REBINDING_TIME => {
                if value.len() != 4 {
                    return Err(Error::Option);
                }
                set_once(&mut rebinding_seconds, read_u32(value))?;
            }
            OPTION_SUBNET_MASK => set_once(&mut subnet_mask, option_ipv4(value)?)?,
            OPTION_ROUTER => {
                if value.len() < 4 || !value.len().is_multiple_of(4) {
                    return Err(Error::Option);
                }
                set_once(&mut router, option_ipv4(&value[..4])?)?;
            }
            OPTION_DNS => {
                if value.len() < 4 || !value.len().is_multiple_of(4) {
                    return Err(Error::Option);
                }
                set_once(&mut dns, option_ipv4(&value[..4])?)?;
            }
            OPTION_OVERLOAD => return Err(Error::OptionOverload),
            _ => {}
        }
    }

    if !end_seen {
        return Err(Error::Option);
    }
    let message_type = message_type.ok_or(Error::MissingOption)?;
    Ok(Reply {
        message_type,
        xid,
        address,
        client_mac,
        server,
        lease_seconds,
        renewal_seconds,
        rebinding_seconds,
        subnet_mask,
        router,
        dns,
    })
}

fn lease_times(
    lease_seconds: u32,
    renewal_seconds: Option<u32>,
    rebinding_seconds: Option<u32>,
) -> Result<(u32, u32), Error> {
    if lease_seconds < 4 {
        return Err(Error::Lease);
    }
    let renewal = renewal_seconds.unwrap_or(lease_seconds / 2);
    let rebinding = rebinding_seconds.unwrap_or(lease_seconds.saturating_mul(7) / 8);
    if renewal == 0 || renewal >= rebinding || rebinding >= lease_seconds {
        return Err(Error::Lease);
    }
    Ok((renewal, rebinding))
}

fn reply_offer(reply: Reply) -> Result<Offer, Error> {
    if reply.message_type != MessageType::Offer || reply.address == [0; 4] {
        return Err(Error::State);
    }
    let server = reply.server.ok_or(Error::MissingOption)?;
    let lease_seconds = reply.lease_seconds.ok_or(Error::MissingOption)?;
    let (renewal_seconds, rebinding_seconds) =
        lease_times(lease_seconds, reply.renewal_seconds, reply.rebinding_seconds)?;
    Ok(Offer {
        address: reply.address,
        server,
        lease_seconds,
        subnet_mask: reply.subnet_mask,
        router: reply.router,
        dns: reply.dns,
        renewal_seconds,
        rebinding_seconds,
    })
}

fn append_option(
    output: &mut [u8],
    cursor: &mut usize,
    code: u8,
    value: &[u8],
) -> Result<(), Error> {
    let length = u8::try_from(value.len()).map_err(|_| Error::Length)?;
    let end = cursor
        .checked_add(2 + value.len())
        .ok_or(Error::OutputTooSmall)?;
    if end > output.len() {
        return Err(Error::OutputTooSmall);
    }
    output[*cursor] = code;
    output[*cursor + 1] = length;
    output[*cursor + 2..end].copy_from_slice(value);
    *cursor = end;
    Ok(())
}

fn encode_request(
    xid: u32,
    mac: MacAddress,
    ciaddr: [u8; 4],
    broadcast: bool,
    message_type: MessageType,
    requested: Option<[u8; 4]>,
    server: Option<[u8; 4]>,
    output: &mut [u8],
) -> Result<usize, Error> {
    // Worst-case options used here: message type, requested IP, server ID,
    // parameter request list and END.
    const REQUIRED_CAPACITY: usize = OPTIONS_OFFSET + 3 + 6 + 6 + 7 + 1;
    if output.len() < REQUIRED_CAPACITY {
        return Err(Error::OutputTooSmall);
    }

    output[..REQUIRED_CAPACITY].fill(0);
    output[0] = BOOTREQUEST;
    output[1] = HTYPE_ETHERNET;
    output[2] = HLEN_ETHERNET;
    output[4..8].copy_from_slice(&xid.to_be_bytes());
    let flags = if broadcast { BROADCAST_FLAG } else { 0 };
    output[10..12].copy_from_slice(&flags.to_be_bytes());
    output[12..16].copy_from_slice(&ciaddr);
    output[28..34].copy_from_slice(&mac.bytes());
    output[236..240].copy_from_slice(&MAGIC_COOKIE);

    let mut cursor = OPTIONS_OFFSET;
    append_option(
        output,
        &mut cursor,
        OPTION_MESSAGE_TYPE,
        &[message_type as u8],
    )?;
    if let Some(address) = requested {
        append_option(output, &mut cursor, OPTION_REQUESTED_IP, &address)?;
    }
    if let Some(server) = server {
        append_option(output, &mut cursor, OPTION_SERVER_IDENTIFIER, &server)?;
    }
    append_option(
        output,
        &mut cursor,
        OPTION_PARAMETER_REQUEST_LIST,
        &[
            OPTION_SUBNET_MASK,
            OPTION_ROUTER,
            OPTION_DNS,
            OPTION_RENEWAL_TIME,
            OPTION_REBINDING_TIME,
        ],
    )?;
    output[cursor] = OPTION_END;
    cursor += 1;
    Ok(cursor)
}

pub struct Client {
    xid: u32,
    mac: MacAddress,
    state: State,
}

impl Client {
    pub const fn new(xid: u32, mac: MacAddress) -> Self {
        Self {
            xid,
            mac,
            state: State::Init,
        }
    }

    pub const fn state(&self) -> State {
        self.state
    }

    pub fn discover(&mut self, output: &mut [u8]) -> Result<usize, Error> {
        if self.state != State::Init {
            return Err(Error::State);
        }
        let length = encode_request(
            self.xid,
            self.mac,
            [0; 4],
            true,
            MessageType::Discover,
            None,
            None,
            output,
        )?;
        self.state = State::Selecting;
        Ok(length)
    }

    pub fn receive_offer(&self, input: &[u8]) -> Result<Offer, Error> {
        if self.state != State::Selecting {
            return Err(Error::State);
        }
        let reply = self.checked_reply(input)?;
        reply_offer(reply)
    }

    pub fn request_offer(&mut self, offer: Offer, output: &mut [u8]) -> Result<usize, Error> {
        if self.state != State::Selecting {
            return Err(Error::State);
        }
        let length = encode_request(
            self.xid,
            self.mac,
            [0; 4],
            true,
            MessageType::Request,
            Some(offer.address),
            Some(offer.server),
            output,
        )?;
        self.state = State::Requesting {
            address: offer.address,
            server: offer.server,
        };
        Ok(length)
    }

    pub fn handle_reply(&mut self, input: &[u8]) -> Result<Event, Error> {
        let reply = self.checked_reply(input)?;
        match (self.state, reply.message_type) {
            (State::Requesting { address, server }, MessageType::Ack) => {
                if reply.address != address || reply.server != Some(server) {
                    return Err(Error::Client);
                }
                let lease = Self::lease_from_reply(reply, address)?;
                self.state = State::Bound(lease);
                Ok(Event::Bound(lease))
            }
            (State::Requesting { .. }, MessageType::Nak)
            | (State::Renewing(_), MessageType::Nak)
            | (State::Rebinding(_), MessageType::Nak) => {
                self.state = State::Init;
                Ok(Event::Restart)
            }
            (State::Renewing(current), MessageType::Ack) => {
                if reply.server != Some(current.server) {
                    return Err(Error::Client);
                }
                let lease = Self::lease_from_reply(reply, current.address)?;
                self.state = State::Bound(lease);
                Ok(Event::Bound(lease))
            }
            (State::Rebinding(current), MessageType::Ack) => {
                let lease = Self::lease_from_reply(reply, current.address)?;
                self.state = State::Bound(lease);
                Ok(Event::Bound(lease))
            }
            _ => Err(Error::State),
        }
    }

    pub fn advance_lease_age(&mut self, age_seconds: u32) -> Result<Option<Event>, Error> {
        let lease = match self.state {
            State::Bound(lease) | State::Renewing(lease) | State::Rebinding(lease) => lease,
            _ => return Err(Error::State),
        };
        if age_seconds >= lease.lease_seconds {
            self.state = State::Init;
            return Ok(Some(Event::Expired));
        }
        if age_seconds >= lease.rebinding_seconds {
            self.state = State::Rebinding(lease);
        } else if age_seconds >= lease.renewal_seconds {
            self.state = State::Renewing(lease);
        } else {
            self.state = State::Bound(lease);
        }
        Ok(None)
    }

    pub fn renewal_request(&self, output: &mut [u8]) -> Result<usize, Error> {
        match self.state {
            State::Renewing(lease) => encode_request(
                self.xid,
                self.mac,
                lease.address,
                false,
                MessageType::Request,
                None,
                None,
                output,
            ),
            State::Rebinding(lease) => encode_request(
                self.xid,
                self.mac,
                lease.address,
                true,
                MessageType::Request,
                None,
                None,
                output,
            ),
            _ => Err(Error::State),
        }
    }

    fn checked_reply(&self, input: &[u8]) -> Result<Reply, Error> {
        let reply = parse_reply(input)?;
        if reply.xid != self.xid {
            return Err(Error::Transaction);
        }
        if reply.client_mac != self.mac.bytes() {
            return Err(Error::Client);
        }
        Ok(reply)
    }

    fn lease_from_reply(reply: Reply, fallback_address: [u8; 4]) -> Result<Lease, Error> {
        let address = if reply.address == [0; 4] {
            fallback_address
        } else {
            reply.address
        };
        if address == [0; 4] {
            return Err(Error::Lease);
        }
        let server = reply.server.ok_or(Error::MissingOption)?;
        let lease_seconds = reply.lease_seconds.ok_or(Error::MissingOption)?;
        let (renewal_seconds, rebinding_seconds) =
            lease_times(lease_seconds, reply.renewal_seconds, reply.rebinding_seconds)?;
        Ok(Lease {
            address,
            server,
            lease_seconds,
            renewal_seconds,
            rebinding_seconds,
            subnet_mask: reply.subnet_mask,
            router: reply.router,
            dns: reply.dns,
        })
    }
}

fn server_reply_fixture(
    xid: u32,
    mac: MacAddress,
    message_type: MessageType,
    address: [u8; 4],
    server: [u8; 4],
    lease_seconds: Option<u32>,
    output: &mut [u8],
) -> Result<usize, Error> {
    const CAPACITY: usize = OPTIONS_OFFSET + 3 + 7 * 6 + 1;
    if output.len() < CAPACITY {
        return Err(Error::OutputTooSmall);
    }
    output[..CAPACITY].fill(0);
    output[0] = BOOTREPLY;
    output[1] = HTYPE_ETHERNET;
    output[2] = HLEN_ETHERNET;
    output[4..8].copy_from_slice(&xid.to_be_bytes());
    output[16..20].copy_from_slice(&address);
    output[28..34].copy_from_slice(&mac.bytes());
    output[236..240].copy_from_slice(&MAGIC_COOKIE);
    let mut cursor = OPTIONS_OFFSET;
    append_option(
        output,
        &mut cursor,
        OPTION_MESSAGE_TYPE,
        &[message_type as u8],
    )?;
    append_option(output, &mut cursor, OPTION_SERVER_IDENTIFIER, &server)?;
    if let Some(lease) = lease_seconds {
        append_option(output, &mut cursor, OPTION_LEASE_TIME, &lease.to_be_bytes())?;
        append_option(
            output,
            &mut cursor,
            OPTION_RENEWAL_TIME,
            &(lease / 2).to_be_bytes(),
        )?;
        append_option(
            output,
            &mut cursor,
            OPTION_REBINDING_TIME,
            &(lease.saturating_mul(7) / 8).to_be_bytes(),
        )?;
        append_option(output, &mut cursor, OPTION_SUBNET_MASK, &[255, 255, 255, 0])?;
        append_option(output, &mut cursor, OPTION_ROUTER, &[192, 0, 2, 1])?;
        append_option(output, &mut cursor, OPTION_DNS, &[192, 0, 2, 53])?;
    }
    output[cursor] = OPTION_END;
    cursor += 1;
    Ok(cursor)
}

pub(super) fn self_test() -> Result<(), Error> {
    let mac = MacAddress::new([2, 0, 0, 0, 0, 1]).map_err(|_| Error::Client)?;
    let xid = 0x5642_5258;
    let mut client = Client::new(xid, mac);
    let mut message = [0u8; MAX_MESSAGE_BYTES];

    let discover = client.discover(&mut message)?;
    if discover < MIN_MESSAGE_BYTES || client.state() != State::Selecting {
        return Err(Error::State);
    }

    let offered = [192, 0, 2, 20];
    let server = [192, 0, 2, 1];
    let offer_length = server_reply_fixture(
        xid,
        mac,
        MessageType::Offer,
        offered,
        server,
        Some(3600),
        &mut message,
    )?;
    let offer = client.receive_offer(&message[..offer_length])?;
    if offer.address != offered || offer.server != server || offer.lease_seconds != 3600 {
        return Err(Error::Lease);
    }

    client.request_offer(offer, &mut message)?;
    let ack_length = server_reply_fixture(
        xid,
        mac,
        MessageType::Ack,
        offered,
        server,
        Some(3600),
        &mut message,
    )?;
    let Event::Bound(lease) = client.handle_reply(&message[..ack_length])? else {
        return Err(Error::State);
    };
    if lease.address != offered || lease.renewal_seconds != 1800 || lease.rebinding_seconds != 3150 {
        return Err(Error::Lease);
    }

    client.advance_lease_age(1800)?;
    if !matches!(client.state(), State::Renewing(_)) {
        return Err(Error::State);
    }
    client.renewal_request(&mut message)?;
    client.advance_lease_age(3150)?;
    if !matches!(client.state(), State::Rebinding(_)) {
        return Err(Error::State);
    }
    client.renewal_request(&mut message)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client() -> (Client, MacAddress) {
        let mac = MacAddress::new([2, 0, 0, 0, 0, 1]).unwrap();
        (Client::new(0x1234_5678, mac), mac)
    }

    #[test]
    fn discover_offer_request_ack_reaches_bound() {
        let (mut client, mac) = client();
        let mut bytes = [0xa5; MAX_MESSAGE_BYTES];
        let discover_length = client.discover(&mut bytes).unwrap();
        assert_eq!(bytes[0], BOOTREQUEST);
        assert_eq!(&bytes[4..8], &0x1234_5678u32.to_be_bytes());
        assert_eq!(&bytes[28..34], &mac.bytes());
        assert_eq!(&bytes[236..240], &MAGIC_COOKIE);
        assert_eq!(bytes[240..243], [OPTION_MESSAGE_TYPE, 1, 1]);
        assert!(discover_length >= MIN_MESSAGE_BYTES);

        let offer_length = server_reply_fixture(
            0x1234_5678,
            mac,
            MessageType::Offer,
            [10, 0, 0, 20],
            [10, 0, 0, 1],
            Some(7200),
            &mut bytes,
        )
        .unwrap();
        let offer = client.receive_offer(&bytes[..offer_length]).unwrap();
        assert_eq!(offer.address, [10, 0, 0, 20]);
        assert_eq!(offer.server, [10, 0, 0, 1]);
        assert_eq!(offer.renewal_seconds, 3600);
        assert_eq!(offer.rebinding_seconds, 6300);

        client.request_offer(offer, &mut bytes).unwrap();
        assert_eq!(
            client.state(),
            State::Requesting {
                address: [10, 0, 0, 20],
                server: [10, 0, 0, 1]
            }
        );

        let ack_length = server_reply_fixture(
            0x1234_5678,
            mac,
            MessageType::Ack,
            [10, 0, 0, 20],
            [10, 0, 0, 1],
            Some(7200),
            &mut bytes,
        )
        .unwrap();
        let Event::Bound(lease) = client.handle_reply(&bytes[..ack_length]).unwrap() else {
            panic!("ACK did not bind lease");
        };
        assert_eq!(lease.subnet_mask, Some([255, 255, 255, 0]));
        assert_eq!(lease.router, Some([192, 0, 2, 1]));
        assert_eq!(lease.dns, Some([192, 0, 2, 53]));
    }

    #[test]
    fn lease_age_enters_renewing_rebinding_then_expires() {
        let lease = Lease {
            address: [192, 0, 2, 2],
            server: [192, 0, 2, 1],
            lease_seconds: 80,
            renewal_seconds: 40,
            rebinding_seconds: 70,
            subnet_mask: None,
            router: None,
            dns: None,
        };
        let (mut client, _) = client();
        client.state = State::Bound(lease);

        assert_eq!(client.advance_lease_age(39), Ok(None));
        assert_eq!(client.state(), State::Bound(lease));
        assert_eq!(client.advance_lease_age(40), Ok(None));
        assert_eq!(client.state(), State::Renewing(lease));
        let mut output = [0u8; MAX_MESSAGE_BYTES];
        let length = client.renewal_request(&mut output).unwrap();
        assert_eq!(output[10..12], [0, 0]);
        assert_eq!(output[12..16], lease.address);
        assert!(length >= MIN_MESSAGE_BYTES);

        assert_eq!(client.advance_lease_age(70), Ok(None));
        assert_eq!(client.state(), State::Rebinding(lease));
        client.renewal_request(&mut output).unwrap();
        assert_eq!(output[10..12], BROADCAST_FLAG.to_be_bytes());

        assert_eq!(client.advance_lease_age(80), Ok(Some(Event::Expired)));
        assert_eq!(client.state(), State::Init);
    }

    #[test]
    fn nak_restarts_requesting_client() {
        let (mut client, mac) = client();
        let mut bytes = [0u8; MAX_MESSAGE_BYTES];
        client.state = State::Requesting {
            address: [192, 0, 2, 2],
            server: [192, 0, 2, 1],
        };
        let length = server_reply_fixture(
            0x1234_5678,
            mac,
            MessageType::Nak,
            [0; 4],
            [192, 0, 2, 1],
            None,
            &mut bytes,
        )
        .unwrap();
        assert_eq!(client.handle_reply(&bytes[..length]), Ok(Event::Restart));
        assert_eq!(client.state(), State::Init);
    }

    #[test]
    fn transaction_client_and_overload_mismatches_fail_closed() {
        let (mut client, mac) = client();
        client.state = State::Selecting;
        let mut bytes = [0u8; MAX_MESSAGE_BYTES];
        let length = server_reply_fixture(
            0x1234_5678,
            mac,
            MessageType::Offer,
            [192, 0, 2, 2],
            [192, 0, 2, 1],
            Some(3600),
            &mut bytes,
        )
        .unwrap();

        let mut bad = bytes;
        bad[4] ^= 1;
        assert_eq!(client.receive_offer(&bad[..length]), Err(Error::Transaction));

        let mut bad = bytes;
        bad[28] ^= 2;
        assert_eq!(client.receive_offer(&bad[..length]), Err(Error::Client));

        let mut overloaded = bytes;
        let end = overloaded[..length]
            .iter()
            .rposition(|byte| *byte == OPTION_END)
            .unwrap();
        overloaded[end] = OPTION_OVERLOAD;
        overloaded[end + 1] = 1;
        overloaded[end + 2] = 1;
        overloaded[end + 3] = OPTION_END;
        assert_eq!(
            client.receive_offer(&overloaded[..end + 4]),
            Err(Error::OptionOverload)
        );
    }

    #[test]
    fn short_output_and_bad_lease_are_transactional() {
        let (mut client, mac) = client();
        let mut short = [0xa5; MIN_MESSAGE_BYTES - 1];
        assert_eq!(client.discover(&mut short), Err(Error::OutputTooSmall));
        assert_eq!(client.state(), State::Init);
        assert_eq!(short, [0xa5; MIN_MESSAGE_BYTES - 1]);

        client.state = State::Selecting;
        let mut bytes = [0u8; MAX_MESSAGE_BYTES];
        let length = server_reply_fixture(
            0x1234_5678,
            mac,
            MessageType::Offer,
            [192, 0, 2, 2],
            [192, 0, 2, 1],
            Some(3),
            &mut bytes,
        )
        .unwrap();
        assert_eq!(client.receive_offer(&bytes[..length]), Err(Error::Lease));
    }

    #[test]
    fn production_dhcp_state_path() {
        assert_eq!(self_test(), Ok(()));
    }
}
