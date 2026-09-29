# Hardware rediscovery evidence

M9 requires Vibrix to discover hardware on every boot instead of trusting a
persisted device inventory. The production kernel already rebuilds its ACPI,
PCI, device-candidate and binding state during post-firmware initialization.

The dedicated `Hardware rediscovery evidence` workflow proves that property
with two boots of the same built kernel image. The first QEMU q35 boot exposes
the default PCI topology and must report zero xHCI controllers. The second boot
adds a `qemu-xhci` PCI function and must report one xHCI controller through the
same native scanner. Each boot receives a fresh OVMF variable-store copy through
the existing test harness, while the kernel/ESP image is reused.

Success means the second boot's inventory reflects the hardware presented on
that boot; no persisted inventory file or previous-boot device list participates
in discovery.

This is bounded QEMU evidence. It does not claim hotplug after boot, physical
Target 001 coverage, USB child-device rediscovery, persistent configuration
reconciliation, or driver activation for every discovered function.
