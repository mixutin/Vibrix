# DNS resolver contract

Checked against RFC 1035 on 2026-09-29.

Vibrix implements a bounded DNS resolver codec for standard Internet-class
IPv4 A-record queries. It creates one recursive query with exactly one
question, validates the response transaction/header/question, rejects truncated
UDP responses and non-zero RCODEs, follows bounded RFC 1035 compression
pointers, and returns at most four A addresses with the minimum observed TTL.

The resolver is allocation-free. Names are limited to the RFC 1035 63-octet
label and 255-octet encoded-name bounds. Matching is ASCII case-insensitive.
The production self-test places the generated DNS query inside the real Vibrix
UDP codec and parses a compressed A-record response after ExitBootServices.

## Deliberate boundary

This milestone does not yet provide retries, caching, search domains, CNAME
chasing, DNSSEC, EDNS, TCP fallback, mDNS, IPv6 records, or external network
traffic. A TC=1 response fails closed until TCP exists. Physical/external DNS
requires the later RTL8168 driver and socket/application plumbing.

Primary source:

- RFC 1035 — Domain Names: Implementation and Specification
