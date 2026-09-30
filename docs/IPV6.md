# IPv6, ICMPv6 and Neighbor Discovery core

This M22 slice adds an allocation-free IPv6/ICMPv6 policy and wire-format core.
It accepts only the fixed 40-byte IPv6 header and ICMPv6 next-header value 58.
Unsupported extension-header chains and fragmentation fail closed.

Implemented behavior:

- strict IPv6 version and payload-length parsing;
- IPv6 pseudo-header ICMPv6 checksum generation/verification;
- Echo Request to Echo Reply handling;
- Neighbor Solicitation validation;
- solicited Neighbor Advertisement generation with a bounded target
  link-layer-address option;
- mandatory hop-limit 255 enforcement for Neighbor Discovery;
- fixed local-target matching and malformed/corrupt-input rejection.

The kernel production self-test executes the same implementation after
ExitBootServices and emits an evidence marker only after Echo and NDP paths both
verify.

This does **not** provide IPv6 sockets, SLAAC, Router Solicitation/Advertisement,
DAD state, extension headers, fragmentation, routing, multicast membership,
live NIC transmission, DHCPv6, DNS over IPv6 or physical Target 001 evidence.
