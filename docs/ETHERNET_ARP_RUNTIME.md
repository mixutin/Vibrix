# Ethernet + ARP runtime foundation

Authoring model: **GPT-5.6 Sol**.

Checked: **2026-09-29**.

## Scope

This change promotes the existing Ethernet-II and ARP wire codecs into a
bounded production IPv4/Ethernet ARP runtime:

- fixed-capacity eight-entry ARP translation table;
- update an existing sender mapping before target/opcode handling;
- learn a new non-zero sender mapping when this host is the target;
- answer ordinary ARP Requests for the configured local IPv4 address;
- answer zero-sender-IP ARP Probes for the configured local address;
- never cache the zero sender address used by ARP Probes;
- report another MAC claiming the configured local IPv4 address as an explicit
  conflict;
- deterministic bounded round-robin replacement when the table is full;
- preserve the existing allocation-free Ethernet-II and ARP wire codecs;
- production self-test constructs a broadcast ARP Request, learns the peer,
  emits a unicast ARP Reply and reparses the exact result.

There is deliberately no timer-based aging yet. The table never grows and
contains no dynamically allocated state.

## Primary sources

- RFC 826, *An Ethernet Address Resolution Protocol*, packet generation and
  packet reception rules:
  https://www.rfc-editor.org/rfc/rfc826
- RFC 5227, *IPv4 Address Conflict Detection*, especially Sections 1.2, 2.1.1,
  2.4 and 2.5:
  https://www.rfc-editor.org/rfc/rfc5227

## Required behavior adopted

RFC 826 specifies that an already-known sender mapping is updated before the
target check and that a host targeted by an ARP Request adds the sender mapping
and replies with local addresses as sender fields.

RFC 5227 preserves those RFC 826 rules and adds the conflict test. It also
requires a host already using an IPv4 address to answer normal ARP Requests for
that address, including ARP Probes whose sender IP is all zeroes. Probe sender
IP zero must not pollute the ARP cache.

A conflicting packet is surfaced to the caller instead of silently selecting a
defense/reconfiguration policy. RFC 5227 requires such a policy, but choosing
between abandoning or defending an address requires configuration, timers and
network lifecycle that Vibrix does not have yet.

## Evidence boundary

The production NIC workflow must host-test the real module, target-Clippy the
kernel and require the exact post-`ExitBootServices` QEMU marker:

`VIBRIX: kernel Ethernet ARP responder verified`

This establishes the **Ethernet + ARP software runtime** roadmap item on the
reference QEMU kernel. It does not claim a physical RTL8168 driver, external
network traffic, ARP cache aging, DHCP address acquisition, routing, sockets or
Target 001 networking.
