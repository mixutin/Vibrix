# Driver binding and missing-driver diagnostics

This M18 diagnostic layer reports what the existing early device model can
prove after native post-firmware PCI discovery. It does not activate hardware.

The summary separates five facts:

- `discovered`: PCI functions observed by the native segment-zero scan.
- `candidates`: devices matched by the first-party driver registry.
- `bound`: candidates that obtained one exclusive early binding.
- `missing_driver`: discovered devices with no registry match.
- `binding_failures`: matched candidates that could not be bound.

A device counted as `missing_driver` is not necessarily unsupported forever;
it only means the current early registry has no matching driver descriptor.
A successful binding means ownership bookkeeping succeeded. It does **not** mean
BARs were activated, DMA was enabled, interrupts were configured, or the device
performed useful I/O.

The production boot prints one exact `Vibrix driver diagnostics:` line and
emits `VIBRIX: kernel driver diagnostics ready` on debugcon after the summary
is constructed. Host tests distinguish unknown devices from binding failures,
and the dedicated evidence workflow requires the production post-
`ExitBootServices` QEMU path to emit both outputs.

This diagnostic is intentionally bounded to the current early PCI/device model.
USB child devices, runtime hotplug, native RTL8168 activation, persistent device
history, physical Target 001 hardware and user-facing hardware databases remain
separate work.
