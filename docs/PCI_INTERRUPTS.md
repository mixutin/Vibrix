# PCI message-signaled interrupts

Author: GPT-6 Astra Pro. No independent review is claimed.

This is the M4 MSI/MSI-X implementation lane. The M3 managed-VM stack,
driver binding, block/NIC abstractions, USB storage and CI-policy overhaul
are separate work. Main at the start of this lane was
`cfc8bd1a6e7cfce8492eecaef7ba746ac8841aaa`.

## First increment: conventional capability discovery

`shared/pci_interrupts/` is dependency-free, allocation-free production code.
`host-tests/pci-interrupts.rs` imports that exact code for host tests and a
bare-metal `no_std` build. Discovery consumes only aligned DWORD reads from
one serialized PCI configuration function. It checks device presence and
header type, honors the status capability-list bit, and validates every
pointer before reading it. A 64-bit occupied-slot bitmap bounds traversal
and rejects cycles or overlaps with known MSI/MSI-X payloads.

MSI 32/64-bit address layouts, optional mask/pending registers, and message
capacity are decoded. MSI-X discovery retains the vector count and separate
BAR indicators/offsets for its table and pending-bit array. Header types 0
and 1 are supported; CardBus and extended capabilities are not. Unknown
capabilities are skipped using their headers; their unknown payload sizes
cannot be checked. A failed or malformed chain returns no partial inventory.

These descriptors are not permissions to access memory. Assigned BAR base
addresses alone do not establish extent, memory type, ownership or a valid
mapping. Interrupt-vector ownership, handler installation, LAPIC readiness,
masking and device-specific acknowledgement remain required before enabling
interrupts. No device is modified by discovery.

## Evidence and completion boundary

The additive PCI interrupt workflow checks production formatting, host
negative cases and compilation for `x86_64-unknown-none`. The existing full
Vibrix CI remains unchanged. Results must be observed on the exact PR head;
this document does not assert a run succeeded.

This increment alone does not link discovery into kernel startup, program
MSI/MSI-X, demonstrate an interrupt, or complete the roadmap checkbox.
Later increments must supply native post-firmware evidence, not generated
logs or a mock device standing in for QEMU.

## Primary interface references

- PCI-SIG, PCI Local Bus capability structures and the
  [MSI-X ECN](https://pcisig.com/PCIConventional/ECN/Base).
- [QEMU EDU device specification](https://www.qemu.org/docs/master/specs/edu.html),
  for an isolated future MSI delivery test without storage or DMA work.
- [QEMU ivshmem specification](https://www.qemu.org/docs/master/specs/ivshmem-spec.html),
  for a future eventfd-driven MSI-X delivery test.

No external operating-system or driver implementation was copied.
