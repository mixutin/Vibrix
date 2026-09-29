# Bounded Ethernet-II framing

Authoring model: **GPT-6 Astra Pro**.

`nic::ethernet` supplies allocation-free encoding and borrowed parsing for the
existing synchronous NIC abstraction. Frames exclude preamble and FCS. The
parser follows that interface's logical 14..1514-byte range, validates a nonzero
unicast source MAC and an Ethernet-II type, and exposes the destination, source,
type and payload. Destination acceptance is left to the caller; multicast and
broadcast destinations are not mistaken for source addresses.

The encoder accepts at most 1500 payload bytes and emits at least 60 bytes,
zeroing the complete padding region. It checks the type, payload size and output
capacity before mutation, writes only the returned prefix, and preserves the
remaining buffer. These are memory-buffer guarantees, not packet delivery.

Payload views include received link padding. A higher-level protocol must use
its own declared length; zero-valued payload bytes must not be silently stripped.
802.3 length/LLC framing and the standard 802.1Q/802.1ad tags are rejected instead
of misinterpreted. Unknown untagged EtherTypes are preserved for explicit upper
layer dispatch. VLAN, jumbo frames, hardware offloads and FCS validation require
separate contracts and are not implied here.

## Production integration and tests

The existing NIC self-test now builds its experimental-EtherType frame through
this encoder, transmits it through the production RAM loopback, verifies short
receive-buffer retention, and parses/compares the returned header and payload
before the required kernel NIC success marker. No hardware or external traffic
is generated. Seven added host tests cover a fixed header fixture, zero padding,
all short outputs, bad sizes/types/source addresses, maximum MTU, unpadded logical
receive and the production loopback. Existing standalone NIC and canonical
host/build/QEMU checks cover the same implementation.

This advances the networking foundation without checking off M10 Ethernet,
ARP/IP, driver, Internet connectivity or Target 001 milestones. No runtime or
build dependency, unsafe code, raw pointer, DMA or workflow change is added.

## Research and design

Primary references checked 2026-09-29: RFC 894, Frame Format and Address Mappings;
RFC 826, Packet format; IANA IEEE 802 Numbers, EtherTypes. These define byte order,
48-bit address layout, zero padding, the 1500-byte MTU and protocol identifiers.
The original Rust code reuses Vibrix's bounds and address type. A full network
stack library remains a future design decision; this narrow framing adapter
neither implements nor replaces one. No external OS implementation was copied.
