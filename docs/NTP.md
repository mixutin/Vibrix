# Bounded NTPv4 client and clock-discipline policy

The first Vibrix NTP slice implements the base 48-byte NTPv4 client exchange
defined by RFC 5905 and a conservative clock-discipline decision policy.

The client:

- emits Version 4 / client-mode requests with a caller-supplied transmit timestamp;
- accepts only exact 48-byte Version 4 server-mode responses;
- rejects unsynchronized leap state and invalid strata;
- binds the response origin timestamp to the exact request transmit timestamp;
- requires nonzero origin/receive/transmit/destination timestamps;
- computes the four-timestamp round-trip delay and clock offset;
- rejects negative delay;
- chooses Ignore, Slew or Step using fixed delay/offset thresholds.

RFC 5905 status and errata were checked on 2026-09-30. The RFC Editor lists
updates including RFC 7822, RFC 8573, RFC 9109, RFC 9748 and RFC 9769. This
bounded implementation intentionally does not parse extension fields or MACs
and does not implement transport port randomization or interleaved mode.

Primary reference:
https://www.rfc-editor.org/info/rfc5905/

## Boundary

The implementation is the NTP protocol and discipline policy only. It does not
yet send UDP traffic to an external server, resolve NTP pool names, authenticate
time with NTS, write a hardware/software wall clock, persist drift estimates,
handle leap-second tables, or provide physical Target 001 time evidence.
