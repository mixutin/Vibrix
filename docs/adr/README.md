# Architecture Decision Records

This directory contains Architecture Decision Records (ADRs) for Vibrix.

An ADR captures a significant architectural decision, its context, and its consequences. The goal is not bureaucracy — it is to make the project's reasoning understandable to future contributors (human and AI) who were not present when the decision was made.

## When to write an ADR

Write an ADR when:

- The decision affects multiple subsystems or the overall system shape
- The decision is difficult to reverse once implemented
- The reasoning behind the decision is non-obvious
- A rejected alternative had merit and the rationale should be preserved

Do **not** write an ADR for routine implementation choices, bug fixes, or anything easily derivable from the code.

## Status values

- **Proposed** — under discussion, not yet decided
- **Accepted** — decision made, implementation may or may not have started
- **Deprecated** — no longer in effect, retained for historical context
- **Superseded** — replaced by a newer ADR (reference the successor)

## Template

```markdown
# ADR-NNNN: Title

- **Status:** Accepted
- **Date:** YYYY-MM-DD
- **Deciders:** (who was involved)

## Context

What is the issue or situation that motivates this decision? What constraints or forces are at play?

## Decision

What was decided? State it clearly and concisely.

## Consequences

What becomes easier or harder as a result? What are the trade-offs?

## Alternatives considered

What other options were evaluated, and why were they rejected?

## References

Primary specifications, research, or discussions that informed the decision.
```

## Index

| ADR | Title | Status |
|-----|-------|--------|
| [0001](0001-use-rust.md) | Rust as the system implementation language | Accepted |
| [0002](0002-monolithic-kernel.md) | Monolithic kernel architecture | Accepted |
| [0003](0003-uefi-boot.md) | UEFI as the boot firmware interface | Accepted |
| [0004](0004-x86-64-primary.md) | x86-64 as the primary architecture | Accepted |
| [0005](0005-no-runtime-deps.md) | No third-party runtime crates | Accepted |
| [0006](0006-custom-kernel-target.md) | Custom kernel target specification | Accepted |
