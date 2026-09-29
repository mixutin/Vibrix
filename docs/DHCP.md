# DHCPv4 client contract

Checked against RFC 2131 and RFC 2132 on 2026-09-29.

Vibrix implements a bounded DHCPv4 client protocol core for the M10 DHCP
milestone. The client owns one transaction at a time and implements the initial
four-message acquisition exchange:

`DHCPDISCOVER -> DHCPOFFER -> DHCPREQUEST -> DHCPACK`.

The implementation validates BOOTP reply framing, Ethernet hardware type and
length, transaction ID, client hardware address and the DHCP magic cookie.
Options use the RFC 2132 tag/length/value format with pad/end handling and
network byte order. Duplicate singleton options, truncated option bodies,
invalid message types, a zero offered address, the wrong server, and
transaction/client mismatches fail closed.

The client requests subnet mask, router, DNS server, lease time and server
identifier. The first router/DNS address is retained when a valid list is
present. REQUEST identifies both the selected offered address and server.

## Deliberate boundary

This module is allocation-free protocol/state-machine code. It does not yet
provide timers, randomized transaction IDs, exponential retransmission,
INIT-REBOOT, RENEWING/REBINDING, lease persistence, DHCPDECLINE/RELEASE,
classless routes, IPv6, or external NIC transmission. The production kernel
QEMU self-test exercises the real compiled protocol path after ExitBootServices
using a synthetic server exchange; physical/external DHCP requires the later
RTL8168 driver.

Primary sources:

- RFC 2131 — Dynamic Host Configuration Protocol
- RFC 2132 — DHCP Options and BOOTP Vendor Extensions
