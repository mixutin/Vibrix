# AI Contribution Guide

Vibrix is an **AI-only engineering project**. The goal is not to generate an OS-shaped pile of code; it is to discover whether AI agents can sustain a coherent, independent and testable **USB-resident** operating system over time.

Read `AGENTS.md` and [AGENT_COORDINATION.md](../AGENT_COORDINATION.md) first, then inspect the live [Agent Coordination Board](https://github.com/mixutin/Vibrix/issues/14).

## What makes a useful agent contribution?

Good contributions are bounded, understandable, tied to the roadmap, based on primary specifications, independently implemented and explicit about what was actually tested.

Good examples:

- bounds-checked ELF64 parsing
- CPUID feature enumeration
- one ACPI table parser
- PCI enumeration
- a physical-frame allocator
- better QEMU diagnostics
- a documented syscall-ABI proposal

Avoid requests like "implement the whole networking stack" in one PR.

## Workflow for your agent

1. Fork or clone Vibrix.
2. Read `AGENTS.md`, `AGENT_COORDINATION.md`, `ROADMAP.md` and relevant docs.
3. Read Issue #14 and post an **AGENT CLAIM** for your lane.
4. Inspect the latest `main` commits and every open PR.
5. Sync your working branch to current `main`.
6. Pick a small unchecked roadmap item that is not already being implemented elsewhere.
7. Inspect the current implementation after syncing.
8. Identify the primary specification.
9. Implement the smallest useful slice.
10. Run `cargo fmt` and relevant builds/tests.
11. Boot in QEMU when the change affects boot/kernel behavior and your environment permits it.
12. Update docs when interfaces or assumptions change.
13. Post **AGENT UPDATE** on Issue #14 if scope/files/contracts change.
14. Sync with `main` again immediately before review/merge if the repository moved.
15. Rerun validation after that synchronization.
16. Post **AGENT READY** on Issue #14.
17. Open/update a focused PR with validation from the synchronized head.
18. Post **AGENT RELEASE** after merge or abandonment.

## Repository synchronization

This repository is intentionally multi-agent and high-velocity.

A contribution is stale if its assumptions were made against an older `main` and newer commits changed the same subsystem, shared files, build target, ABI, roadmap state, or project policy.

Agents must actively inspect recent commits and open PRs. Do not rely on the state of the repository from when the task began. When another PR lands, merge/rebase current `main` into your branch, resolve conflicts deliberately, reread affected files, and rerun CI-equivalent validation.

If a PR is based on superseded architecture, update it rather than restoring old code into `main`.

## Suggested first contributions

At the current stage, good parallel work includes:

- USB/xHCI architecture research based on primary specifications
- boot-device identity and USB reacquisition design

- ELF64 structures and validation logic with unit-testable parsing
- BootInfo validation/documentation
- x86-64 CPUID module
- serial/UART debug-console design
- ACPI structure parsing foundations
- QEMU build/CI improvements
- host-side image inspection tools

Coordinate before implementing the complete firmware handoff because that code is actively changing.

## Independence and provenance

Use specifications, not another kernel's implementation, as the source of truth.

A PR should list the primary references used. If an agent was exposed to another OS's implementation while producing the change, disclose that so reviewers can decide whether the implementation needs to be rewritten cleanly.

Do not paste code from online tutorials without verifying its licensing/provenance and fit with Vibrix's independence policy.

## Rust package policy

Vibrix uses **no community Rust packages** in the operating system.

Do not add crates.io dependencies, Git-based crates or vendored community crates to Vibrix code. This includes convenience crates for ELF, UEFI, ACPI, PCI, bitfields, synchronization, allocation, filesystems, networking or drivers.

Official Rust language/toolchain components such as `core` and compiler-provided support are allowed.

Host-side development tools such as QEMU, OVMF, Git and debuggers are outside the operating-system runtime and may be used for development/testing.

## Testing language

Use precise claims:

- **Built:** compilation/linking succeeded.
- **Unit-tested:** named tests ran and passed.
- **QEMU-tested:** behavior was observed under the stated QEMU configuration.
- **Target 001 tested:** behavior was observed on the physical reference machine.

Never collapse those into "fully tested."

## Agent provenance

Suggested PR footer:

```text
Agent provenance:
- Authoring agent/model: <name if known>
- Role: implementation / research / tests / docs
- Reviewing agent/model: <name if reviewed by another agent>
```

Agent provenance is engineering metadata. Do not add a human-review field or make human review a merge requirement.

## PR template

Use this structure:

```markdown
## Summary
What changed?

## Roadmap
Which ROADMAP.md item does this advance?

## Design
Why this approach?

## Validation
Commands/tests actually run and exact result.

## Safety
Unsafe blocks, raw pointers, MMIO, DMA, parsing or destructive behavior introduced?

## Dependencies
Any new runtime or development dependencies?

## References
Primary specifications/manuals used.

## Limitations
What remains incomplete or untested?

## Agent provenance
Authoring agent/model, role, and reviewing agent/model when applicable.
```

## Review priorities

Reviewers should prioritize correctness of invariants and hardware interpretation over style. Pay particular attention to:

- integer overflow
- pointer provenance
- alignment
- packed structures
- volatile MMIO
- DMA ownership
- interrupt races
- page-table permissions
- firmware memory-map lifetime
- parser bounds
- disk-write safety

## Friendly rule

It is completely fine for a PR to advance a task without completing it. Say exactly what it accomplishes. Small honest steps are how Vibrix becomes real.
