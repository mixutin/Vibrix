//! Bounded DNS resolver codec for IPv4 A records.
//!
//! Implements one standard recursive query and parses bounded UDP responses.
//! No cache, retries, search domains, TCP fallback or device I/O live here.

use super::{MTU, udp};

pub const PORT: u16 = 53;
pub const HEADER_BYTES: usize = 12;
pub const MAX_MESSAGE_BYTES: usize = 512;
pub const MAX_NAME_BYTES: usize = 255;
pub const MAX_ADDRESSES: usize = 4;

const TYPE_A: u16 = 1;
const CLASS_IN: u16 = 1;
const FLAG_RD: u16 = 1 << 8;
const FLAG_QR: u16 = 1 << 15;
const FLAG_TC: u16 = 1 << 9;
const RCODE_MASK: u16 = 0x000f;
const OPCODE_MASK: u16 = 0x7800;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Name,
    Length,
    OutputTooSmall,
    Transaction,
    Header,
    Truncated,
    ResponseCode(u8),
    Question,
    Compression,
    NoAddress,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AnswerSet {
    addresses: [[u8; 4]; MAX_ADDRESSES],
    len: usize,
    ttl_min: u32,
}

impl AnswerSet {
    pub fn addresses(&self) -> &[[u8; 4]] {
        &self.addresses[..self.len]
    }

    pub const fn ttl_min(&self) -> u32 {
        self.ttl_min
    }
}

fn encode_name(name: &str, output: &mut [u8]) -> Result<usize, Error> {
    let name = name.strip_suffix('.').unwrap_or(name);
    if name.is_empty() || name.len() > 253 {
        return Err(Error::Name);
    }
    let mut cursor = 0usize;
    for label in name.split('.') {
        if label.is_empty() || label.len() > 63 || !label.is_ascii() {
            return Err(Error::Name);
        }
        let needed = 1usize.checked_add(label.len()).ok_or(Error::Length)?;
        let end = cursor.checked_add(needed).ok_or(Error::Length)?;
        if end >= output.len() {
            return Err(Error::OutputTooSmall);
        }
        output[cursor] = label.len() as u8;
        output[cursor + 1..end].copy_from_slice(label.as_bytes());
        cursor = end;
    }
    if cursor >= output.len() {
        return Err(Error::OutputTooSmall);
    }
    output[cursor] = 0;
    Ok(cursor + 1)
}

fn lower(byte: u8) -> u8 {
    if byte.is_ascii_uppercase() {
        byte + (b'a' - b'A')
    } else {
        byte
    }
}

fn decode_name(
    packet: &[u8],
    start: usize,
    output: &mut [u8; MAX_NAME_BYTES],
) -> Result<(usize, usize), Error> {
    let mut cursor = start;
    let mut consumed = None;
    let mut written = 0usize;
    let mut labels = 0usize;
    let mut jumps = 0usize;

    loop {
        let length = *packet.get(cursor).ok_or(Error::Length)?;
        if length & 0xc0 == 0xc0 {
            let second = *packet.get(cursor + 1).ok_or(Error::Length)?;
            let pointer = (((length & 0x3f) as usize) << 8) | second as usize;
            if pointer >= packet.len() {
                return Err(Error::Compression);
            }
            if consumed.is_none() {
                consumed = Some(cursor + 2);
            }
            jumps += 1;
            if jumps > 16 {
                return Err(Error::Compression);
            }
            cursor = pointer;
            continue;
        }
        if length & 0xc0 != 0 {
            return Err(Error::Compression);
        }
        cursor += 1;
        if length == 0 {
            let end = consumed.unwrap_or(cursor);
            return Ok((end, written));
        }

        let length = length as usize;
        if length > 63 {
            return Err(Error::Name);
        }
        let end = cursor.checked_add(length).ok_or(Error::Length)?;
        if end > packet.len() {
            return Err(Error::Length);
        }
        if labels != 0 {
            if written >= output.len() {
                return Err(Error::Name);
            }
            output[written] = b'.';
            written += 1;
        }
        if written.checked_add(length).ok_or(Error::Name)? > output.len() {
            return Err(Error::Name);
        }
        for byte in &packet[cursor..end] {
            output[written] = lower(*byte);
            written += 1;
        }
        labels += 1;
        cursor = end;
    }
}

fn expected_name(name: &str, output: &mut [u8; MAX_NAME_BYTES]) -> Result<usize, Error> {
    let name = name.strip_suffix('.').unwrap_or(name);
    if name.is_empty() || name.len() > 253 || !name.is_ascii() {
        return Err(Error::Name);
    }
    let bytes = name.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        output[index] = lower(*byte);
    }
    Ok(bytes.len())
}

