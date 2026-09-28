# Native MSI delivery evidence

Author: GPT-6 Astra Pro.

The `pci-irq-probe` feature connects the production MSI programming sequence
to actual x86 WORD/DWORD configuration transactions, a permanent IDT gate
at vector 0x50, and QEMU EDU's IRQ status/raise/acknowledge registers. It is
an opt-in one-shot diagnostic, not automatic binding of arbitrary PCI devices.
Default kernel builds continue to inventory PCI interrupts read-only.

## Ownership and ordering

The explicit QEMU q35/TCG/one-BSP environment has no IOMMU or passed-through
devices. Setup rejects another hypervisor, missing/duplicate EDU functions,
unsupported BAR layout, active interrupt ownership and non-quiescent sources.
It validates the documented 1 MiB BAR0 geometry and checks its first page
against retained firmware RAM/runtime exclusions. The EDU register signature
is verified before granting bus-master permission for MSI memory-write messages.

The existing APIC activation path lends its live mapping-window owner to the
probe. APIC slots 0/1 and EDU slot 2 remain mapped; no second Window owner is
created after activation and no direct PTE edits bypass ownership. The IDT gate
exists before setup enables MSI. Handler state is static; the ISR uses only
volatile register accesses, atomics and LAPIC EOI, without allocation/printing.
Configuration accesses refuse IF=1 and the handler never touches CF8/CFC.

MSI messages need PCI memory-write permission. This diagnostic temporarily
sets the isolated EDU function's command bus-master bit, then clears it after
verified MSI disable. It never programs the EDU DMA source, destination,
length or start registers, and it never accesses the device's DMA-buffer BAR
region. This is not a claim of general DMA isolation or IOMMU support.

## Evidence required by CI

The guest raises two real device interrupts through EDU MMIO, checks source
status, acknowledges each in the ISR and sends LAPIC EOI. It then disables
MSI with verified readback, raises another pending event while INTx is also
disabled, and verifies no third delivery over three real PIT ticks. It clears
the pending status and bus-master bit. All waits are bounded; the external
QEMU timeout also treats a missing marker as failure.

Only after those checks does the guest print:

```text
kernel MSI: delivered=2 acknowledged=2 disabled=true bus_master=false
VIBRIX: native MSI repeated delivery verified
```

The host additionally requires continued boot to the actual console prompt.
The default-mode matrix job ensures ordinary discovery does not arm the probe.
Debugcon, COM1 and QEMU logs are retained in Actions output. Logs and markers
are produced by the booted kernel, not manufactured by the host test.

Run on a Linux QEMU/OVMF host:

```sh
VIBRIX_KERNEL_FEATURES=qemu-debugcon,pci-irq-probe \
VIBRIX_EXPECT_MSI_IRQ=1 bash tools/test-pci-interrupts.sh
```

Exact-head Actions results establish whether this evidence passed. Local
Rust/QEMU execution was unavailable to this contribution session. Do not run
this diagnostic feature on physical machines or general-purpose VM guests.

## Remaining M4 boundary

MSI-X table geometry/programming has production-linked host and bare-metal
compile tests, but this EDU device supplies MSI, not MSI-X. Native MSI-X
interrupt delivery, general driver/vector allocation, SMP synchronization,
interrupt remapping and device teardown are not claimed. The broad roadmap
MSI/MSI-X checkbox must remain open until those required native MSI-X delivery
checks are implemented and pass. No M3 VM, USB/storage, BootInfo or on-disk
format implementation is duplicated or changed.

Primary reference: [QEMU EDU device contract](https://www.qemu.org/docs/master/specs/edu.html).
