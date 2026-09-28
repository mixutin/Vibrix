# Native PCI interrupt inventory

Author: GPT-6 Astra Pro.

The normal post-ExitBootServices segment-zero scan now invokes the same
bounded capability parser exercised by the host tests. COM1 reports MSI
address width, mask support and message capacity, plus MSI-X vector count
and separate table/PBA BAR offsets. Malformed chains are reported and do not
become partial usable capabilities. This path does not program interrupts,
change command/BAR registers, enable bus mastering, size BARs or access MMIO.

`tools/test-pci-interrupts.sh` builds the production UEFI loader/kernel and
boots QEMU q35/TCG/OVMF with an EDU PCI device. It requires actual native
post-firmware entry, the EDU's discovered MSI capability, completed inventory
and kernel startup. Guest debugcon, COM1 and QEMU stderr are printed into the
Actions job log, including on failures. Only an ephemeral host-directory ESP
and a per-run firmware-variable copy are used; no physical disk is touched.

The additive PCI interrupt workflow also checks that both programming test
modules remain in the test executable, preventing a harness edit from
silently dropping MSI or MSI-X test coverage. Existing Vibrix CI is retained.

Run on a Linux host with Rust, QEMU and OVMF:

```sh
bash tools/test-pci-interrupts.sh
```

This proves discovery in the actual kernel only after its CI run passes.
It does not prove an interrupt was delivered and does not finish the broad
M4 MSI/MSI-X checkbox. The next increment exercises native MSI delivery;
MSI-X delivery remains a separate requirement. No M3 VM, driver-binding,
USB transport, persistent-root, BootInfo or on-disk contract is changed.

Primary device contract: [QEMU EDU specification](https://www.qemu.org/docs/master/specs/edu.html).
