//! Bounded Ethernet/IPv4 ARP neighbor state and local responder.
//!
//! The wire codec lives in `arp.rs`. This module implements the RFC 826
//! receive/update/reply rules for one configured local IPv4 address, plus the
//! RFC 5227 conflict check and zero-sender-IP Probe behavior. It performs no
//! device I/O and owns no timers. Conflict policy remains with the caller; this
//! module neither defends/abandons an address nor emits unsolicited ARP traffic.

use super::{MAX_FRAME, MacAddress, arp, ethernet};

pub const CAPACITY: usize = 8;
const ZERO_IP: [u8; 4] = [0; 4];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Neighbor {
    pub ip: [u8; 4],
    pub mac: MacAddress,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Ethernet(ethernet::Error),
    Arp(arp::Error),
    OutputTooSmall,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    Ignored,
    Learned,
    Reply(usize),
    Conflict { claimant: MacAddress },
}

/// Fixed-capacity ARP translation table.
///
/// Existing entries are always updated in place. New entries fill empty slots
/// first, then replace entries round-robin. No age is inferred because this
/// bounded foundation has no ARP timer yet.
pub struct NeighborCache {
    entries: [Option<Neighbor>; CAPACITY],
    replace: usize,
}

impl NeighborCache {
    pub const fn new() -> Self {
        Self {
            entries: [None; CAPACITY],
            replace: 0,
        }
    }

    pub fn lookup(&self, ip: [u8; 4]) -> Option<MacAddress> {
        self.entries
            .iter()
            .flatten()
            .find(|entry| entry.ip == ip)
            .map(|entry| entry.mac)
    }

    fn has(&self, ip: [u8; 4]) -> bool {
        self.lookup(ip).is_some()
    }

    fn learn(&mut self, ip: [u8; 4], mac: MacAddress) {
        if ip == ZERO_IP {
            return;
        }
        if let Some(entry) = self
            .entries
            .iter_mut()
            .flatten()
            .find(|entry| entry.ip == ip)
        {
            entry.mac = mac;
            return;
        }
        if let Some(slot) = self.entries.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(Neighbor { ip, mac });
            return;
        }
        self.entries[self.replace] = Some(Neighbor { ip, mac });
        self.replace = (self.replace + 1) % CAPACITY;
    }
}

impl Default for NeighborCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Process one Ethernet frame for one configured local MAC/IPv4 address.
///
/// RFC 826's existing-entry merge occurs before target/opcode handling.
/// A new sender mapping is learned only when this host is the target. ARP
/// Probes (zero sender IP) are never cached. Requests for the local address,
/// including Probes, receive a unicast ARP Reply. A peer claiming our own IP
/// from a different MAC is returned as an explicit conflict per RFC 5227.
pub fn process(
    local_mac: MacAddress,
    local_ip: [u8; 4],
    cache: &mut NeighborCache,
    input: &[u8],
    output: &mut [u8],
) -> Result<Action, Error> {
    let frame = ethernet::parse(input).map_err(Error::Ethernet)?;
    if frame.ether_type() != ethernet::ARP {
        return Ok(Action::Ignored);
    }
    if frame.destination() != local_mac.bytes() && frame.destination() != ethernet::BROADCAST {
        return Ok(Action::Ignored);
    }

    let packet = arp::Packet::parse(frame.payload()).map_err(Error::Arp)?;

    if packet.sender_ip == local_ip && packet.sender_mac != local_mac {
        return Ok(Action::Conflict {
            claimant: packet.sender_mac,
        });
    }

    let existed = packet.sender_ip != ZERO_IP && cache.has(packet.sender_ip);
    if existed {
        cache.learn(packet.sender_ip, packet.sender_mac);
    }

    if packet.target_ip != local_ip {
        return Ok(if existed {
            Action::Learned
        } else {
            Action::Ignored
        });
    }

    if packet.sender_ip != ZERO_IP && !existed {
        cache.learn(packet.sender_ip, packet.sender_mac);
    }

    if packet.operation != arp::Operation::Request {
        return Ok(if packet.sender_ip == ZERO_IP {
            Action::Ignored
        } else {
            Action::Learned
        });
    }

    let reply = arp::Packet {
        operation: arp::Operation::Reply,
        sender_mac: local_mac,
        sender_ip: local_ip,
        target_mac: packet.sender_mac.bytes(),
        target_ip: packet.sender_ip,
    };
    let mut payload = [0u8; arp::PACKET_BYTES];
    reply.write(&mut payload).map_err(Error::Arp)?;

    let required = super::ETHERNET_HEADER + arp::PACKET_BYTES;
    let wire_length = required.max(ethernet::MIN_FRAME);
    if output.len() < wire_length {
        return Err(Error::OutputTooSmall);
    }
    let length = ethernet::encode(
        local_mac,
        packet.sender_mac.bytes(),
        ethernet::ARP,
        &payload,
        output,
    )
    .map_err(Error::Ethernet)?;
    Ok(Action::Reply(length))
}

