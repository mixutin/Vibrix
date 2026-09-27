# PCI segment-zero discovery (native, read-only)

The kernel's `kernel/src/arch/x86_64/pci.rs` now scans the *actual*
x86 configuration mechanism #1 (ports CF8/CFC) **after ExitBootServices**.
It visits all 256 bus numbers, 32 device slots per bus and functions 1–7
only when function zero advertises multifunction support. The scan
recognizes absent vendor IDs, decodes vendor/device/class/subclass/
programming-interface/revision/header-type metadata, and counts USB/xHCI
controllers. The first eight functions are logged to native COM1; the
segment-zero summary is independently logged there. The QEMU debugcon
smoke gate requires the **kernel-originated** `VIBRIX: kernel PCI segment0
enumerated` marker.

The same scan reads assigned BAR registers only for supported endpoint
(type 0, six slots) and PCI-to-PCI bridge (type 1, two slots) headers.
The pure decoder distinguishes I/O, memory32, memory64 pairs and
prefetchability, skips zero/unassigned bases and rejects reserved memory
types or a truncated final 64-bit BAR. Host fixtures cover these cases,
multifunction probing and valid configuration-address encoding.
The QEMU gate additionally requires `VIBRIX: kernel PCI BARs parsed` after
the real scan finds assigned BARs without malformed entries.

## QEMU evidence

[Exact-head Actions run 36342144203](https://github.com/mixutin/Vibrix/actions/runs/36342144203)
passed format, host PCI/ACPI/IDT/frame tests, kernel/UEFI Clippy and builds,
and normal, panic, breakpoint and page-fault QEMU probes. The native
**post-firmware kernel** printed `Vibrix PCI segment0: 6 devices, 9
assigned BARs, 0 xHCI` on the default q35 machine and emitted both
independent PCI debug markers. A **separate QEMU smoke run** set
`VIBRIX_QEMU_XHCI=1` to attach `-device qemu-xhci`, proving the exact
same native kernel scanner recognized `7 devices, 10 assigned BARs, 1
xHCI` over COM1. The default virtual machine does not include an xHCI
controller; neither machine configuration connects a proven persistent USB
root nor initializes the controller or a driver. PCI visibility alone cannot
satisfy M7 native USB milestones.

## Invariants and non-goals

- **No device-config writes:** the CPU writes only the 32-bit address
  selector to I/O port CF8 and reads the chosen DWORD from CFC. There are
  no writes to the PCI command register or a device's BAR.
- **No BAR sizing writes** (the conventional write-all-ones probe is NOT
  used); parsing an assigned base does not establish resource length,
  ownership, memory cache type, DMA permissions or that MMIO is mapped.
- These ports offer the **256-byte legacy configuration header in PCI
  segment zero only**. Other host-bridge segments and extended config space
  require ACPI MCFG/ECAM and firmware table mappings not yet implemented.
  On a machine without mechanism #1 this path may discover no devices.
- Config address/data accesses must be serialized; the early kernel runs
  one boot CPU in ring 0 with interrupts disabled. This must be replaced
  with synchronization before IRQ/AP access is enabled.
- The scan does not probe or activate an xHCI controller, assign PCI
  resources, use MSI, configure bus mastering, bind drivers, select storage
  or identify the persistent boot USB. Even when PCI shows a USB
  controller, that controller is NOT an operational native USB driver.
- QEMU's current FAT test disk is virtual development boot media and is
  **not** a verified USB-attached persistent root. Do not claim Target 001
  discovery from QEMU evidence.

This provides an independently tested native x86 PCI discovery path for
the first machine segment and decodes real assigned BAR values. It does
not implement the separate M4 ACPI SDT traversal, MCFG/ECAM, MSI-X or
driver binding milestones.

Primary reference: PCI Local Bus Specification's Configuration Mechanism
#1, PCI-to-PCI Bridge Architecture Specification (type-1 header) and PCIe
Base Specification (BAR encoding and functions). This uses original
first-party Rust and the x86 I/O instructions, no external OS code or new
Cargo dependency.
