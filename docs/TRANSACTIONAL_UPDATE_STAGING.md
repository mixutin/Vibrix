# Transactional update staging

This M16 slice defines the control-plane transaction that must complete before a
candidate system generation may enter Vibrix's existing trial-boot policy.

The kernel model has four ordered phases:

1. manifest recorded;
2. payload written;
3. candidate independently verified;
4. verified candidate durably staged.

Each transition is bound to a monotonically increasing transaction identifier.
Out-of-order and stale transitions fail without mutating the active staging
record. A candidate is never promoted to known-good by this module; a durable
candidate is handed to the separate trial/health/rollback policy.

## Interruption recovery

The recovery constructor models the record that would have survived an abrupt
reset. Records from the manifest, payload or verified-but-not-durable phases are
discarded. Only the exact latest transaction in the durable phase, with a
generation newer than known-good, is retained. Stale transactions and rollback
generations fail closed.

The tests exercise interruption after every stage and verify failure atomicity.
The normal post-firmware kernel self-test runs the same state machine before
emitting its evidence marker.

## Boundary

This is a fixed-capacity executable policy model. It does not yet write a
Vibrix USB, perform cache flush/FUA operations, validate signatures, fetch
artifacts, switch boot selectors, or claim physical power-loss durability.
Those require the persistent native USB block path. The model defines the
ordering and recovery invariants that storage integration must preserve.
