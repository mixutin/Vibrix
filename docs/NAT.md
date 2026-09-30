# IPv4 NAT and port redirection

This M22 slice implements a fixed-capacity static NAT/port-redirection table and
a real packet rewrite dataplane for IPv4 TCP/UDP packets.

A rule maps one internal transport endpoint to one externally visible endpoint.
Outbound translation rewrites source address/port. Inbound translation rewrites
the externally visible destination back to the internal endpoint. The dataplane
recomputes the IPv4 header checksum and TCP/UDP pseudo-header checksum after
translation; IPv4 UDP packets that intentionally use checksum zero retain that
semantics.

The implementation fails closed for IPv4 options, fragmented packets, malformed
TCP/UDP lengths, unsupported protocols, duplicate mappings, zero endpoints and
unmatched flows. Failed admission/lookup occurs before packet mutation.

The production post-firmware self-test performs outbound and inbound UDP
translation using the same implementation.

This is static NAT only. It does not allocate ephemeral mappings, track
connection lifetime, hairpin NAT, translate ICMP errors, support IPv6/NAT64,
rewrite application payloads, hook a physical NIC forwarding path, or provide
administrator-facing rule syntax yet.
