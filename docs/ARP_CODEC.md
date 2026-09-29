# Ethernet/IPv4 ARP payload codec

Authoring model: **GPT-6 Astra Pro**.

`nic::arp::Packet` encodes and decodes the fixed 28-byte Ethernet/IPv4 ARP
request/reply format. Header fields are checked before address decoding:
hardware type 1, protocol type 0x0800, hardware length 6, protocol length 4,
and request/reply opcode 1 or 2. Multi-byte fields use network byte order.
Sender MACs reuse the existing nonzero unicast `MacAddress` type.

Protocol addresses are data, not proof of ownership or suitability. In
particular a zero sender IPv4 address remains representable for ARP probes.
An unknown request target MAC may be zero or broadcast. The codec does not
impose reply-only assumptions on request address fields.

Parsing accepts 28 through 1500 payload bytes, ignores enclosing link padding,
and returns an owned fixed-size value without allocation. Encoding writes
exactly 28 bytes and returns that length; all shorter output buffers remain
unchanged. Unused suffix bytes are untouched. A link encoder must initialize
its own padding rather than send the entire scratch buffer.

## Integration boundary

This is a payload codec, not an ARP responder, cache, resolver or driver. The
caller must validate the enclosing Ethernet frame and EtherType before dispatch,
apply local destination/source and address-ownership policy, and separately
authorize network activation. No inbound mapping is trusted or installed here.
There is no automatic reply, rate limiter, retry/expiry timer, conflict detector,
external packet transmission or claim that M10 Ethernet + ARP is complete.

The module is independent of the Ethernet framing PR: it consumes the existing
NIC address type and does not modify that PR's frame-encoding behavior. No
legacy `host-tests/arp_cache.rs` code is copied; that fixture is a different,
non-production cache model.

## Validation

Eight production-linked host tests cover a fixed request fixture, reply byte
order, every truncated length, malformed fixed-header bytes and opcodes,
unchanged short outputs, link padding, probe addresses and invalid sender MACs.
The existing NIC self-test calls this production codec for request and reply
round trips, a rejected output buffer and malformed address length before the
required QEMU NIC marker. This proves codec execution, not an ARP exchange.
Existing standalone NIC tests, actual-target lint/build and QEMU workflows
include the new module without changing CI. No dependencies or unsafe Rust.

## Research

Primary references checked on 2026-09-29: RFC 826, Packet format and Packet
Generation; RFC 5227 section 2.1.1 for probe sender and target fields. All code
is original safe Rust using Vibrix's existing address/buffer contracts. A full
network-stack library remains a separate integration choice; a serializer does
not provide its cache, timers, routing, activation policy or hardware support.
