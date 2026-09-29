# IPv4 routing table and route-selection policy

Vibrix M22 uses a fixed-capacity, allocation-free IPv4 routing table as its
first routing control-plane primitive.

Each route contains a canonical network prefix, prefix length, optional next-hop
gateway, interface identifier and metric. Invalid prefix lengths, host bits in a
network prefix and an unspecified gateway fail closed.

Selection is deterministic:

1. longest matching prefix wins;
2. for equal prefix lengths, the lower metric wins;
3. an exact remaining tie keeps the earlier table entry.

A default route is represented by `0.0.0.0/0`. Inserting the same
network/prefix/interface replaces that route without consuming another slot.
Capacity failure does not mutate the table.

The kernel's normal subsystem self-test exercises the production implementation
and emits a marker only after longest-prefix and default-route behavior pass.

This completes only the bounded M22 **routing table and route-selection policy**
item once exact-head evidence is green. It does not forward packets, manage
interfaces, perform ARP/neighbor resolution, expose route syscalls, implement
policy routing/ECMP, or prove traffic over physical hardware.
