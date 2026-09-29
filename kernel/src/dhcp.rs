//! Bounded DHCPv4 client codec and state machine.
//!
//! Implements the minimum RFC 2131 client exchange used by Vibrix:
//! DISCOVER -> OFFER -> REQUEST -> ACK. No timers, retransmission, renewal,
//! persistence or device I/O live here.

use super::MacAddress;

pub const CLIENT_PORT: u16 = 68;
pub const SERVER_PORT: u16 = 67;
pub const FIXED_BYTES: usize = 236;
pub const COOKIE_BYTES: usize = 4;
pub const MIN_PACKET_BYTES: usize = FIXED_BYTES + COOKIE_BYTES;
pub const MAX_PACKET_BYTES: usize = 576;
const MAGIC_COOKIE: [u8; 4] = [99, 130, 83, 99];

const OPTION_PAD: u8 = 0;
const OPTION_SUBNET_MASK: u8 = 1;
const OPTION_ROUTER: u8 = 3;
const OPTION_DNS: u8 = 6;
const OPTION_REQUESTED_IP: u8 = 50;
const OPTION_LEASE_TIME: u8 = 51;
const OPTION_MESSAGE_TYPE: u8 = 53;
const OPTION_SERVER_IDENTIFIER: u8 = 54;
const OPTION_PARAMETER_REQUEST_LIST: u8 = 55;
const OPTION_CLIENT_IDENTIFIER: u8 = 61;
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
            _ => Err(Error::MessageType),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    OutputTooSmall,
    Length,
    Format,
    Transaction,
    ClientIdentity,
    Options,
    MessageType,
    State,
    Server,
    Address,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum State {
    Init,
    Selecting,
    Requesting,
    Bound,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Offer {
    pub address: [u8; 4],
    pub server: [u8; 4],
    pub subnet_mask: Option<[u8; 4]>,
    pub router: Option<[u8; 4]>,
    pub dns: Option<[u8; 4]>,
    pub lease_seconds: Option<u32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Lease {
    pub address: [u8; 4],
    pub server: [u8; 4],
    pub subnet_mask: Option<[u8; 4]>,
    pub router: Option<[u8; 4]>,
    pub dns: Option<[u8; 4]>,
    pub lease_seconds: Option<u32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Client {
    mac: MacAddress,
    xid: u32,
    state: State,
    offer: Option<Offer>,
}

impl Client {
    pub const fn new(mac: MacAddress, xid: u32) -> Self {
        Self {
            mac,
            xid,
            state: State::Init,
            offer: None,
        }
    }

    pub const fn state(&self) -> State {
        self.state
    }

    pub const fn transaction_id(&self) -> u32 {
        self.xid
    }

    pub fn discover(&mut self, output: &mut [u8]) -> Result<usize, Error> {
        if self.state != State::Init {
            return Err(Error::State);
        }
        let length = write_client_message(
            self.mac,
            self.xid,
            MessageType::Discover,
            None,
            None,
            output,
        )?;
        self.state = State::Selecting;
        Ok(length)
    }

    pub fn accept_offer(&mut self, input: &[u8]) -> Result<Offer, Error> {
        if self.state != State::Selecting {
            return Err(Error::State);
        }
        let parsed = parse_reply(self.mac, self.xid, input)?;
        if parsed.message_type != MessageType::Offer {
            return Err(Error::MessageType);
        }
        let server = parsed.server.ok_or(Error::Server)?;
        let offer = Offer {
            address: parsed.address,
            server,
            subnet_mask: parsed.subnet_mask,
            router: parsed.router,
            dns: parsed.dns,
            lease_seconds: parsed.lease_seconds,
        };
        self.offer = Some(offer);
        self.state = State::Requesting;
        Ok(offer)
    }

    pub fn request(&self, output: &mut [u8]) -> Result<usize, Error> {
        if self.state != State::Requesting {
            return Err(Error::State);
        }
        let offer = self.offer.ok_or(Error::State)?;
        write_client_message(
            self.mac,
            self.xid,
            MessageType::Request,
            Some(offer.address),
            Some(offer.server),
            output,
        )
    }

    pub fn accept_ack(&mut self, input: &[u8]) -> Result<Lease, Error> {
        if self.state != State::Requesting {
            return Err(Error::State);
        }
        let offer = self.offer.ok_or(Error::State)?;
        let parsed = parse_reply(self.mac, self.xid, input)?;
        if parsed.message_type != MessageType::Ack {
            return Err(Error::MessageType);
        }
        if parsed.address != offer.address {
            return Err(Error::Address);
        }
        if let Some(server) = parsed.server
            && server != offer.server
        {
            return Err(Error::Server);
        }
        self.state = State::Bound;
        Ok(Lease {
            address: parsed.address,
            server: parsed.server.unwrap_or(offer.server),
            subnet_mask: parsed.subnet_mask.or(offer.subnet_mask),
            router: parsed.router.or(offer.router),
            dns: parsed.dns.or(offer.dns),
            lease_seconds: parsed.lease_seconds.or(offer.lease_seconds),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ParsedReply {
    message_type: MessageType,
    address: [u8; 4],
    server: Option<[u8; 4]>,
    subnet_mask: Option<[u8; 4]>,
    router: Option<[u8; 4]>,
    dns: Option<[u8; 4]>,
    lease_seconds: Option<u32>,
}

fn write_option(output: &mut [u8], cursor: &mut usize, code: u8, data: &[u8]) -> Result<(), Error> {
    let needed = 2usize.checked_add(data.len()).ok_or(Error::Length)?;
    let end = cursor.checked_add(needed).ok_or(Error::Length)?;
    if end > output.len() || data.len() > u8::MAX as usize {
        return Err(Error::OutputTooSmall);
    }
    output[*cursor] = code;
    output[*cursor + 1] = data.len() as u8;
    output[*cursor + 2..end].copy_from_slice(data);
    *cursor = end;
    Ok(())
}

fn write_client_message(
    mac: MacAddress,
    xid: u32,
    message_type: MessageType,
    requested_ip: Option<[u8; 4]>,
    server: Option<[u8; 4]>,
    output: &mut [u8],
) -> Result<usize, Error> {
    if output.len() < MIN_PACKET_BYTES + 24 {
        return Err(Error::OutputTooSmall);
    }
    output[..MIN_PACKET_BYTES].fill(0);
    output[0] = 1;
    output[1] = 1;
    output[2] = 6;
    output[4..8].copy_from_slice(&xid.to_be_bytes());
    output[10..12].copy_from_slice(&0x8000u16.to_be_bytes());
    output[28..34].copy_from_slice(&mac.bytes());
    output[FIXED_BYTES..MIN_PACKET_BYTES].copy_from_slice(&MAGIC_COOKIE);

    let mut cursor = MIN_PACKET_BYTES;
    write_option(
        output,
        &mut cursor,
        OPTION_MESSAGE_TYPE,
        &[message_type as u8],
    )?;

    let mut client_id = [0u8; 7];
    client_id[0] = 1;
    client_id[1..].copy_from_slice(&mac.bytes());
    write_option(output, &mut cursor, OPTION_CLIENT_IDENTIFIER, &client_id)?;

    if let Some(address) = requested_ip {
        write_option(output, &mut cursor, OPTION_REQUESTED_IP, &address)?;
    }
    if let Some(server) = server {
        write_option(output, &mut cursor, OPTION_SERVER_IDENTIFIER, &server)?;
    }

    let requested = [
        OPTION_SUBNET_MASK,
        OPTION_ROUTER,
        OPTION_DNS,
        OPTION_LEASE_TIME,
        OPTION_SERVER_IDENTIFIER,
    ];
    write_option(
        output,
        &mut cursor,
        OPTION_PARAMETER_REQUEST_LIST,
        &requested,
    )?;
    if cursor >= output.len() {
        return Err(Error::OutputTooSmall);
    }
    output[cursor] = OPTION_END;
    cursor += 1;
    Ok(cursor)
}

fn read_ipv4(data: &[u8]) -> Result<[u8; 4], Error> {
    if data.len() != 4 {
        return Err(Error::Options);
    }
    let mut address = [0u8; 4];
    address.copy_from_slice(data);
    Ok(address)
}

fn parse_reply(mac: MacAddress, xid: u32, input: &[u8]) -> Result<ParsedReply, Error> {
    if !(MIN_PACKET_BYTES..=MAX_PACKET_BYTES).contains(&input.len()) {
        return Err(Error::Length);
    }
    if input[0] != 2 || input[1] != 1 || input[2] != 6 {
        return Err(Error::Format);
    }
    if u32::from_be_bytes([input[4], input[5], input[6], input[7]]) != xid {
        return Err(Error::Transaction);
    }
    if input[28..34] != mac.bytes() {
        return Err(Error::ClientIdentity);
    }
    if input[FIXED_BYTES..MIN_PACKET_BYTES] != MAGIC_COOKIE {
        return Err(Error::Format);
    }

    let mut address = [0u8; 4];
    address.copy_from_slice(&input[16..20]);
    if address == [0; 4] {
        return Err(Error::Address);
    }

    let mut message_type = None;
    let mut server = None;
    let mut subnet_mask = None;
    let mut router = None;
    let mut dns = None;
    let mut lease_seconds = None;
    let mut cursor = MIN_PACKET_BYTES;

    while cursor < input.len() {
        let code = input[cursor];
        cursor += 1;
        if code == OPTION_PAD {
            continue;
        }
        if code == OPTION_END {
            break;
        }
        if cursor >= input.len() {
            return Err(Error::Options);
        }
        let length = input[cursor] as usize;
        cursor += 1;
        let end = cursor.checked_add(length).ok_or(Error::Options)?;
        if end > input.len() {
            return Err(Error::Options);
        }
        let data = &input[cursor..end];
        match code {
            OPTION_MESSAGE_TYPE => {
                if data.len() != 1 || message_type.is_some() {
                    return Err(Error::Options);
                }
                message_type = Some(MessageType::parse(data[0])?);
            }
            OPTION_SERVER_IDENTIFIER => {
                if server.is_some() {
                    return Err(Error::Options);
                }
                server = Some(read_ipv4(data)?);
            }
            OPTION_SUBNET_MASK => {
                if subnet_mask.is_some() {
                    return Err(Error::Options);
                }
                subnet_mask = Some(read_ipv4(data)?);
            }
            OPTION_ROUTER => {
                if data.len() < 4 || !data.len().is_multiple_of(4) || router.is_some() {
                    return Err(Error::Options);
                }
                router = Some(read_ipv4(&data[..4])?);
            }
            OPTION_DNS => {
                if data.len() < 4 || !data.len().is_multiple_of(4) || dns.is_some() {
                    return Err(Error::Options);
                }
                dns = Some(read_ipv4(&data[..4])?);
            }
            OPTION_LEASE_TIME => {
                if data.len() != 4 || lease_seconds.is_some() {
                    return Err(Error::Options);
                }
                lease_seconds = Some(u32::from_be_bytes([data[0], data[1], data[2], data[3]]));
            }
            _ => {}
        }
        cursor = end;
    }

    Ok(ParsedReply {
        message_type: message_type.ok_or(Error::MessageType)?,
        address,
        server,
        subnet_mask,
        router,
        dns,
        lease_seconds,
    })
}

fn write_server_reply(
    mac: MacAddress,
    xid: u32,
    message_type: MessageType,
    address: [u8; 4],
    server: [u8; 4],
    output: &mut [u8],
) -> Result<usize, Error> {
    if output.len() < MIN_PACKET_BYTES + 40 {
        return Err(Error::OutputTooSmall);
    }
    output[..MIN_PACKET_BYTES].fill(0);
    output[0] = 2;
    output[1] = 1;
    output[2] = 6;
    output[4..8].copy_from_slice(&xid.to_be_bytes());
    output[16..20].copy_from_slice(&address);
    output[28..34].copy_from_slice(&mac.bytes());
    output[FIXED_BYTES..MIN_PACKET_BYTES].copy_from_slice(&MAGIC_COOKIE);
    let mut cursor = MIN_PACKET_BYTES;
    write_option(
        output,
        &mut cursor,
        OPTION_MESSAGE_TYPE,
        &[message_type as u8],
    )?;
    write_option(output, &mut cursor, OPTION_SERVER_IDENTIFIER, &server)?;
    write_option(output, &mut cursor, OPTION_SUBNET_MASK, &[255, 255, 255, 0])?;
    write_option(output, &mut cursor, OPTION_ROUTER, &server)?;
    write_option(output, &mut cursor, OPTION_DNS, &[203, 0, 113, 53])?;
    write_option(
        output,
        &mut cursor,
        OPTION_LEASE_TIME,
        &3600u32.to_be_bytes(),
    )?;
    output[cursor] = OPTION_END;
    Ok(cursor + 1)
}

pub(super) fn self_test() -> Result<(), Error> {
    let mac = MacAddress::new([2, 0, 0, 0, 0, 42]).map_err(|_| Error::ClientIdentity)?;
    let xid = 0x5642_5258;
    let address = [192, 0, 2, 42];
    let server = [192, 0, 2, 1];
    let mut client = Client::new(mac, xid);
    let mut packet = [0u8; MAX_PACKET_BYTES];

    let discover_len = client.discover(&mut packet)?;
    if discover_len <= MIN_PACKET_BYTES
        || client.state() != State::Selecting
        || packet[0] != 1
        || packet[4..8] != xid.to_be_bytes()
    {
        return Err(Error::Format);
    }

    let offer_len = write_server_reply(mac, xid, MessageType::Offer, address, server, &mut packet)?;
    let offer = client.accept_offer(&packet[..offer_len])?;
    if offer.address != address || offer.server != server || client.state() != State::Requesting {
        return Err(Error::State);
    }

    let request_len = client.request(&mut packet)?;
    if request_len <= MIN_PACKET_BYTES || packet[0] != 1 {
        return Err(Error::Format);
    }

    let ack_len = write_server_reply(mac, xid, MessageType::Ack, address, server, &mut packet)?;
    let lease = client.accept_ack(&packet[..ack_len])?;
    if client.state() != State::Bound
        || lease.address != address
        || lease.server != server
        || lease.subnet_mask != Some([255, 255, 255, 0])
        || lease.router != Some(server)
        || lease.dns != Some([203, 0, 113, 53])
        || lease.lease_seconds != Some(3600)
    {
        return Err(Error::State);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client() -> (Client, MacAddress) {
        let mac = MacAddress::new([2, 0, 0, 0, 0, 7]).unwrap();
        (Client::new(mac, 0x0102_0304), mac)
    }

    #[test]
    fn production_exchange_reaches_bound_state() {
        assert_eq!(self_test(), Ok(()));
    }

    #[test]
    fn discover_has_required_bootp_identity_cookie_and_options() {
        let (mut client, mac) = client();
        let mut output = [0xa5; MAX_PACKET_BYTES];
        let length = client.discover(&mut output).unwrap();
        assert_eq!(output[0..4], [1, 1, 6, 0]);
        assert_eq!(&output[4..8], &0x0102_0304u32.to_be_bytes());
        assert_eq!(&output[10..12], &0x8000u16.to_be_bytes());
        assert_eq!(&output[28..34], &mac.bytes());
        assert_eq!(&output[FIXED_BYTES..MIN_PACKET_BYTES], &MAGIC_COOKIE);
        assert_eq!(output[MIN_PACKET_BYTES..MIN_PACKET_BYTES + 3], [53, 1, 1]);
        assert_eq!(output[length - 1], OPTION_END);
        assert_eq!(output[length], 0xa5);
    }

    #[test]
    fn offer_request_ack_preserves_selected_address_and_server() {
        let (mut client, mac) = client();
        let mut packet = [0u8; MAX_PACKET_BYTES];
        client.discover(&mut packet).unwrap();
        let address = [198, 51, 100, 25];
        let server = [198, 51, 100, 1];
        let length = write_server_reply(
            mac,
            client.transaction_id(),
            MessageType::Offer,
            address,
            server,
            &mut packet,
        )
        .unwrap();
        let offer = client.accept_offer(&packet[..length]).unwrap();
        assert_eq!(offer.address, address);
        assert_eq!(offer.server, server);

        let request_length = client.request(&mut packet).unwrap();
        assert!(
            packet[..request_length]
                .windows(6)
                .any(|window| window == [50, 4, address[0], address[1], address[2], address[3]])
        );
        assert!(
            packet[..request_length]
                .windows(6)
                .any(|window| window == [54, 4, server[0], server[1], server[2], server[3]])
        );

        let length = write_server_reply(
            mac,
            client.transaction_id(),
            MessageType::Ack,
            address,
            server,
            &mut packet,
        )
        .unwrap();
        let lease = client.accept_ack(&packet[..length]).unwrap();
        assert_eq!(lease.address, address);
        assert_eq!(lease.server, server);
        assert_eq!(client.state(), State::Bound);
    }

    #[test]
    fn wrong_transaction_identity_message_and_address_fail_closed() {
        let (mut client, mac) = client();
        let mut packet = [0u8; MAX_PACKET_BYTES];
        client.discover(&mut packet).unwrap();
        let length = write_server_reply(
            mac,
            0x9999_9999,
            MessageType::Offer,
            [192, 0, 2, 10],
            [192, 0, 2, 1],
            &mut packet,
        )
        .unwrap();
        assert_eq!(
            client.accept_offer(&packet[..length]),
            Err(Error::Transaction)
        );

        let length = write_server_reply(
            MacAddress::new([2, 0, 0, 0, 0, 8]).unwrap(),
            client.transaction_id(),
            MessageType::Offer,
            [192, 0, 2, 10],
            [192, 0, 2, 1],
            &mut packet,
        )
        .unwrap();
        assert_eq!(
            client.accept_offer(&packet[..length]),
            Err(Error::ClientIdentity)
        );

        let length = write_server_reply(
            mac,
            client.transaction_id(),
            MessageType::Ack,
            [192, 0, 2, 10],
            [192, 0, 2, 1],
            &mut packet,
        )
        .unwrap();
        assert_eq!(
            client.accept_offer(&packet[..length]),
            Err(Error::MessageType)
        );
    }

    #[test]
    fn malformed_options_and_short_outputs_are_rejected_without_state_change() {
        let (mut client, mac) = client();
        let mut short = [0xa5; MIN_PACKET_BYTES + 8];
        assert_eq!(client.discover(&mut short), Err(Error::OutputTooSmall));
        assert_eq!(client.state(), State::Init);
        assert_eq!(short, [0xa5; MIN_PACKET_BYTES + 8]);

        let mut packet = [0u8; MAX_PACKET_BYTES];
        client.discover(&mut packet).unwrap();
        let length = write_server_reply(
            mac,
            client.transaction_id(),
            MessageType::Offer,
            [192, 0, 2, 20],
            [192, 0, 2, 1],
            &mut packet,
        )
        .unwrap();
        packet[MIN_PACKET_BYTES + 1] = 250;
        assert_eq!(client.accept_offer(&packet[..length]), Err(Error::Options));
        assert_eq!(client.state(), State::Selecting);
    }
}
