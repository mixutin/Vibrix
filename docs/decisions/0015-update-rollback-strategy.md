# ADR 0015: Immutable generations and bounded trial-boot rollback

- **Status:** Accepted strategy; updater, verifier and persistent boot selection are not implemented.
- **Date:** 2026-09-28
- **Authoring AI:** GPT-6 Astra Pro
- **Roadmap:** M9 — System update + rollback strategy
- **Compatibility:** No current BootInfo, ESP, GPT, package or VibrixFS wire-format change.

## Goals and non-goals

An interrupted update must not overwrite the only known-good Vibrix system.
The same removable USB must retain a recovery route; internal disks must never
be selected as alternate installation targets. A valid signature alone does
not establish successful installation, storage durability or a healthy boot.

This decision completes the strategy and executable state-machine model. It
is not a claim of working secure updates, secure boot, persistence or recovery.

## Generation model

Treat a release as an immutable kernel/userspace/package generation. Stage a
candidate beside the known-good generation on the already-identified Vibrix
USB. Preserve mutable `/home` separately. Versioned configuration/package
metadata migrations must remain compatible with the fallback generation or
use a separately recoverable snapshot; refuse unsupported destructive migrations.

Before marking a generation bootable, verify its signed manifest and all
payload hashes, target/ABI/format compatibility, free-space budget and exact
USB disk/ESP/root identity. Signature verification and key rotation require
reviewed implementations and pinned trust roots; model booleans are not proof.
Revalidate the identity after reconnect and before every resumed update phase.
Unknown/duplicate media, unsupported flush, ambiguous state, I/O error or an
incompatible schema abort without selecting another disk.

Write and flush immutable payloads before publishing selection metadata. The
future metadata contract needs version, sequence, known-good generation,
pending generation, bounded attempts, security epoch, payload/manifest identity
and integrity checks. Use independently validated redundant records and ordered
commit barriers: preserve the previous valid record until the new one is durable.
Conflicting equally current records or malformed state fail to read-only
recovery rather than selecting by location or enumeration order. CRCs detect
corruption, not attackers; generation authentication remains mandatory.

## Trial boot and health commitment

Allow at most **three** unconfirmed trial boots. Decrement and durably record
the attempt before executing the candidate. Reset/power loss without a trusted
health acknowledgement consumes that attempt. After exhaustion, choose the
unchanged known-good generation automatically. A newer update cannot replace
or reset the budget of an already pending trial.

A health acknowledgement is specific to the most recent generation/attempt;
stale acknowledgements must not promote a candidate. Require the expected root
to mount, essential userspace to become ready, and real storage flush to succeed.
Only then durably promote the generation. Keep old payloads through commitment;
reclamation is a separate, bounded operation and must never erase the only
recoverable generation. Explicitly re-staging a failed release requires a new
user/update-manager decision, not an automatic retry loop.

Release sequence and security epoch are distinct. Ordinary staging requires a
newer sequence; automatic fallback selects only the previously known-good one.
Both must satisfy the independently trusted minimum security epoch. Do not
advance a revocation floor merely because a trial boot passed: that could make
the only fallback unusable. Authenticated revocation/floor changes require a
valid recovery generation at or above the new floor, otherwise stop in recovery.
Without trusted monotonic state or a remote trust service, a movable USB cannot
promise protection from an attacker restoring an entire old signed disk image.

## Boot-chain and filesystem integration gates

The current fixed `BOOTX64.EFI` / `KERNEL.ELF` loader does not select generations.
A future **versioned** ESP/loader contract must bind the selected kernel, root
generation and signed manifest without mixing releases. Keep bootstrap-loader
updates outside the first updater until an independently bootable recovery path
can replace/restore them safely. Do not overwrite the active bootstrap or
silently reinterpret the current ESP layout.

The native USB/block layer must provide real ordered flush semantics, and
VibrixFS must implement the durability contract in
[ADR 0011](0011-vibrixfs-crash-consistency.md) before writable updates are enabled.
[ADR 0005](0005-boot-usb-identity.md) remains a separate proposed identity/ABI
implementation gate. No loader firmware pointer survives into the updater.
An offline bundle follows the same verification path as a network download.
Stable/beta/nightly channels never bypass signature or compatibility checks.

Recovery must boot from the same USB without the failed generation. It should
inspect records and logs, verify available generations, restore an eligible
known-good selection, and export diagnostics with explicit consent. Missing
native USB, corrupted filesystem or unavailable trusted recovery must fail
clearly; this strategy does not promise recovery from physical media failure.

## Executable model and evidence

`kernel/src/update_policy.rs` implements staging prerequisites, generation/floor
checks, a three-attempt trial budget, exact-attempt confirmation, failed-health
rejection and automatic known-good selection. Rejected staging/confirmation
leaves state unchanged. The state is pure RAM; it has no serialization, signing,
filesystem I/O, network I/O, actual reboot or privileged authorization mechanism.

Host tests enumerate all 32 combinations of prerequisite assertions and cover
rollback, healthy promotion, stale tickets, pending-trial replacement, downgrade
and security-floor rejection. The QEMU-debug kernel executes failed-trial and
successful-promotion simulations using the same model before reporting
`VIBRIX: kernel update policy model verified` through COM1 and debugcon.
These markers prove **model execution inside the kernel**, not a disk update.

Future runtime acceptance requires power cuts at every payload/flush/record
boundary, failed boot and health acknowledgement, full-media and disconnect
faults, corrupt/mixed-generation payloads, revoked keys/epochs and successful
fallback across real USB reboots. No runtime update/security/recovery checkbox
may be inferred from this strategy item. No dependencies, new unsafe code or
external implementation source are added.
