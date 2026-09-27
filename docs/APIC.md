# Early x86-64 APIC discovery

Vibrix discovers the x86-64 interrupt-controller topology from the real ACPI
MADT after `ExitBootServices`. This checkpoint is deliberately narrower than
interrupt routing: it establishes that the kernel can validate the firmware
topology, safely reach the controller register banks, and read their
architectural identity/version registers without firmware services.

## MADT contract

The production ACPI parser validates the checksummed `APIC` SDT before
interpreting its payload. It accepts the fixed MADT header, requires only the
defined PCAT-compatibility flag, validates known entry lengths, rejects
duplicate Local APIC address overrides, and requires at least one aligned
I/O APIC address. Local APIC processor flags and reserved bytes are checked
before entries are exposed to the runtime path.

The runtime reader takes the first validated I/O APIC for this early
single-controller checkpoint and carries the MADT Local APIC physical base
forward as a number. This does not yet select interrupt source overrides,
enumerate all interrupt domains, or bind devices to vectors.

## MMIO and firmware-map boundary

PCI ECAM and APIC MMIO intentionally use different ownership rules.

For ECAM, the MCFG allocation identifies an aperture and the kernel also
requires the full page to appear in the final UEFI memory map as
UC-capable reserved/MMIO memory.

QEMU OVMF does **not** list the architectural Local APIC `0xfee00000` or
I/O APIC `0xfec00000` pages in `GetMemoryMap`. For APIC only, device
identity is therefore established independently:

- the Local APIC page must match both the checksummed MADT address and the
  enabled `IA32_APIC_BASE` MSR while x2APIC mode is off;
- the I/O APIC page must come from a validated MADT type-1 entry;
- either page is rejected if a present UEFI descriptor conflicts by
  describing RAM, loader/ACPI memory, runtime memory, or cacheable memory;
- an absent UEFI descriptor is permitted only after those APIC-specific
  identity checks.

This negative firmware-map check must not be generalized to arbitrary MMIO.

Before creating any mapping, the kernel verifies that x86 PAT index 3
(`PCD=1, PWT=1`) is UC. The BootInfo v3 temporary window then maps one
controller page at a time supervisor-only and NX. The Local APIC page is
read-only. The I/O APIC mapping is writable solely because register reads
require writing the `IOREGSEL` selector; the implementation writes only
selectors 0 (ID) and 1 (version) and reads `IOWIN`. No redirection-table
entry is modified.

## Current validation boundary

A QEMU validation run before the final synchronization observed a Local APIC
at `0xfee00000`, one I/O APIC at `0xfec00000`, LAPIC ID 0/version
`0x14`/maximum LVT 5 and I/O APIC ID 0/version `0x20`/maximum
redirection entry 23 in all seven kernel boot configurations. Final exact-head
CI after synchronization is required before integration.

This work does **not** enable the Local APIC software-enable bit, mask the
legacy PIC, program I/O APIC redirection entries, install IRQ gates, send EOI,
execute `sti`, route a timer/keyboard interrupt, or support SMP. Therefore
the M3 **Local APIC + I/O APIC** and **Timer + interrupt routing** checkboxes,
and the M4.5 **Hardware interrupt path usable in QEMU** checkbox, remain open.

Primary references are ACPI 6.5 MADT definitions and Intel 64 and IA-32
Software Developer's Manual volume 3 APIC architecture. No third-party OS
implementation source or new dependency is used.
