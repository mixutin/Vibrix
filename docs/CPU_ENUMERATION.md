# Firmware CPU enumeration

Author: **GPT-6 Astra Pro**.

The M12 CPU enumeration API in `kernel/src/cpu_topology.rs` consumes the full
mapped MADT as bytes, validates the APIC signature, reported table length,
checksum and record framing, and produces a fixed-capacity inventory of at most
64 logical processors. It supports processor Local APIC (type 0) and processor
Local x2APIC (type 9) records. Firmware UID and full-width APIC ID are kept
separate. Enabled, disabled and online-capable states are not conflated.

Primary specification: ACPI 6.5 sections 5.2.12.2 and 5.2.12.12:
https://uefi.org/specs/ACPI/6.5/05_ACPI_Software_Programming_Model.html

Reserved fields/flags, broadcast APIC identifiers, duplicate APIC IDs or UIDs,
malformed known-record lengths, an empty enabled set and capacity overflow fail
closed. Unknown well-framed records are skipped. No partial inventory escapes
on failure. The strict duplicate policy includes disabled entries; firmware
with duplicate placeholder identities is not silently accepted.

## Runtime and evidence

The existing post-ExitBootServices ACPI reader calls this parser while the
checksummed real MADT is mapped read-only/NX. It logs each enabled processor's
UID and APIC ID and an exact count, followed by the kernel CPU-enumeration
marker on COM1 and debugcon. No firmware pointer/reference escapes the existing
mapping callback. The original APIC/timer discovery continues unchanged.

Host tests cover mixed record types, availability, truncation at every byte,
checksum corruption, reserved/invalid fields, duplicate identities and capacity.
The dedicated QEMU test starts 1, 4 and 16 virtual CPUs and requires the kernel
inventory to report exactly the configured count and unique enabled IDs.

This completes **firmware inventory only**, not AP startup, topology hierarchy
(cores/threads/packages), CPU hotplug, per-CPU stacks, SMP synchronization,
scheduling, TLB shootdown or Target 001 8C/16T validation. Only the BSP executes
Vibrix code. No new unsafe primitive, dependency or copied implementation source
is introduced.
