# External dependencies and supply-chain policy

**External libraries are welcome.** Vibrix permits community Rust crates from crates.io, Git repositories and properly licensed vendored sources throughout the loader, kernel, drivers, system libraries, userspace, installer, package tooling and host development tools. Use good libraries when they improve safety, simplicity or maintainability. Independence is ownership of Vibrix's architecture, not compulsory reinvention.

Allowed does not mean unreviewed or universally target-compatible. This policy replaces any blanket prohibition on third-party crates; it does not disable security, licensing or Rust-native/USB-only constraints.

## Admission paths

| Source | How to use it |
| --- | --- |
| crates.io | Declare a deliberate version/features in Cargo.toml, commit Cargo.lock, satisfy the existing license/advisory policy and test the affected targets. crates.io is already an allowed registry. |
| Git | Research the upstream repository; add its exact URL to `sources.allow-git` in deny.toml in the same PR. Use a full 40-character commit `rev`, never a floating branch/tag or abbreviated hash. The lockfile must resolve that exact commit. |
| Vendored/path crate | Retain upstream licenses/notices, source URL, exact revision and identifiable local changes. Declare the crate in Cargo's graph so it is scanned. Path metadata alone does not establish provenance. |
| Additional registry | Justify and explicitly allow the specific registry after a provenance/security review. Do not allow all unknown registries. |
| Host tool or another package ecosystem | Use verified official distribution sources, pin versions/digests where appropriate, document its host-only role and add relevant manifest/lockfile/update/scanning coverage before relying on it. Do not claim Cargo scans it automatically. |

`unknown-git = "deny"` means **unreviewed repositories fail**, not “Git dependencies forbidden.” The empty `allow-git` list reflects that no Git crate has yet been admitted in this baseline. A dependency PR may add a researched repository without changing the policy to globally trust GitHub or every source. `required-git-spec = "rev"` and the inventory helper enforce immutable Git inputs, including transitive Git sources.

## Required research record

Before adding/upgrading a dependency, record the package, exact version/revision, use case and primary upstream URLs in the PR. Compare an appropriate maintained crate with the reasonable alternatives; custom code is permitted but not inherently safer.

Check the actual license expression and bundled assets/notices, maintenance and provenance, security advisories, transitive packages and feature expansion. Inspect unsafe behavior and host-executed build scripts/procedural macros appropriate to the trust boundary. Record the date/version researched, decisions and outstanding limitations. Cite primary docs/source rather than inventing confidence from download counts.

The PR must explain whether the code ships in Vibrix or only runs on the host. Build-time code can still affect the resulting binary; “development dependency” does not mean risk-free.

## Licensing is not a same-license requirement

First-party Vibrix is 0BSD. Dependencies retain their own licenses and distribution obligations. The current reviewed identifiers are in `deny.toml`: 0BSD, MIT, Apache-2.0, Apache-2.0 WITH LLVM-exception, BSD-2-Clause, BSD-3-Clause, ISC, Unicode-3.0 and Zlib.

A different license is not automatically disallowed forever. Investigate its terms and the intended distribution, retain the necessary notices, and propose an explicit addition or package-scoped exception with rationale. Missing/unclear licensing must be resolved, not silently treated as 0BSD. Automated license matching is a screening aid, not legal advice or a guarantee of compatibility.

Never silence a scanner globally to pass a dependency PR. Any necessary source/license/advisory exception must be narrow, justified, attributable to an owner, and have a review date. Do not blanket-ignore RustSec findings or lower thresholds to hide a vulnerability. Prefer an upgrade, replacement, feature removal or upstream fix.

## Actual target compatibility

The loader targets `x86_64-unknown-uefi`; the `no_std` kernel targets `x86_64-unknown-none`. Host compilation alone proves neither. Disable default features when required, select only capabilities the component supplies, and demonstrate actual-target builds and applicable QEMU behavior.

