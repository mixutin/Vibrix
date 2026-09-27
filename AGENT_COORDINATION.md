# Vibrix Agent Coordination

**Single-agent default:** When one agent is working, it may implement, test,
open PRs and merge its own bounded changes without waiting for another AI
agent or maintainer to post a review. The project owner explicitly authorized
this workflow on 2026-09-27. Do not manufacture review approvals.

**Multi-agent option:** When multiple agents work concurrently, use the
[Agent Coordination Board #46](https://github.com/mixutin/Vibrix/issues/46)
as a comment ledger for claims and handoffs. Issue #14 was deleted; it is
not a prerequisite or a valid coordination destination. If the board is
unavailable, document overlap and synchronization in the PR.

## Agent lifecycle (when concurrently active)

- **AGENT CLAIM:** agent/model, bounded scope and branch, roadmap item,
  affected files/contracts, current main SHA and competing PRs.
- **AGENT UPDATE:** changed scope, contract, branch or main.
- **AGENT READY:** synchronized head and exact validation evidence and limits.
- **AGENT RELEASE:** after merging or abandoning a lane.

These comments are helpful coordination, **not blocking approval rituals**
when no other agent is participating.

## Shared architecture contracts

BootInfo, firmware exit, page tables, syscall/executable ABI, VFS,
process/driver model, persistent USB identity, on-disk filesystem and
package formats require explicit design rationale and compatibility notes
in a PR or ADR. A second agent's signoff is welcome, not mandatory.
Do not combine unrelated, unsafe rewrites merely to accumulate checkmarks.

## Required integration evidence

Before merge, inspect the newest `main` and open PRs; synchronize any
materially changed assumptions; run formatting and available relevant host
tests, Clippy, builds and QEMU boot verification for low-level changes.
When GitHub Actions is the only QEMU host, its exact commit/head run is
valid evidence. A failed or stale run is not.

A roadmap checkbox requires **implemented and observed behavior on its
stated target**. No coordination or review waiver makes a stub complete.
USB system/root storage must remain removable-only; provisioning must not
erase disks without explicit opt-in.
