# Secure random service

Vibrix M11 secure randomness uses the x86-64 RDSEED instruction as a bounded
hardware entropy source. This first implementation deliberately has no fallback
to timers, MAC addresses, disk identifiers, firmware-map values, or an
unverified RDRAND-only construction.

## Contract

- CPUID leaf 7 EBX bit 18 must advertise RDSEED.
- Every RDSEED result is accepted only when the architectural carry flag is set.
- Each 64-bit output gets at most 64 attempts, with PAUSE-friendly spin waiting.
- Consecutive identical 64-bit accepted samples fail closed as a continuous
  catastrophic-source check.
- Requests are limited to 256 bytes and are transactional: any failure leaves
  the caller buffer unchanged.
- Calls are globally serialized so the continuous comparison remains coherent
  when SMP work begins.
- Unsupported or temporarily exhausted hardware returns an error. Vibrix does
  not manufacture entropy from low-quality identifiers.

The API currently exposes random bytes to the kernel only. A userspace
getrandom-style syscall, entropy-pool mixing, persistence, blocking semantics,
fork reseeding, and non-x86 entropy sources remain future work.

## Research and rationale

Primary sources checked 2026-09-29:

- Intel Digital Random Number Generator Software Implementation Guide, revision
  2.1 and current Intel publication. Intel documents RDSEED as the interface to
  the entropy-source/conditioning path for seeding software PRNGs, requires
  CPUID feature detection and CF checking, and recommends bounded retries with
  PAUSE for asynchronous consumers:
  https://www.intel.com/content/www/us/en/developer/articles/guide/intel-digital-random-number-generator-drng-software-implementation-guide.htm
- NIST SP 800-90B, Recommendation for the Entropy Sources Used for Random Bit
  Generation, section 4.4 and health-test requirements:
  https://nvlpubs.nist.gov/nistpubs/SpecialPublications/nist.sp.800-90b.pdf

Intel's DRNG already contains its own startup/runtime health machinery. Vibrix's
duplicate-word rejection is an additional fail-closed software diagnostic; it
is not claimed to be a complete SP 800-90B validation program.

A software ChaCha20 DRBG was considered. RustCrypto's maintained no_std
ChaCha20 implementation is a reasonable future choice if Vibrix needs a
high-throughput seeded DRBG, but direct RDSEED avoids adding a cryptographic
dependency and state-reseed policy before the kernel needs that throughput.
The tradeoff is deliberate: this initial service requires RDSEED-capable x86-64
hardware and can report temporary unavailability.