pub(super) fn self_test() -> Result<(), Error> {
    let local_mac =
        MacAddress::new([2, 0, 0, 0, 0, 1]).map_err(|_| Error::Arp(arp::Error::Invariant))?;
    let remote_mac =
        MacAddress::new([2, 0, 0, 0, 0, 2]).map_err(|_| Error::Arp(arp::Error::Invariant))?;
    let local_ip = [192, 0, 2, 1];
    let remote_ip = [192, 0, 2, 2];

    let request = arp::Packet {
        operation: arp::Operation::Request,
        sender_mac: remote_mac,
        sender_ip: remote_ip,
        target_mac: [0; 6],
        target_ip: local_ip,
    };
    let mut arp_bytes = [0u8; arp::PACKET_BYTES];
    request.write(&mut arp_bytes).map_err(Error::Arp)?;
    let mut frame = [0u8; MAX_FRAME];
    let frame_length = ethernet::encode(
        remote_mac,
        ethernet::BROADCAST,
        ethernet::ARP,
        &arp_bytes,
        &mut frame,
    )
    .map_err(Error::Ethernet)?;

    let mut cache = NeighborCache::new();
    let mut reply = [0u8; MAX_FRAME];
    let Action::Reply(reply_length) = process(
        local_mac,
        local_ip,
        &mut cache,
        &frame[..frame_length],
        &mut reply,
    )?
    else {
        return Err(Error::Arp(arp::Error::Invariant));
    };
    if cache.lookup(remote_ip) != Some(remote_mac) {
        return Err(Error::Arp(arp::Error::Invariant));
    }

    let reply_frame = ethernet::parse(&reply[..reply_length]).map_err(Error::Ethernet)?;
    if reply_frame.destination() != remote_mac.bytes()
        || reply_frame.source() != local_mac
        || reply_frame.ether_type() != ethernet::ARP
    {
        return Err(Error::Arp(arp::Error::Invariant));
    }
    let reply_packet = arp::Packet::parse(reply_frame.payload()).map_err(Error::Arp)?;
    if reply_packet.operation != arp::Operation::Reply
        || reply_packet.sender_mac != local_mac
        || reply_packet.sender_ip != local_ip
        || reply_packet.target_mac != remote_mac.bytes()
        || reply_packet.target_ip != remote_ip
    {
        return Err(Error::Arp(arp::Error::Invariant));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mac(last: u8) -> MacAddress {
        MacAddress::new([2, 0, 0, 0, 0, last]).unwrap()
    }

    fn frame(packet: arp::Packet, destination: [u8; 6]) -> ([u8; MAX_FRAME], usize) {
        let mut payload = [0u8; arp::PACKET_BYTES];
        packet.write(&mut payload).unwrap();
        let mut frame = [0u8; MAX_FRAME];
        let length = ethernet::encode(
            packet.sender_mac,
            destination,
            ethernet::ARP,
            &payload,
            &mut frame,
        )
        .unwrap();
        (frame, length)
    }

    #[test]
    fn request_for_local_address_learns_and_replies() {
        assert_eq!(self_test(), Ok(()));
    }

    #[test]
    fn probe_is_answered_but_zero_sender_ip_is_not_cached() {
        let local = mac(1);
        let remote = mac(2);
        let local_ip = [192, 0, 2, 1];
        let packet = arp::Packet {
            operation: arp::Operation::Request,
            sender_mac: remote,
            sender_ip: ZERO_IP,
            target_mac: [0; 6],
            target_ip: local_ip,
        };
        let (frame, length) = frame(packet, ethernet::BROADCAST);
        let mut cache = NeighborCache::new();
        let mut output = [0u8; MAX_FRAME];
        let Action::Reply(reply_length) =
            process(local, local_ip, &mut cache, &frame[..length], &mut output).unwrap()
        else {
            panic!("probe was not answered");
        };
        assert_eq!(cache.lookup(ZERO_IP), None);
        let reply = ethernet::parse(&output[..reply_length]).unwrap();
        let reply = arp::Packet::parse(reply.payload()).unwrap();
        assert_eq!(reply.sender_ip, local_ip);
        assert_eq!(reply.target_ip, ZERO_IP);
        assert_eq!(reply.target_mac, remote.bytes());
    }

    #[test]
    fn existing_sender_mapping_updates_even_when_not_target() {
        let local = mac(1);
        let old = mac(2);
        let new = mac(3);
        let remote_ip = [192, 0, 2, 9];
        let mut cache = NeighborCache::new();
        cache.learn(remote_ip, old);

        let packet = arp::Packet {
            operation: arp::Operation::Reply,
            sender_mac: new,
            sender_ip: remote_ip,
            target_mac: mac(4).bytes(),
            target_ip: [192, 0, 2, 44],
        };
        let (frame, length) = frame(packet, local.bytes());
        let mut output = [0xa5; MAX_FRAME];
        assert_eq!(
            process(
                local,
                [192, 0, 2, 1],
                &mut cache,
                &frame[..length],
                &mut output
            ),
            Ok(Action::Learned)
        );
        assert_eq!(cache.lookup(remote_ip), Some(new));
        assert_eq!(output, [0xa5; MAX_FRAME]);
    }

    #[test]
    fn new_sender_not_for_us_is_not_cached() {
        let local = mac(1);
        let remote = mac(2);
        let remote_ip = [192, 0, 2, 9];
        let packet = arp::Packet {
            operation: arp::Operation::Reply,
            sender_mac: remote,
            sender_ip: remote_ip,
            target_mac: mac(4).bytes(),
            target_ip: [192, 0, 2, 44],
        };
        let (frame, length) = frame(packet, ethernet::BROADCAST);
        let mut cache = NeighborCache::new();
        let mut output = [0xa5; MAX_FRAME];
        assert_eq!(
            process(
                local,
                [192, 0, 2, 1],
                &mut cache,
                &frame[..length],
                &mut output
            ),
            Ok(Action::Ignored)
        );
        assert_eq!(cache.lookup(remote_ip), None);
        assert_eq!(output, [0xa5; MAX_FRAME]);
    }

    #[test]
    fn conflicting_claim_of_local_ip_is_explicit_and_not_cached() {
        let local = mac(1);
        let claimant = mac(2);
        let local_ip = [192, 0, 2, 1];
        let packet = arp::Packet {
            operation: arp::Operation::Reply,
            sender_mac: claimant,
            sender_ip: local_ip,
            target_mac: local.bytes(),
            target_ip: local_ip,
        };
        let (frame, length) = frame(packet, local.bytes());
        let mut cache = NeighborCache::new();
        let mut output = [0xa5; MAX_FRAME];
        assert_eq!(
            process(local, local_ip, &mut cache, &frame[..length], &mut output),
            Ok(Action::Conflict { claimant })
        );
        assert_eq!(cache.lookup(local_ip), None);
        assert_eq!(output, [0xa5; MAX_FRAME]);
    }

    #[test]
    fn full_cache_replacement_is_bounded_and_deterministic() {
        let mut cache = NeighborCache::new();
        for n in 1..=CAPACITY as u8 {
            cache.learn([10, 0, 0, n], mac(n));
        }
        assert_eq!(cache.lookup([10, 0, 0, 1]), Some(mac(1)));
        cache.learn([10, 0, 1, 1], mac(20));
        assert_eq!(cache.lookup([10, 0, 0, 1]), None);
        assert_eq!(cache.lookup([10, 0, 1, 1]), Some(mac(20)));
        cache.learn([10, 0, 1, 2], mac(21));
        assert_eq!(cache.lookup([10, 0, 0, 2]), None);
        assert_eq!(cache.lookup([10, 0, 1, 2]), Some(mac(21)));
    }

    #[test]
    fn short_reply_output_is_transactional() {
        let local = mac(1);
        let remote = mac(2);
        let packet = arp::Packet {
            operation: arp::Operation::Request,
            sender_mac: remote,
            sender_ip: [192, 0, 2, 2],
            target_mac: [0; 6],
            target_ip: [192, 0, 2, 1],
        };
        let (frame, length) = frame(packet, ethernet::BROADCAST);
        let mut cache = NeighborCache::new();
        let mut output = [0xa5; ethernet::MIN_FRAME - 1];
        assert_eq!(
            process(
                local,
                [192, 0, 2, 1],
                &mut cache,
                &frame[..length],
                &mut output
            ),
            Err(Error::OutputTooSmall)
        );
        assert_eq!(output, [0xa5; ethernet::MIN_FRAME - 1]);
        // Learning occurs before replying, matching RFC 826 receive order.
        assert_eq!(cache.lookup([192, 0, 2, 2]), Some(remote));
    }
}
