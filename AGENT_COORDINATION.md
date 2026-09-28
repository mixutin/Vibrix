# Vibrix Agent Coordination

## Coordinate in PRs, not agent boards

Use current repository state and the affected pull requests for scope, overlap and handoffs. The owner rejected the board-based workflow; #46 is closed and #14 was deleted. Do not create replacement boards, reopen them or require a claim on an unavailable issue.

Before substantial work, inspect current main and open PRs. State the agent/model, bounded scope, branch, affected files/contracts, synchronized SHA and known overlap in the PR. Update that description/comment when the scope or base changes. Link dependent PRs and their merge order rather than implying they have already landed.

## Single-agent default

When one agent is working, it may implement, test, open PRs and merge its own bounded changes without waiting for another agent or maintainer to post a review, under the owner's standing authorization of 2026-09-27. Never manufacture approval. Additional AI review is welcome when available; absent peers cannot block progress.

## Shared contracts and research

BootInfo, firmware exit, page tables, syscall/executable ABI, VFS, process/driver models, persistent USB identity and on-disk/package formats require explicit research, design rationale and compatibility notes in the PR or an ADR. Follow the research requirements in [AGENTS.md](AGENTS.md). Notify overlapping contributors in the relevant PR discussion, not through a separate board ritual.

## Integration evidence

Immediately before integration, inspect the newest main and open PRs, synchronize materially changed assumptions, and rerun applicable formatting, production-linked host tests, target Clippy/builds, dependency checks and QEMU verification. Use the exact head's successful Actions run when it is the available test host. Stale, failed or cancelled runs do not count.

Record actual results and limits. Release or hand off work in its PR. A roadmap checkbox requires implemented and observed behavior on its stated target; no process waiver makes a stub complete. USB system/root storage remains removable-only and provisioning must never erase disks without explicit opt-in.
