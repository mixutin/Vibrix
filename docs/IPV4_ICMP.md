# IPv4 and ICMP echo foundation

Authoring model: **GPT-5.6 Sol**.

Checked: **2026-09-29**.

## Scope

This change implements a bounded IPv4/ICMP host path on top of Vibrix's
existing Ethernet-II and NIC abstractions:

- fixed IPv4 version 4 header with IHL=5 (20 bytes);
- header Total Length and Internet checksum validation;
- no IPv4 options;
- no IPv4 fragmentation or reassembly;
- local-unicast destination matching;
- ICMPv4 Echo Request and Echo Reply;
- full ICMP checksum validation, including odd-length data;
- Echo Reply source/destination reversal;
- identifier, sequence number and echo data preservation;
- transactional encoders that do not mutate output on validation/capacity
  errors;
- a production kernel self-test that constructs an Ethernet/IPv4/ICMP Echo
  Request, runs the responder, parses the resulting Echo Reply and verifies the
  complete address/header/data contract.

The responder deliberately ignores frames for another MAC, another local IP,
another IPv4 protocol, ICMP Echo Replies, and link broadcast Echo Requests.
Malformed IPv4/ICMP input fails closed.

## Primary sources

- RFC 791, *Internet Protocol*, especially section 3.1 for the IPv4 header,
  Total Length, fragment fields, TTL, Protocol and header checksum:
  https://www.rfc-editor.org/rfc/rfc791
- RFC 792, *Internet Control Message Protocol*, Echo/Echo Reply format and
  checksum requirements:
  https://www.rfc-editor.org/rfc/rfc792
- RFC 1122, section 3.2.2.6, requiring a host ICMP Echo server, source-address
  selection for replies and preservation of Echo data:
  https://www.rfc-editor.org/rfc/rfc1122
- RFC 1071, Internet checksum implementation properties:
  https://www.rfc-editor.org/rfc/rfc1071

## Design choices

Vibrix currently has no routing table, IPv4 fragmentation queues, DHCP-assigned
address, RTL8168 hardware driver or external link. Therefore this milestone
uses a strict no-options/no-fragments local-host subset rather than silently
accepting packets the kernel cannot correctly process.

The IPv4 parser permits Ethernet padding beyond IPv4 Total Length but exposes
only the datagram payload covered by Total Length. The responder only answers a
specific local unicast MAC/IP. RFC 1122 permits broadcast/multicast Echo
Requests to be discarded, which avoids introducing an amplification path before
network policy exists.

No dependency is added. The checksum, IPv4 and ICMP code is first-party Rust
and allocation-free.

## Evidence boundary

The dedicated NIC workflow must run production-linked host tests and the normal
post-`ExitBootServices` QEMU kernel self-test, and require the exact marker:

`VIBRIX: kernel IPv4 ICMP echo verified`

This proves the kernel executes the bounded IPv4 + ICMP Echo behavior under
QEMU. It does **not** claim:

- RTL8168-family hardware operation;
- external Ethernet traffic;
- ARP neighbor learning/cache behavior;
- routing or forwarding;
- fragmentation/reassembly;
- IPv4 options;
- DHCP, DNS, UDP, TCP or sockets;
- Target 001 networking.

Those remain separate roadmap work.
