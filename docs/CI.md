# Continuous integration and evidence

Vibrix CI checks policy and dependency changes, builds the real UEFI loader and bare-metal kernel, runs production-linked host tests, and boots the resulting EFI tree in QEMU/OVMF. Source configuration is authoritative: [ci.yml](../.github/workflows/ci.yml), [dependencies.yml](../.github/workflows/dependencies.yml) and [pages.yml](../.github/workflows/pages.yml).

## Required-check layout

`Vibrix CI` runs on PRs, pushes to main, merge groups and manual dispatch. Outdated PR runs are cancelled; main runs are not cancelled by this PR-only rule. Jobs use read-only repository permissions and checkout without persisted credentials.

- **Workflow and policy checks:** policy-helper regression tests, immutable block-style Action references, tracked Cargo.lock, pinned actionlint and website JavaScript syntax.
- **Dependency supply-chain gate / Locked Cargo graph audit:** full locked metadata/feature inventory, RustSec and cargo-deny. The same reusable workflow also runs daily at **05:23 UTC** and by manual dispatch.
- **Review dependency changes:** PR-only review of newly introduced known vulnerabilities across runtime/development/unknown scopes. It is intentionally skipped on other events; full graph auditing still runs there.
- **Minimal features (vibrix-boot / vibrix-kernel):** locked checks without default features on `x86_64-unknown-uefi` and `x86_64-unknown-none` respectively.
- **build-and-smoke-test:** the existing production tests, actual-target Clippy/builds and QEMU behavior probes described below.
- **CI gate:** always evaluates the preceding jobs. Failure, cancellation or an unexpected skip fails the aggregate gate; only the non-PR dependency-review skip is expected.
- **Website checks:** the Pages workflow validates local links/fragments, JSON-LD, homepage/status metadata and tested browser-data helpers without deploying PR content. Deployment gets Pages/OIDC write permissions only in its separate main-branch job after these checks pass.

Configure repository rules to require **CI gate** and **Website checks** before merging, with up-to-date branch/merge-queue validation as appropriate. **YAML does not enable branch protection.** At the 2026-09-28 audit, main had no required-check protection. This pass adds check names, not a claim that repository administration settings changed. Until rules are configured, agents must still honor exact-head passing checks under project policy.

## Reproducible inputs

Cargo.lock is committed because Vibrix builds an OS/application, not just a reusable library. Metadata, feature trees, target checks, Clippy and the QEMU build script use `--locked`. CI must not silently regenerate dependency selections. Deliberate dependency changes update and commit the lockfile in their own PR.

