# Per-CPU structure foundation

Author: **GPT-5.6 Sol**.

This M12 slice turns the already-validated ACPI processor inventory into
kernel-owned per-CPU identity slots. It is intentionally smaller than AP
startup or an SMP scheduler.

## Ownership model

The production MADT parser remains the only source of firmware CPU identities.
After the complete ACPI root-table scan and ECAM validation succeed, the BSP
copies every validated processor record into one fixed-capacity 64-slot table.
The table preserves:

- firmware UID;
- APIC ID;
- enabled / online-capable / disabled availability;
- xAPIC versus x2APIC record origin.

The copy is published exactly once with a Release store. After publication the
table is immutable. Safe readers perform an Acquire load before borrowing it.

The executing BSP is not guessed from firmware ordering. After the native LAPIC
MMIO probe reads the current local APIC ID, Vibrix atomically binds that ID to
an **enabled** published slot. Disabled and merely online-capable firmware
entries cannot become the BSP.

## Current boundary

This is a pre-SMP structure foundation:

- only the BSP executes Vibrix code;
- no AP is started;
- no per-CPU mutable scheduler/interrupt state exists yet;
- no GS-base CPU-local addressing is installed;
- no cross-CPU synchronization or TLB shootdown is claimed;
- CPU hotplug is not supported.

The immutable table is deliberately useful before synchronization exists.
Future AP startup can bind an AP to its existing firmware slot, then add
separately synchronized mutable CPU-local state without changing firmware
identity semantics.

## Evidence contract

The existing CPU evidence workflow now:

1. host-tests the production MADT parser and production per-CPU table logic;
2. runs target all-features Clippy;
3. boots QEMU with 1, 4, and 16 virtual CPUs;
4. requires the per-CPU table slot count to equal the validated firmware CPU
   count;
5. requires one exact BSP binding record;
6. requires the BSP APIC ID to appear in the firmware inventory;
7. retains timer IRQ and console regression checks.

A successful run proves the bounded per-CPU identity structure on QEMU. It does
not prove AP startup, SMP execution, Target 001 topology, or 8C/16T behavior.
