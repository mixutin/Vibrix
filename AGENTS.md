# AGENTS.md — Vibrix Agent Instructions

These instructions apply to every AI coding agent working in this repository.

Vibrix is an **AI-only engineering project**. Code, technical documentation, pull requests, reviews and architecture discussion are produced by AI agents. The project owner may provide direction and authorize actions, but there is no human-review gate.

## Mission

Build Vibrix: an independent, Rust-native Unix-like OS implemented from scratch that lives persistently on a removable USB drive. Vibrix is also an experiment in how far AI-assisted engineering can go while remaining understandable, testable and technically honest.

## Prime directive: do not fake progress

Generated code is not evidence that a feature works. Mark a roadmap item complete only after its stated behavior is demonstrated on QEMU or named hardware. Never invent test results.

## Independence

Read `docs/INDEPENDENCE.md`.

Allowed references include primary hardware/architecture manuals, UEFI/ACPI/PCI/USB/NVMe specifications, standards, language/toolchain docs and research papers.

Do not copy or translate implementation code from Linux, BSD, GNU userspace, third-party bootloaders, libc implementations, drivers or filesystems. Do not quietly add another OS as a runtime layer.

If external implementation source materially influenced a contribution, disclose it.

## Rust-native policy

Use Rust for bootloader, kernel, drivers, system libraries, userspace, filesystem, networking, installer and package tooling.

Tiny architecture-specific assembly is allowed only where hardware requires it. Isolate and document it. Do not introduce a C/C++ runtime.

## Dependencies

**Community Rust packages are forbidden in Vibrix.**

Do not add crates.io dependencies, Git-based Rust dependencies, vendored third-party crates, or community packages to the bootloader, kernel, drivers, system libraries, installer, package tooling or first-party userspace.

The only Rust foundation/runtime code allowed is what is supplied as part of the official Rust language/toolchain itself, such as `core` and compiler support.

If functionality is needed for ELF, UEFI, ACPI, PCI, USB, NVMe, synchronization, allocation, parsing, bitfields, filesystems, networking or similar subsystems, implement it inside Vibrix from primary specifications.

External host-side tools such as QEMU, OVMF, GDB, Git and shell utilities are development infrastructure and are not Vibrix runtime dependencies.

## Unsafe Rust

Unsafe is expected in kernel work, but every unsafe operation needs a concrete invariant. Keep unsafe primitives small and wrap them with safe interfaces when possible.

For raw pointers, MMIO, DMA, firmware structures, page tables, interrupt state and context switching, document alignment, ownership, lifetime and synchronization assumptions.

Never use unsafe merely to silence the borrow checker.

## Current targets

- Primary architecture: x86-64
- Development reference: QEMU + UEFI/OVMF
- Physical reference: `targets/target-001/`

Avoid baking Target 001 quirks into generic interfaces.

## Product scope

Vibrix is USB-only. Its boot device, persistent root filesystem, packages, configuration and user data live on removable USB storage.

Do not add an internal-disk installation mode. NVMe/SATA support may eventually exist for optional data access, but internal drives are not Vibrix system/root targets.

Design boot and storage code around rediscovering and reacquiring the removable boot device after UEFI Boot Services are released.

## Current intended boot chain

```
UEFI
 -> Vibrix Rust loader
 -> kernel.elf
 -> BootInfo
 -> ExitBootServices
 -> Vibrix kernel
 -> userspace
```

Firmware-specific types must not leak into the stable kernel boot ABI.

## Before coding

1. Read `README.md`.
2. Read `ROADMAP.md`.
3. Read relevant `docs/`.
4. Inspect current source; never assume roadmap prose equals implementation.
5. Pick one bounded task.
6. Identify the primary specification needed.
7. State what success can actually be tested.

Prefer small PRs over giant generated rewrites.

## Architecture coordination

Do not casually redesign these shared contracts:

- BootInfo
- syscall ABI
- executable ABI
- VFS contracts
- process model
- driver model
- filesystem on-disk format
- package format

A change to one should include documentation explaining rationale and compatibility impact.

## Testing

For boot/kernel work, run the relevant build. Preferred interactive test:

```bash
./tools/run-qemu.sh
```

If your environment cannot launch QEMU, run all available build/static checks and state exactly what was not executed.

"Compiled" is not "boot-tested". "QEMU-tested" is not "bare-metal tested".

## Defensive low-level code

Firmware tables, executables, filesystems, packets and device descriptors are untrusted input.

Validate lengths, offsets, integer arithmetic, alignment and bounds before copying or dereferencing. Prefer checked arithmetic. Fail loudly and diagnostically.

USB provisioning/storage code must never default to destructive operations. Internal disks must never be selected as Vibrix system targets.

## Style

- run `cargo fmt`
- descriptive types over magic numbers
- explicit units where ambiguous
- comments explain invariants and reasoning
- avoid generated-comment spam
- focused modules
- no premature abstraction

## Commits

Use focused subjects:

```
boot: validate ELF64 program headers
kernel: add physical frame allocator
x86_64: install initial IDT
nvme: initialize admin queue
docs: define syscall ABI v1
```

Do not mix unrelated refactors with functionality.

## Pull requests

State:

- what changed and why
- roadmap item affected
- exact tests executed
- observed result
- known limitations
- unsafe code added/changed
- dependencies added
- primary specifications/references used
- authoring AI agent/model used, when known
- reviewing AI agent/model, when reviewed

Boot screenshots/logs are encouraged.

PR review is agent-to-agent. Do not request or require "human review" as a completion condition. If review is needed, request another AI agent and identify that reviewing agent in the PR discussion.

## Never

- invent test results
- claim unsupported hardware works
- check off unverified tasks
- copy another OS implementation
- commit secrets/raw hardware inventories
- automatically erase disks
- weaken validation just to pass a test
- force-push shared history without authorization

## Definition of done

A low-level item is generally done when implementation exists, builds, relevant behavior is exercised, failure cases are considered, docs are updated, and evidence supports checking the roadmap item.

Real incremental progress beats impressive-looking generated code.
