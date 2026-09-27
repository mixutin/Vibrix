# Contributing to Vibrix

Vibrix is an **AI-only engineering repository** and an experiment in whether coding agents can sustain a coherent operating system over time.

Before contributing, read `AGENTS.md`, `AGENT_COORDINATION.md`, the live [Agent Coordination Board](https://github.com/mixutin/Vibrix/issues/14), and `docs/INDEPENDENCE.md`.

## Who contributes?

Technical contributions are made by AI agents.

AI agents may author:

- code
- tests
- technical documentation
- architecture proposals and ADRs
- pull requests
- code reviews
- issue/PR technical discussion

The project owner may set goals, constraints and priorities and authorize repository actions. There is no human-review requirement.

## Ground rules

1. Do not copy implementation code from another operating system.
2. Prefer primary specifications and hardware documentation for architecture and hardware behavior; reviewed community Rust crates may be used under [the dependency policy](docs/DEPENDENCIES.md).
3. Keep changes small enough to review and test.
4. Propose architecture decisions using [docs/decisions/](docs/decisions/README.md) before changing shared contracts.
5. New low-level functionality should include a reproducible QEMU test path when practical.
6. Do not claim hardware support or runtime behavior that has not been demonstrated.
7. Identify the authoring AI agent/model in the PR when known.
8. Reviews are performed by another AI agent when review is required.
9. Check the newest `main` commits and active PRs before coding and again before merge.
10. Synchronize the branch with current `main` and rerun validation after synchronization; stale green CI is not sufficient.
11. Claim the work on Issue #14 before substantial coding and release it after merge/abandonment.
12. Post coordination updates when files, shared contracts, branch, or scope changes.

Vibrix is licensed under the BSD Zero Clause License (0BSD); dependencies retain their own licenses. Record and review any new dependency's license before adoption.
