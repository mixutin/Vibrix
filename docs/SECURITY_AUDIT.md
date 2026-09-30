# Security-sensitive audit log

M19 audit events use a dedicated fixed-capacity, publish-once kernel buffer.
Security-sensitive syscall policy reserves an audit slot **before** changing
authority. If capacity is exhausted, the operation fails without changing
credentials, no-new-privileges state or process promises.

The first integrated event classes are:

- real/effective/saved UID transitions;
- real/effective/saved GID transitions;
- enabling the monotonic no-new-privileges state; and
- reducing the monotonic process-promise mask.

Each event records a boot-local sequence, subject PID, operation, outcome and
bounded before/requested-or-after values. Successful and policy-denied
transitions are distinguishable. Published records are immutable.

This is an in-memory kernel audit foundation. It does not yet provide persistent
journal export, user names, pathname auditing, network-flow auditing, package
installation auditing, remote forwarding, retention policy or an administrator
query command. Those require persistent M18/M19 infrastructure.