The Rust toolchain is pinned to `nightly-2026-09-28`, the nightly observed in successful main [run 36388253686](https://github.com/mixutin/Vibrix/actions/runs/36388253686). Rustup installation explicitly requests the required components and both targets. Scanner versions and Action SHAs are pinned in workflow source. These pins improve reproducibility but do **not** make the complete hosted runner, apt package set or final binaries bit-for-bit reproducible.

## Dependency evidence and limits

The 14-day `dependency-reports-<run>-<attempt>` artifact contains Cargo metadata, a review inventory, UEFI/kernel feature trees, scanner outputs and tool versions. Inventory includes source/revision, license, dependency edges and enabled features, plus build-script/procedural-macro/native-link indicators. It is not a complete installed-system SBOM or proof that a library is safe.

Both RustSec and cargo-deny retain their failure status. The deny checks still run after an audit finding when graph/tool setup succeeded, and report uploads run on success or failure. An unavailable database/scanner is not a successful audit.

Community crates remain permitted. Git repositories and unfamiliar licenses have explicit researched admission paths; see [DEPENDENCIES.md](DEPENDENCIES.md). Do not silence security findings to admit a library. Standalone host binaries, the runner, compiler, firmware and arbitrary vendored source are not magically covered by a Cargo scan.

## Build and host regression coverage

CI preserves script executable-mode and shell-syntax checks, OVMF discovery fixtures, cargo formatting, UEFI and bare-metal Clippy with warnings denied, and both actual builds.

Host tests exercise ELF64 parsing, BootInfo v3 validation, firmware memory maps and loader cleanup, transition preflight, bounded framebuffer writes, ACPI/MCFG and PCI/BAR parsing, physical frames, early heap/mapping window, CPUID, GDT/TSS, IDT layout, IRQ routing, bounded console editing/dispatch, native serial logic, the PS/2 decoder and the discovery/driver-candidate model. Legacy self-contained host fixtures remain explicitly distinguished from production-linked evidence.

Storage tests include safe regular-file GPT creation and inspection for 512/4096-byte logical sectors, exactly one ESP plus one Vibrix System partition, and rejection of overwrites. VibrixFS host wire/journal/image tests include format/inspect round trips, metadata checks and corruption rejection. These do not mount persistent root or write a physical drive.

Standalone `rustc --test` harnesses cannot automatically link future external crate dependencies. An agent adding a crate to one of these modules must migrate the affected tests to a production-linked Cargo harness or otherwise supply the real dependency, retaining the tests rather than removing them to pass CI.

## QEMU behavior coverage

The production loader/kernel must demonstrate the real post-ExitBootServices handoff and independent native COM1 output. Loader messages cannot substitute for kernel evidence. The current suite exercises normal boot with timer IRQ delivery, PCI discovery with a virtual xHCI controller, real virtual keyboard input, and bounded console editing/commands. Separate kernels deliberately exercise panic, returning breakpoint, page-fault diagnostics, supervisor write protection and access after unmap.

**Current verified console baseline:** main `cfc8bd1a6e7cfce8492eecaef7ba746ac8841aaa`, [run 36388253686](https://github.com/mixutin/Vibrix/actions/runs/36388253686), 2026-09-28. PR #89's [run 36386907847](https://github.com/mixutin/Vibrix/actions/runs/36386907847) additionally records the scoped console-completion evidence. The real q35 kernel accepts edited `help`, `clear`, `mem`, `pci`, `acpi`, `uptime` and `reboot` via virtual keyboard injection; reboot must actually terminate QEMU under `-no-reboot`, not merely print a success string. `info` and malformed/unknown command behavior also have production-module host coverage.

This is a **polled PS/2 development console with native serial output and a QEMU timer IRQ path**. It is not an IRQ-driven USB keyboard, framebuffer terminal, Ring-3 shell, native USB-storage driver, mounted filesystem or physical Target 001 validation. Memory/PCI/ACPI diagnostic snapshots are bounded early-boot state, not a claim of general live system monitoring.

## Diagnostics

Run the interactive build with `./tools/run-qemu.sh` or automated regression with `./tools/test-qemu.sh`. See [QEMU.md](QEMU.md) for prerequisites and controls. Interactive logs are `build/qemu/interactive-debugcon.log` and `interactive-serial.log`; smoke logs are `build/qemu/debugcon.log` and `serial.log`. The framebuffer is not a full terminal.

CI preserves the **most recent** QEMU `.log` files for seven days even on failure. The build script recreates its output directory for each probe, so this artifact is not an archive of every successful earlier boot; the Actions step logs retain the broader run history. Inspect the failed step and its head SHA first.

## Historical evidence

These runs prove their original narrower behavior, not the latest repository state: [initial handoff / BootInfo v2](https://github.com/mixutin/Vibrix/actions/runs/36337520346), [early frame issuance](https://github.com/mixutin/Vibrix/actions/runs/36339966455), [native IDT and fault diagnostics](https://github.com/mixutin/Vibrix/actions/runs/36340579141), and [kernel panic](https://github.com/mixutin/Vibrix/actions/runs/36337648665). Current BootInfo is v3. Earlier claims that IRQ/console work had not landed describe historical revisions, not current main.

No emulator run establishes Target 001 support. Record a separate actual physical boot before making that claim. Exact-head evidence, not a badge or generated transcript, is the integration criterion.


## Native desktop profile (2026-09-29)

The [userspace-desktop workflow](../.github/workflows/userspace-desktop.yml) adds **Native desktop, terminal and pointer pixels**. It tests the actual shared ABI, stride/canaries and RGB/BGR conversion, keyboard/mouse/FIFO decoding, window/terminal state and bounded renderer, then boots `--desktop` and checks real CPL3 pointer rejection, terminal file commands, file previews, mouse clicks/dragging and maximize/restore against QEMU screendumps. The minimal-feature matrix also includes the new first-party desktop crate. Existing canonical, dependency and shell-display checks remain required.

The artifact stores real `desktop-*.ppm`, `desktop-result.json` and boot logs. The JSON records the source commit and dirty-worktree status; it explicitly reports Chromium as not running. Desktop evidence must pass on the actual PR head before integration alongside **CI gate**, **Website checks** and the default shell regression. A busy-polling single-process QEMU preview does not establish browser isolation, physical hardware support, USB persistence or a production compositor. See [DESKTOP.md](DESKTOP.md).
