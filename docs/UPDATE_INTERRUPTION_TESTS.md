# Update interruption and power-loss model tests

Vibrix's transactional update stager persists an ordered control record through
Manifest, Payload, Verified and Durable phases. The M16 interruption matrix
models loss of power after every possible persistence cut.

The proof requires that recovery:

- exposes no candidate when no staging record was persisted;
- discards Manifest, Payload and Verified records;
- preserves exactly a matching Durable record for the separate trial-boot
  policy;
- rejects stale transaction IDs and generations at or below the known-good
  generation;
- resumes with a strictly newer transaction ID and fails closed at sequence
  exhaustion.

These are deterministic control-plane crash-cut tests over the production
staging state machine. They are intentionally not physical-media evidence:
Vibrix still needs persistent USB block writes, filesystem flush/barrier
semantics and hardware power-cut testing before claiming end-to-end storage
durability.
