# ADR 0014: Portable preferences, ephemeral hardware state

- **Status:** Accepted policy; persistent configuration integration remains pending.
- **Date:** 2026-09-28
- **Authoring AI:** GPT-6 Astra Pro
- **Roadmap:** M9 — Portable configuration policy
- **Compatibility:** No BootInfo, GPT or VibrixFS format change.

## Context

The same Vibrix USB is intended to boot different machines. User preferences
should travel; CPU counts, PCI addresses, USB addresses, driver bindings and
firmware mappings must not. The current kernel rediscovery and the future
boot-media identity design are distinct from persistent user configuration.

## Decision

Keep three explicitly different classes of state. Portable preferences belong
to the installation on removable storage. Hardware observations and runtime
bindings are recreated in RAM on every boot. Credentials are separate protected
state, never ordinary preference fields or unredacted diagnostic output.

The initial executable reference policy is `kernel/src/config_policy.rs`:
versioned, allocation-free and bounded to 1024 ASCII bytes and 32 physical lines.
It accepts whole-line comments, whitespace and single `key=value` records.
Version 1 must appear exactly once. Supported keys are `hostname`, `keymap`
(`us`/`fi`), `network` (`off`/`dhcp` preference), and `log_level`
(`error`/`info`/`debug`). The hostname is one lowercase ASCII alphanumeric/hyphen
label of 1–63 bytes with alphanumeric ends. There is no shell interpolation,
include directive, escape syntax, filesystem path or executable setting.

Defaults are hostname `vibrix`, US keymap, network off, and info logging.
Duplicate keys, unknown versions/keys, oversized input, control bytes, invalid
values and hardware-selection keys reject the **whole** document. The parser
returns a borrowed immutable value only after full validation; a caller must
not partially apply failed input or overwrite the rejected source file.
New preference keys require an explicit policy/schema compatibility review.

Network preference is not network consent: `network_for_boot(false)` always
returns off, including on a second host with the same USB. A transient user or
explicit deployment-policy approval on the current boot is required to use the
saved DHCP preference. This boolean is not a stored authorization token. No
network interface is activated by this module.

Future configuration loading must use validated files on the identified Vibrix
root, not a host internal filesystem. Proposed namespaces are `/etc/vibrix` for
administrator-controlled portable preferences, `/home` for user preferences,
and `/run/vibrix` for disposable runtime facts; these paths are **policy**, not
currently mounted directories. Precedence is compiled defaults, then validated
administrator document, then explicit allowed session overrides. Ordinary user
preferences must not override administrator security policy or system boot
identity. Missing files use safe defaults; invalid files enter a diagnostic
safe-default/read-only path without destroying the original.

Never persist firmware handles, MMIO addresses, DMA ownership, CPU topology,
PCI/USB enumeration numbers or an active driver binding as authoritative state.
A future hardware-specific user override must match freshly rediscovered
capabilities and remain a hint, not a substitute for validation. Avoid stable
machine fingerprints by default. Driver discovery must not write new preferences
every boot, reducing flash writes and machine-history leakage.

Root/media selection remains subject to
[ADR 0005](0005-boot-usb-identity.md), including no internal-disk fallback and
ambiguity rejection. This ADR does not accept or implement that still-proposed
handoff/reacquisition contract. Preferences cannot set `root`, `root_device`,
`pci_address`, `usb_address`, `cpu_count`, or `driver_binding`.

## Validation and boundaries

Production host tests cover schema/version rejection, duplicate keys, unsafe
hardware keys, document bounds, encoding, hostname injection and safe network
consent. The QEMU-debug kernel executes the same parser/policy on a fixed sample
and checks both network-gating branches before emitting independent COM1 and
debugcon markers. This validates an **executable configuration policy**, not
loading settings from USB, changing the active keymap, managing credentials,
network activation, a mounted `/etc` or cross-machine persistence.

Those runtime features require VFS/native USB/userspace and separate tests:
move one USB between machines, retain preferences, rediscover hardware, reject
stale identifiers, and verify no internal drive writes. No dependencies or
unsafe code are added; no external implementation source was used.
