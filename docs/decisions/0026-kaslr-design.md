# ADR 0026 — Kernel address randomization design and threat model

- **Status:** Accepted design
- **Date:** 2026-09-29
- **Roadmap:** M23 — kernel address randomization design and threat model

## Context

Vibrix currently loads its x86-64 kernel at a deterministic higher-half virtual
layout. W^X and userspace isolation reduce exploitability, but a stable kernel
layout gives an attacker with a memory-corruption primitive predictable code
and object addresses.

Kernel address randomization is a **probabilistic hardening layer**, not an
isolation boundary. Information disclosures can defeat it, and it must not be
used to justify unsafe memory handling or weaker permissions.

## Threat model

The design aims to raise the cost of attacks where an unprivileged process or
hostile input path gains a kernel memory-corruption primitive but does not
already know the current boot's kernel layout.

In scope:

- reuse-oriented attacks requiring stable kernel text/gadget addresses;
- attacks relying on deterministic early dynamic-region locations;
- cross-boot reuse of addresses learned on another machine or previous boot.

Out of scope:

- attackers with arbitrary kernel read capability or an address disclosure;
- privileged debug interfaces that intentionally reveal addresses;
- physical attackers able to replace the boot image;
- DMA attackers before IOMMU isolation exists;
- microarchitectural side channels that independently recover addresses;
- compromised firmware or bootloader entropy.

## Decision

A future x86-64 implementation will randomize the kernel's **virtual base at
boot** while preserving explicit non-overlap with fixed architecture regions,
MMIO windows, the userspace half, guarded stacks and temporary bootstrap
mappings.

The loader must choose the slide before final kernel page tables are activated.
The slide will be:

- aligned to the largest mapping granularity required by the final kernel image;
- selected from a compile-time-defined canonical higher-half window;
- bounded so every PT_LOAD segment and required bootstrap mapping fits;
- derived from the production secure-random service or an authenticated
  bootloader-provided entropy handoff with equivalent strength;
- rejected rather than replaced with a predictable pseudo-random fallback when
  required entropy is unavailable.

The kernel ELF must become relocatable under a narrowly documented relocation
set. Unsupported relocation records fail boot. Absolute addresses must not be
silently patched by ad-hoc scanning.

## Entropy and disclosure rules

The random slide itself is secret for the lifetime of the boot. Normal user
interfaces, crash reports, compatibility bundles and logs must not expose raw
kernel pointers. Developer/debug builds may opt into symbol/address output but
must be clearly separated from the hardened profile.

Per-boot entropy is independent; a previous boot's layout must not determine the
next one. Entropy accounting will be documented before implementation rather
than inferred from the width of the address window.

## Failure and recovery behavior

KASLR failure must never select an overlapping or non-canonical mapping. If the
image cannot be relocated safely, boot fails with a bounded diagnostic.

A development-only deterministic mode may exist for debugging, but production
release profiles will not silently disable randomization once implementation is
declared complete.

## Validation plan

Implementation evidence will require at least:

1. repeated QEMU boots showing multiple valid slides;
2. deterministic test vectors for relocation processing and overflow rejection;
3. proof that kernel text/data permissions remain W^X-correct after relocation;
4. fault/interrupt/userspace regressions under randomized layouts;
5. checks that normal diagnostics do not print raw randomized kernel addresses;
6. explicit entropy-unavailable failure-path coverage.

This ADR completes only the **design and threat-model** roadmap deliverable. It
does not implement KASLR, userspace ASLR, PIE userspace, module randomization,
IOMMU isolation, or protection against address disclosures.

## References

Design references checked 2026-09-29:

- Linux kernel self-protection documentation, KASLR discussion:
  https://docs.kernel.org/security/self-protection.html
- Linux kernel threat-model documentation:
  https://www.kernel.org/doc/html/latest/process/threat-model.html

These references inform security goals and limitations only; Vibrix does not
copy implementation code.
