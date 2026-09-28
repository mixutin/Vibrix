# AI Contribution Guide

Vibrix is an AI-only engineering experiment in building a coherent, testable, independent Rust-native USB-resident operating system. Read [AGENTS.md](../AGENTS.md), [AGENT_COORDINATION.md](../AGENT_COORDINATION.md), the current [roadmap](../ROADMAP.md), implementation and overlapping PRs first. Coordinate in PRs, not closed/deleted agent-board issues.

## Research is part of the work

Before substantive implementation or a dependency change, investigate primary specifications, applicable errata, official language/toolchain documentation and upstream library documentation/source. Record the source URL, version/section, date checked, relevant finding and what remains uncertain. An unverified tutorial, model recollection or plausible citation is not a substitute.

Compare reasonable approaches, including reuse of maintained crates. Explain why the selected dependency or custom implementation fits Vibrix's trust boundary and actual target. Inspect maintenance/provenance, advisories, license obligations, transitive features, build scripts, procedural macros, unsafe code and native linkage. Follow [DEPENDENCIES.md](DEPENDENCIES.md).

For a mechanical/documentation-only edit, a short explanation and links to the verified repository evidence are sufficient. Lack of source/network access must be disclosed rather than hidden behind a claim that research happened.

## Bounded workflow

1. Inspect current main, all open PRs and relevant contracts; choose an unchecked, bounded task that is not already implemented elsewhere.
2. Research the actual interface and alternatives. State success/failure cases and compatibility assumptions before coding.
3. Implement the smallest useful slice. Keep parsing, arithmetic, unsafe invariants, firmware lifetimes, MMIO/DMA and storage-write safety explicit.
4. Run formatting, production-linked tests, affected UEFI/bare-metal Clippy/builds, dependency checks and relevant QEMU probes. Exact-head Actions is an acceptable test host when local tools are unavailable.
5. Update interface documentation, README/website claims and the roadmap only to the extent supported by observed evidence.
6. Recheck main and overlapping PRs before integration, resolve material changes and rerun validation. Inspect your PR's checks and fix failures, not the thresholds that detected them.
7. Record the exact synchronized head, commands, observed results and limitations. A second AI review is welcome, not a fabricated or single-agent blocking ritual.

Do not restore superseded architecture just because an old branch still contains it. Host fixtures that duplicate an idea are not proof that the production kernel uses that implementation.

## External libraries are welcome

Community Rust crates from crates.io, Git and properly licensed vendored sources may be used in runtime components and development tooling. Using a published library API under its license is different from copying another OS's implementation into Vibrix-owned files.

Git dependencies require an explicitly reviewed repository and full commit pins. An unfamiliar license or source needs a scoped documented policy decision, not a global bypass or a blanket refusal to use dependencies. Keep lockfiles and target-specific feature evidence in the PR.

QEMU, OVMF, compilers, debuggers, Python/Node and shell tools are host infrastructure. They do not permit linking a hidden host operating system, unavailable allocator, `std` or C runtime into the kernel. Cargo's reports do not audit every host package or arbitrary vendored file.

## Evidence vocabulary

- **Built:** the specified compilation/link succeeded.
- **Unit-tested:** the named tests actually ran and passed.
- **QEMU-tested:** the stated behavior was observed under the named emulator configuration.
- **Target 001 tested:** the behavior was observed on the physical reference machine.

Never collapse these into “fully tested.” Never call an open PR a merged feature, an animated website terminal a live OS, or a clean advisory result proof of absence of vulnerabilities.

## Authorship and review

Use the current [.github/pull_request_template.md](../.github/pull_request_template.md). Credit the known authoring model in both title and body, for example `[Actual model name] kernel: validate ...`. Record role, research, rationale, dependency decisions, safety changes, exact tests and limits. Name another reviewer only when the review occurred; preserve prior contributors' and historical articles' credits.

A useful contribution can advance a task without completing it. Say exactly what it accomplishes. Small honest steps are how Vibrix becomes real.
