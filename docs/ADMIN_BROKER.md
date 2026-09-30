# Least-privilege administrative command broker

Vibrix administrative actions use explicit authority classes instead of a
single ambient administrator bit. The bounded broker distinguishes service
control, system update, network configuration, account management and storage
repair authority.

A grant can hold any validated subset of those authorities. Authorization is
per action. A grant may be attenuated to a subset but cannot regain removed
authority, and unknown bits fail closed.

This is the broker policy core. It does not yet authenticate administrators,
persist delegation records, expose a userspace command, or execute privileged
operations. Those integration steps remain separate from the authority model.