Do not silently import a host OS, `std`, a C/C++ runtime, dynamic loading or an unavailable allocator into Vibrix. A crate may implement parsing, UEFI support, allocation, networking or filesystem facilities, but Vibrix still owns the boot ABI, safety invariants, subsystem integration and persistent removable-root model. Using a crate does not complete a roadmap checkbox.

Tiny documented hardware assembly remains allowed. Using QEMU/OVMF/GDB/Python/Node on the host does not make them Vibrix runtime components. Do not copy or lightly translate another OS's implementation into first-party files; see [INDEPENDENCE.md](INDEPENDENCE.md).

## What CI checks

| Layer | Coverage and behavior |
| --- | --- |
| Locked resolution/builds | Committed Cargo.lock, `cargo metadata --locked --all-features`, target feature trees, locked Clippy/builds and no-default-feature checks on both targets. Missing/stale locks fail rather than being regenerated by CI. |
| RustSec / cargo-audit | Known Rust advisories against the complete committed lockfile, on PR/main/merge-group and daily/manual runs. |
| cargo-deny | All-features Cargo graph: advisories, yanked packages, licenses, wildcard bans and allowed sources. Duplicate versions remain warnings, not a gratuitous library ban. |
| Dependency review | PR-introduced known vulnerabilities at low or higher severity across runtime, development and unknown scopes supported by GitHub. cargo-deny remains the single license allowlist. |
| Inventory artifacts | Package/source/license/feature/dependency information plus build-script, proc-macro and native `links` indicators; full metadata and target feature trees. This is a review inventory, **not** a standards-compliant whole-OS SBOM. |
| Workflow hygiene | actionlint plus regression-tested full-SHA/digest checks for the repository's block-style external `uses:` entries. Local workflows remain allowed. No write token is needed for PR validation. |
| Updates | Dependabot proposes Cargo and GitHub Actions updates weekly. Pinned scanner versions, actionlint and the Rust nightly require deliberate update PRs; this Dependabot configuration does not automatically upgrade every host tool. |

Reports are retained for 14 days. Fresh scheduled scans catch newly published advisories without needing a source change. Network/scanner failure is not converted into a clean scan. See [CI.md](CI.md) for checks and evidence locations.

## Coverage limits

No scanner guarantees absence of malware, unknown vulnerabilities, unsound unsafe code or license obligations. Cargo does not audit arbitrary copied source, unregistered vendored files, firmware, the compiler, the complete Ubuntu/QEMU installation, browser assets or future npm/Python package graphs. Its metadata flags are review prompts, not proof of the absence of undeclared native behavior.

Keep external code inventoried. When adding a new package ecosystem, add its own lockfile, update policy and appropriate scanning instead of claiming RustSec covers it. Review downloaded executables and build commands as code execution, and never run an unverified remote install script simply because a tutorial suggests it.

## Local workflow

After deliberately updating a dependency and committing the resulting lockfile:

```bash
cargo metadata --locked --all-features --format-version 1 > metadata.json
python3 tools/dependency_inventory.py --metadata metadata.json --output inventory.json
cargo audit --file Cargo.lock
cargo deny --locked --all-features check advisories bans licenses sources
cargo check --locked -p vibrix-boot --no-default-features --target x86_64-unknown-uefi
cargo check --locked -p vibrix-kernel --no-default-features --target x86_64-unknown-none
./tools/test-qemu.sh
```

Install the versions of cargo-audit/cargo-deny recorded in `.github/workflows/dependencies.yml`. Do not use `cargo generate-lockfile` as a CI verification step: it is an update operation. Use locked resolution to validate the committed result.

## Primary references checked 2026-09-28

- [Cargo metadata and --locked](https://doc.rust-lang.org/cargo/commands/cargo-metadata.html)
- [cargo-deny common options](https://embarkstudios.github.io/cargo-deny/cli/common.html) and [source configuration](https://embarkstudios.github.io/cargo-deny/checks/sources/cfg.html)
- [Official dependency-review action configuration](https://github.com/actions/dependency-review-action)
- [Official actionlint documentation](https://github.com/rhysd/actionlint)
