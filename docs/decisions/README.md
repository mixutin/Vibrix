# Architecture decision records (ADRs)

Vibrix records architectural decisions so that low-level interfaces remain understandable and changes can be reviewed independently of generated code.

## When to write an ADR

Propose an ADR before changing a shared contract such as BootInfo, the syscall or executable ABI, VFS, process/driver model, on-disk filesystem format, or package format. Also use one for cross-subsystem decisions about the persistent USB root, boot-device identity, firmware handoff, or an irreversible choice. Small implementation details need no ADR.

## Workflow

1. Copy [0000-template.md](0000-template.md) to the next unused four-digit number, e.g. `0001-short-title.md`. Reserve `0000` for the template. Never reuse a number, even if a proposal is rejected.
2. Set **Status** to `Proposed` and describe the decision, context, alternatives, safety/compatibility consequences, and evidence. Link relevant roadmap tasks and primary specifications. Distinguish observed test results from plans; do not claim unrun tests.
3. Open a pull request with the shared-contract rationale and compatibility consequences. Invite another AI agent to review if available; a sole validated agent may integrate under AGENTS.md. Resolve substantive concerns in the discussion.
4. When an authorized active agent or maintainer accepts the decision, merge the ADR with **Status** set to `Accepted`. A merged proposal is not implicitly accepted if the status remains `Proposed`. Implementation and its tests can follow in a separate focused PR.
5. To replace an accepted decision, add a new ADR with **Status** `Proposed` and a **Supersedes** link. Once accepted, update the earlier ADR to `Superseded by [NNNN]` in the same PR. Do not rewrite the historical rationale.
6. Rejected proposals remain in the directory with **Status** `Rejected` and a brief reason, preserving the history.

## Status values

- `Proposed`: awaiting a decision; not an implementation requirement.
- `Accepted`: reviewed and approved as project direction; implementation still needs evidence.
- `Rejected`: considered but not adopted.
- `Superseded by [NNNN]`: replaced by the linked accepted decision.

Keep every ADR in plain Markdown and link it from relevant code/docs when its contract is implemented. An ADR is not proof that code builds, boots under QEMU, or works on hardware.
