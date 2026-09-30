# Resolver configuration and local cache

Vibrix already has a bounded DNS A resolver. This M22 slice adds an explicit
resolver configuration object and an optional local A-record cache.

Configuration requires one to three nonzero IPv4 resolver addresses and records
whether caching is enabled. The cache stores at most eight canonical lowercase
DNS names, one IPv4 address per entry, an absolute expiry derived from a
caller-supplied monotonic second counter, and a deterministic insertion
generation. TTL zero is rejected. Expired entries are removed during lookup and
a full cache replaces the oldest insertion deterministically.

This is resolver policy/cache logic only. It does not persist resolver
configuration, issue external DNS traffic itself, implement CNAME/DNSSEC/EDNS,
or provide a userspace configuration utility.