pub fn encode_query(id: u16, name: &str, output: &mut [u8]) -> Result<usize, Error> {
    if output.len() < HEADER_BYTES + 5 {
        return Err(Error::OutputTooSmall);
    }
    output[..HEADER_BYTES].fill(0);
    output[0..2].copy_from_slice(&id.to_be_bytes());
    output[2..4].copy_from_slice(&FLAG_RD.to_be_bytes());
    output[4..6].copy_from_slice(&1u16.to_be_bytes());

    let name_length = encode_name(name, &mut output[HEADER_BYTES..])?;
    let cursor = HEADER_BYTES + name_length;
    let end = cursor.checked_add(4).ok_or(Error::Length)?;
    if end > output.len() || end > MAX_MESSAGE_BYTES {
        return Err(Error::OutputTooSmall);
    }
    output[cursor..cursor + 2].copy_from_slice(&TYPE_A.to_be_bytes());
    output[cursor + 2..end].copy_from_slice(&CLASS_IN.to_be_bytes());
    Ok(end)
}

pub fn parse_a_response(id: u16, name: &str, input: &[u8]) -> Result<AnswerSet, Error> {
    if !(HEADER_BYTES..=MAX_MESSAGE_BYTES).contains(&input.len()) {
        return Err(Error::Length);
    }
    if u16::from_be_bytes([input[0], input[1]]) != id {
        return Err(Error::Transaction);
    }
    let flags = u16::from_be_bytes([input[2], input[3]]);
    if flags & FLAG_QR == 0 || flags & OPCODE_MASK != 0 {
        return Err(Error::Header);
    }
    if flags & FLAG_TC != 0 {
        return Err(Error::Truncated);
    }
    let rcode = (flags & RCODE_MASK) as u8;
    if rcode != 0 {
        return Err(Error::ResponseCode(rcode));
    }

    let questions = u16::from_be_bytes([input[4], input[5]]);
    let answers = u16::from_be_bytes([input[6], input[7]]);
    if questions != 1 || answers == 0 {
        return Err(Error::Question);
    }

    let mut decoded = [0u8; MAX_NAME_BYTES];
    let (mut cursor, decoded_len) = decode_name(input, HEADER_BYTES, &mut decoded)?;
    let mut expected = [0u8; MAX_NAME_BYTES];
    let expected_len = expected_name(name, &mut expected)?;
    if decoded_len != expected_len || decoded[..decoded_len] != expected[..expected_len] {
        return Err(Error::Question);
    }
    let question_end = cursor.checked_add(4).ok_or(Error::Length)?;
    if question_end > input.len() {
        return Err(Error::Length);
    }
    let qtype = u16::from_be_bytes([input[cursor], input[cursor + 1]]);
    let qclass = u16::from_be_bytes([input[cursor + 2], input[cursor + 3]]);
    if qtype != TYPE_A || qclass != CLASS_IN {
        return Err(Error::Question);
    }
    cursor = question_end;

    let mut result = AnswerSet {
        addresses: [[0; 4]; MAX_ADDRESSES],
        len: 0,
        ttl_min: u32::MAX,
    };

    for _ in 0..answers {
        let (next, _) = decode_name(input, cursor, &mut decoded)?;
        cursor = next;
        let header_end = cursor.checked_add(10).ok_or(Error::Length)?;
        if header_end > input.len() {
            return Err(Error::Length);
        }
        let record_type = u16::from_be_bytes([input[cursor], input[cursor + 1]]);
        let class = u16::from_be_bytes([input[cursor + 2], input[cursor + 3]]);
        let ttl = u32::from_be_bytes([
            input[cursor + 4],
            input[cursor + 5],
            input[cursor + 6],
            input[cursor + 7],
        ]);
        let data_len = u16::from_be_bytes([input[cursor + 8], input[cursor + 9]]) as usize;
        cursor = header_end;
        let data_end = cursor.checked_add(data_len).ok_or(Error::Length)?;
        if data_end > input.len() {
            return Err(Error::Length);
        }

        if record_type == TYPE_A && class == CLASS_IN && data_len == 4 && result.len < MAX_ADDRESSES
        {
            result.addresses[result.len].copy_from_slice(&input[cursor..data_end]);
            result.len += 1;
            result.ttl_min = result.ttl_min.min(ttl);
        }
        cursor = data_end;
    }

    if result.len == 0 {
        return Err(Error::NoAddress);
    }
    Ok(result)
}

