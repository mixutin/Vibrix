# Bounded stateful IPv4 packet filter

Vibrix's first packet-filter dataplane builds on the existing transactional
ruleset validator. It is fixed-capacity, allocation-free policy code intended
for later integration at the native NIC/IP ingress and egress boundaries.

The engine provides:

- validated first-match IPv4 rules for TCP, UDP, ICMP, or any protocol;
- source and destination CIDR matching;
- destination-port ranges for TCP and UDP rules;
- an explicit default Pass or Block policy;
- sixteen remembered TCP/UDP flows;
- bidirectional acceptance for remembered flows;
- deterministic state replacement when the bounded table is full;
- state flush after a successful ruleset generation change;
- transactional preservation of both rules and state after a failed update.

The production kernel subsystem self-test exercises default-deny behavior,
rule-based TCP admission and reverse-flow state. The existing packet-filter
evidence marker is emitted only after this behavior and the ruleset validation
tests succeed.

## Current boundary

This is a policy/dataplane primitive, not yet the complete network firewall.
It is not wired into every physical NIC packet path, has no TCP-state-machine
inspection beyond a remembered 5-tuple, no state timeout clock, no fragment
reassembly, NAT, port redirection, IPv6, logging, per-interface rules, or
administrator-facing ruleset language. Those remain separate roadmap work.
