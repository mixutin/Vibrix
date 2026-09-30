# IPv4 ingress anti-spoofing and resource limits

This M22 slice defines a bounded pre-parser ingress guard for the current IPv4
stack.

The guard rejects:

- packets larger than a configured per-interface byte ceiling;
- all fragments while Vibrix lacks IP fragment reassembly;
- unspecified source addresses unless an explicit DHCP-style exception is set;
- loopback, multicast and limited-broadcast source addresses on normal ingress;
- packets claiming the host's own local address as their remote source.

The production subsystem self-test exercises accepted traffic plus spoofed,
fragmented and oversized rejection cases.

This is intentionally conservative. It does not implement stateful firewalling,
uRPF, interface-aware routing validation, IPv6, fragment reassembly, NAT or
physical NIC receive integration. Those remain separate work.
