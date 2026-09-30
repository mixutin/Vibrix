# Named package profiles

Vibrix defines five bounded package profiles using the same package-name and
capacity rules as the transactional package database:

| Profile | Initial membership | Purpose |
| --- | --- | --- |
| Minimal | `vibrix-shell` | Small native userspace base. |
| Developer | `vibrix-shell`, `vibrix-package`, `vibrix-ui` | Native development-oriented base components currently available in-tree. |
| Server | `vibrix-shell` | Conservative service host baseline; network services remain disabled by default. |
| Recovery | `vibrix-shell` | Rescue/admin baseline with no implicit network service enablement. |
| Security-lab | `vibrix-shell`, `vibrix-package` | Optional authorized testing profile, never part of the default installation. |

The Security-lab profile carries explicit policy flags requiring a disposable
workspace model, restricted persistent mounts, optional network isolation, and
network-facing services disabled by default. These policy flags are declarations
for later sandbox/profile orchestration; they do not themselves create a mount
or network namespace.

Profile membership is intentionally small until more packages exist. Expanding a
profile must continue to fit the fixed transactional profile capacity and must
not silently enable network-facing services.

This completes the bounded *definition* of the Minimal, Developer, Server,
Recovery and optional Security-lab profiles. It does not claim package payload
installation, persistent package state, repository signatures, sandbox namespace
enforcement, or a complete `vpm` UX.
