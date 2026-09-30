//! Bounded NTPv4 client wire parser and clock-discipline policy.
//!
//! This module implements the 48-byte basic NTP header needed by a client and
//! validates a server response against the exact transmit timestamp from the
//! request. It computes the RFC 5905 four-timestamp offset/delay sample and
//! feeds a conservative step/slew policy. No socket, DNS, RTC write or NTS
//! authentication is performed here.

pub const PACKET_BYTES: usize = 48;
pub const NTP_FRACTION_SCALE: u64 = 1u64 << 32;
pub const STEP_THRESHOLD: i64 = (NTP_FRACTION_SCALE / 8) as i64;
pub const MAX_ACCEPTABLE_DELAY: u64 = NTP_FRACTION_SCALE * 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Timestamp(pub u64);

impl Timestamp {
    pub const ZERO: Self = Self(0);

    pub const fn from_parts(seconds: u32, fraction: u32) -> Self {
        Self(((seconds as u64) << 32) | fraction as u64)
    }

    pub const fn raw(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Sample {
    pub offset: i64,
    pub delay: u64,
    pub stratum: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisciplineAction {
    Ignore,
    Slew(i64),
    Step(i64),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Length,
    Version,
    Mode,
    Unsynchronized,
    InvalidStratum,
    OriginMismatch,
    ZeroTimestamp,
    NegativeDelay,
}

fn read_timestamp(packet: &[u8], offset: usize) -> Timestamp {
    Timestamp(u64::from_be_bytes(
        packet[offset..offset + 8]
            .try_into()
            .expect("validated NTP timestamp slice"),
    ))
}

fn write_timestamp(packet: &mut [u8; PACKET_BYTES], offset: usize, timestamp: Timestamp) {
    packet[offset..offset + 8].copy_from_slice(&timestamp.raw().to_be_bytes());
}

fn signed_delta(later: Timestamp, earlier: Timestamp) -> i64 {
    later.raw().wrapping_sub(earlier.raw()) as i64
}

/// Compose the basic client request. The caller supplies the local transmit
/// timestamp so the exact value can be matched against the response origin.
pub fn request(transmit: Timestamp) -> Result<[u8; PACKET_BYTES], Error> {
    if transmit == Timestamp::ZERO {
        return Err(Error::ZeroTimestamp);
    }
    let mut packet = [0u8; PACKET_BYTES];
    packet[0] = (4 << 3) | 3; // LI=0, VN=4, Mode=3 (client)
    write_timestamp(&mut packet, 40, transmit);
    Ok(packet)
}

/// Parse one basic NTPv4 server reply and compute the four-timestamp sample.
///
/// Extension fields and authenticated packets are deliberately unsupported in
/// this first slice; callers must supply exactly the base 48-byte header.
pub fn response(
    packet: &[u8],
    request_transmit: Timestamp,
    destination: Timestamp,
) -> Result<Sample, Error> {
    if packet.len() != PACKET_BYTES {
        return Err(Error::Length);
    }
    let leap = packet[0] >> 6;
    let version = (packet[0] >> 3) & 0x7;
    let mode = packet[0] & 0x7;
    if version != 4 {
        return Err(Error::Version);
    }
    if mode != 4 {
        return Err(Error::Mode);
    }
    if leap == 3 {
        return Err(Error::Unsynchronized);
    }
    let stratum = packet[1];
    if !(1..=15).contains(&stratum) {
        return Err(Error::InvalidStratum);
    }

    let origin = read_timestamp(packet, 24);
    let receive = read_timestamp(packet, 32);
    let transmit = read_timestamp(packet, 40);
    if origin != request_transmit {
        return Err(Error::OriginMismatch);
    }
    if request_transmit == Timestamp::ZERO
        || receive == Timestamp::ZERO
        || transmit == Timestamp::ZERO
        || destination == Timestamp::ZERO
    {
        return Err(Error::ZeroTimestamp);
    }

    let client_path = signed_delta(destination, request_transmit);
    let server_path = signed_delta(transmit, receive);
    let delay = client_path.checked_sub(server_path).ok_or(Error::NegativeDelay)?;
    if delay < 0 {
        return Err(Error::NegativeDelay);
    }
    let offset = signed_delta(receive, request_transmit)
        .saturating_add(signed_delta(transmit, destination))
        / 2;

    Ok(Sample {
        offset,
        delay: delay as u64,
        stratum,
    })
}

/// Conservative clock-discipline decision. Small offsets are slewed, large
/// offsets are stepped, and samples with excessive round-trip delay are ignored.
pub const fn discipline(sample: Sample) -> DisciplineAction {
    if sample.delay > MAX_ACCEPTABLE_DELAY {
        return DisciplineAction::Ignore;
    }
    let magnitude = sample.offset.unsigned_abs();
    if magnitude > STEP_THRESHOLD as u64 {
        DisciplineAction::Step(sample.offset)
    } else if sample.offset == 0 {
        DisciplineAction::Ignore
    } else {
        DisciplineAction::Slew(sample.offset)
    }
}

pub fn self_test() -> Result<(), Error> {
    let t1 = Timestamp::from_parts(100, 0);
    let _request = request(t1)?;
    let t2 = Timestamp(t1.raw() + 100);
    let t3 = Timestamp(t1.raw() + 200);
    let t4 = Timestamp(t1.raw() + 400);

    let mut reply = [0u8; PACKET_BYTES];
    reply[0] = (4 << 3) | 4;
    reply[1] = 2;
    write_timestamp(&mut reply, 24, t1);
    write_timestamp(&mut reply, 32, t2);
    write_timestamp(&mut reply, 40, t3);

    let sample = response(&reply, t1, t4)?;
    if sample.offset != -50
        || sample.delay != 300
        || sample.stratum != 2
        || discipline(sample) != DisciplineAction::Slew(-50)
    {
        return Err(Error::NegativeDelay);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_reply(t1: Timestamp, t2: Timestamp, t3: Timestamp) -> [u8; PACKET_BYTES] {
        let mut packet = [0u8; PACKET_BYTES];
        packet[0] = (4 << 3) | 4;
        packet[1] = 1;
        write_timestamp(&mut packet, 24, t1);
        write_timestamp(&mut packet, 32, t2);
        write_timestamp(&mut packet, 40, t3);
        packet
    }

    #[test]
    fn request_is_basic_v4_client_packet_with_exact_transmit_timestamp() {
        let t1 = Timestamp::from_parts(0xe0000000, 0x12345678);
        let packet = request(t1).unwrap();
        assert_eq!(packet.len(), PACKET_BYTES);
        assert_eq!(packet[0] & 0x7, 3);
        assert_eq!((packet[0] >> 3) & 0x7, 4);
        assert_eq!(read_timestamp(&packet, 40), t1);
        assert_eq!(request(Timestamp::ZERO), Err(Error::ZeroTimestamp));
    }

    #[test]
    fn four_timestamp_offset_and_delay_are_computed_and_origin_is_bound() {
        let t1 = Timestamp::from_parts(1000, 0);
        let t2 = Timestamp(t1.raw() + 40);
        let t3 = Timestamp(t1.raw() + 60);
        let t4 = Timestamp(t1.raw() + 100);
        let reply = valid_reply(t1, t2, t3);

        assert_eq!(
            response(&reply, t1, t4),
            Ok(Sample {
                offset: 0,
                delay: 80,
                stratum: 1,
            })
        );
        assert_eq!(
            response(&reply, Timestamp(t1.raw() + 1), t4),
            Err(Error::OriginMismatch)
        );
    }

    #[test]
    fn malformed_unsynchronized_and_bogus_server_replies_fail_closed() {
        let t1 = Timestamp::from_parts(100, 0);
        let t2 = Timestamp(t1.raw() + 10);
        let t3 = Timestamp(t1.raw() + 20);
        let t4 = Timestamp(t1.raw() + 30);
        let mut reply = valid_reply(t1, t2, t3);

        assert_eq!(response(&reply[..47], t1, t4), Err(Error::Length));

        reply[0] = (3 << 3) | 4;
        assert_eq!(response(&reply, t1, t4), Err(Error::Version));

        reply[0] = (4 << 3) | 3;
        assert_eq!(response(&reply, t1, t4), Err(Error::Mode));

        reply[0] = (3 << 6) | (4 << 3) | 4;
        assert_eq!(response(&reply, t1, t4), Err(Error::Unsynchronized));

        reply[0] = (4 << 3) | 4;
        reply[1] = 0;
        assert_eq!(response(&reply, t1, t4), Err(Error::InvalidStratum));
    }

    #[test]
    fn clock_discipline_is_bounded_by_offset_and_delay() {
        assert_eq!(
            discipline(Sample {
                offset: 1,
                delay: 10,
                stratum: 1,
            }),
            DisciplineAction::Slew(1)
        );
        assert_eq!(
            discipline(Sample {
                offset: STEP_THRESHOLD + 1,
                delay: 10,
                stratum: 1,
            }),
            DisciplineAction::Step(STEP_THRESHOLD + 1)
        );
        assert_eq!(
            discipline(Sample {
                offset: STEP_THRESHOLD + 1,
                delay: MAX_ACCEPTABLE_DELAY + 1,
                stratum: 1,
            }),
            DisciplineAction::Ignore
        );
    }

    #[test]
    fn production_self_test_passes() {
        self_test().unwrap();
    }
}
