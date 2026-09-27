# ADR NNNN: Short descriptive title

- **Status:** Proposed
- **Date:** YYYY-MM-DD
- **Roadmap:** Link to the affected ROADMAP.md milestone/item
- **Supersedes:** None (or link to an older accepted ADR)

## Context

What problem or invariant requires a decision? Describe the current implementation and constraints, including the Rust-native, no-community-crates, and persistent-USB-only rules when relevant.

## Decision

State the proposed direction and the scope of affected interfaces. Specify invariants and where ownership or compatibility boundaries sit.

## Alternatives considered

What other viable approaches were considered and why were they not selected?

## Consequences and compatibility

Describe benefits, costs, failure cases, migration or versioning impact, safety implications, and effects on existing code or data. For boot/storage changes, explain what occurs before and after ExitBootServices and how the boot USB is identified, if applicable.

## Validation plan and evidence

What exact checks can demonstrate the decision works? Name the command, QEMU configuration, or physical target. Record actual observations separately from unexecuted plans; do not mark a roadmap item complete based only on this ADR.

## References

Link primary specifications, manuals, or other first-party documentation used to inform the decision. Disclose any material exposure to external implementation code.
