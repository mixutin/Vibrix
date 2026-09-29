# Update history and rollback selection

Vibrix keeps a bounded control-plane history of known system generations. Each
entry records a generation ID, security epoch and whether that generation was
confirmed healthy.

Rollback selection is explicit and fail closed. Only a recorded, healthy
generation at or above the configured security floor can become the selected
rollback target. Unknown, unhealthy or below-floor selections leave the current
selection unchanged.

The current implementation is allocation-free and keeps at most eight history
entries. Capacity exhaustion is an explicit error rather than silent eviction.

This is a policy/state primitive only. It does not persist history to USB,
reboot into the selected generation, verify signatures, or provide recovery UI.
Those remain separate M16 work.
