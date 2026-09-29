# UDP datagram foundation

Authoring model: **GPT-5.6 Sol**.

Checked: **2026-09-29**.

## Scope

This change adds an allocation-free UDP datagram layer on top of the bounded
IPv4 implementation from PR #195.

Implemented behavior:

- RFC 768 source/destination port fields;
- UDP Length validation and payload slicing;
- IPv4 pseudo-header checksum generation;
- non-zero received checksum validation;
- odd-length payload checksum handling;
- generated checksums default on;
- mathematical zero checksum transmitted as `0xffff`;
- transmitted checksum zero accepted and reported as absent, as RFC 768 permits;
- a production self-test that encodes UDP, parses it, wraps it in IPv4, parses
  the IPv4 packet and validates the UDP datagram again.

All encode precondition/capacity failures leave the output unchanged.

## Primary sources

- RFC 768, *User Datagram Protocol*, wire format, UDP Length, pseudo-header
  checksum and zero-checksum encoding:
  https://www.rfc-editor.org/rfc/rfc768
- RFC 1122 section 4.1, especially 4.1.3.4, requiring UDP checksum
  generation/validation support and checksum generation to default on:
  https://www.rfc-editor.org/rfc/rfc1122

## Design choices

This PR does not add socket state, port allocation, demultiplexing, blocking
receive queues, routing or a hardware driver. Those are separate roadmap items.
The UDP layer therefore exposes a checked borrowed datagram codec and a
production kernel self-test rather than inventing application or socket policy.

A received checksum of zero is represented explicitly as absent. Non-zero
checksums fail closed when the pseudo-header or datagram bytes do not verify.
Vibrix-generated UDP datagrams always include a checksum.

No dependency and no unsafe code are added.

## Evidence boundary

The dedicated NIC workflow must compile/test the production code and require
the post-`ExitBootServices` QEMU marker:

`VIBRIX: kernel UDP datagram verified`

That proves the UDP layer executes in the real kernel under QEMU. It does not
claim sockets, DHCP, DNS, external traffic, RTL8168 hardware operation or
Target 001 networking.
