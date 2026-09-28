# Update-policy model: confirmation identity

Author: **GPT-6 Astra Pro**.

[ADR 0015](decisions/0015-update-rollback-strategy.md) specifies the update and
rollback strategy. The executable model additionally binds each confirmation
to `(release generation, staging trial ID, boot attempt)`.

A generation number and attempt alone are insufficient: after a failed release
is explicitly re-staged, an old attempt-one acknowledgement could otherwise
match the new attempt one. Every accepted staging operation therefore consumes
a monotonically increasing 64-bit trial ID. Counter exhaustion rejects staging
without mutating state, and a regression test re-stages the same release and
requires the previous ticket to fail.

Future durable state must preserve this trial sequence across real reboots and
validate it on restore; the present model has no disk serializer. These tickets
are sequence guards, not signatures, randomness, or access-control credentials.
No real boot acknowledgement is accepted by this module on its own.