fn write_test_response(
    query: &[u8],
    id: u16,
    address: [u8; 4],
    output: &mut [u8],
) -> Result<usize, Error> {
    if query.len() > MAX_MESSAGE_BYTES || output.len() < query.len() + 16 {
        return Err(Error::OutputTooSmall);
    }
    output[..query.len()].copy_from_slice(query);
    output[0..2].copy_from_slice(&id.to_be_bytes());
    output[2..4].copy_from_slice(&0x8180u16.to_be_bytes());
    output[6..8].copy_from_slice(&1u16.to_be_bytes());
    let mut cursor = query.len();
    output[cursor..cursor + 2].copy_from_slice(&[0xc0, 0x0c]);
    cursor += 2;
    output[cursor..cursor + 2].copy_from_slice(&TYPE_A.to_be_bytes());
    output[cursor + 2..cursor + 4].copy_from_slice(&CLASS_IN.to_be_bytes());
    output[cursor + 4..cursor + 8].copy_from_slice(&300u32.to_be_bytes());
    output[cursor + 8..cursor + 10].copy_from_slice(&4u16.to_be_bytes());
    output[cursor + 10..cursor + 14].copy_from_slice(&address);
    Ok(cursor + 14)
}

pub(super) fn self_test() -> Result<(), Error> {
    let id = 0x5642;
    let name = "vibrix.test";
    let mut query = [0u8; MAX_MESSAGE_BYTES];
    let query_len = encode_query(id, name, &mut query)?;

    let mut udp_bytes = [0u8; MTU];
    let source = [192, 0, 2, 42];
    let resolver = [192, 0, 2, 53];
    let udp_len = udp::encode(
        source,
        resolver,
        49152,
        PORT,
        &query[..query_len],
        &mut udp_bytes,
    )
    .map_err(|_| Error::Length)?;
    let datagram =
        udp::parse(source, resolver, &udp_bytes[..udp_len]).map_err(|_| Error::Length)?;
    if datagram.destination_port() != PORT || datagram.payload() != &query[..query_len] {
        return Err(Error::Header);
    }

    let mut response = [0u8; MAX_MESSAGE_BYTES];
    let response_len =
        write_test_response(&query[..query_len], id, [203, 0, 113, 7], &mut response)?;
    let answers = parse_a_response(id, name, &response[..response_len])?;
    if answers.addresses() != [[203, 0, 113, 7]] || answers.ttl_min() != 300 {
        return Err(Error::NoAddress);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_query_response_path() {
        assert_eq!(self_test(), Ok(()));
    }

    #[test]
    fn query_encodes_labels_header_and_a_in_question() {
        let mut output = [0xa5; MAX_MESSAGE_BYTES];
        let len = encode_query(0x1234, "WWW.Example.COM.", &mut output).unwrap();
        assert_eq!(
            &output[..12],
            &[0x12, 0x34, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0]
        );
        assert_eq!(&output[12..29], b"\x03WWW\x07Example\x03COM\x00");
        assert_eq!(&output[29..33], &[0, 1, 0, 1]);
        assert_eq!(output[len], 0xa5);
    }

    #[test]
    fn compressed_answer_and_case_insensitive_question_are_accepted() {
        let mut query = [0u8; MAX_MESSAGE_BYTES];
        let query_len = encode_query(7, "WWW.Example.COM", &mut query).unwrap();
        let mut response = [0u8; MAX_MESSAGE_BYTES];
        let response_len =
            write_test_response(&query[..query_len], 7, [192, 0, 2, 9], &mut response).unwrap();
        let answers = parse_a_response(7, "www.example.com", &response[..response_len]).unwrap();
        assert_eq!(answers.addresses(), [[192, 0, 2, 9]]);
        assert_eq!(answers.ttl_min(), 300);
    }

    #[test]
    fn malformed_names_transactions_and_truncation_fail_closed() {
        let mut output = [0u8; MAX_MESSAGE_BYTES];
        assert_eq!(encode_query(1, "", &mut output), Err(Error::Name));
        assert_eq!(encode_query(1, "a..b", &mut output), Err(Error::Name));
        assert_eq!(
            encode_query(1, &"a".repeat(64), &mut output),
            Err(Error::Name)
        );

        let query_len = encode_query(9, "vibrix.test", &mut output).unwrap();
        let query = output;
        let mut response = [0u8; MAX_MESSAGE_BYTES];
        let response_len =
            write_test_response(&query[..query_len], 9, [192, 0, 2, 8], &mut response).unwrap();
        assert_eq!(
            parse_a_response(10, "vibrix.test", &response[..response_len]),
            Err(Error::Transaction)
        );
        response[2] |= 0x02;
        assert_eq!(
            parse_a_response(9, "vibrix.test", &response[..response_len]),
            Err(Error::Truncated)
        );
    }

    #[test]
    fn compression_loop_is_rejected() {
        let mut packet = [0u8; HEADER_BYTES + 6];
        packet[0..2].copy_from_slice(&1u16.to_be_bytes());
        packet[2..4].copy_from_slice(&0x8180u16.to_be_bytes());
        packet[4..6].copy_from_slice(&1u16.to_be_bytes());
        packet[6..8].copy_from_slice(&1u16.to_be_bytes());
        packet[12..14].copy_from_slice(&[0xc0, 0x0c]);
        assert_eq!(
            parse_a_response(1, "loop.test", &packet),
            Err(Error::Compression)
        );
    }
}
