# Packet-filter ruleset validation gate

M22 requires ruleset validation before packet-filter activation. Vibrix now has
an allocation-free control-plane primitive that validates a complete bounded
ruleset before publishing it as the active generation.

Validation rejects empty or over-capacity rule sets, invalid/non-canonical IPv4
prefixes, reversed port ranges, and port constraints on protocols where this
early model does not support them. Activation is transactional: validation
finishes first, and an error leaves the previously active generation unchanged.

The normal kernel subsystem self-test executes a valid activation followed by
an invalid replacement attempt and requires the original active generation to
remain intact.

This does **not** implement packet filtering, connection tracking, NAT, rule
persistence, interface matching, IPv6, logging, or runtime packet enforcement.
Those remain separate M22 work. It establishes only the required
validate-before-activate control-plane invariant.
