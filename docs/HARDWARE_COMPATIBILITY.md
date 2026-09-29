# Hardware compatibility and quirk reporting

This M18 slice turns the existing immutable PCI identity/driver-candidate model
into an explicit compatibility report. It deliberately does **not** equate a
registry match or early binding with working hardware.

For every discovered PCI identity, the kernel classifies the device as either:

- a current Vibrix driver candidate, or
- discovered with no matching first-party driver.

The same observation may carry a known implementation limitation. The first
recorded limitation is the current xHCI path's **32-byte context-only** boundary.
That is reported as a compatibility limitation because controllers requiring
64-byte contexts are outside the implemented xHCI contract. RTL8168 matching
currently has no additional quirk entry in this table.

The native post-firmware boot emits a bounded aggregate:

`Vibrix hardware compatibility: discovered=... driver_candidates=... missing_driver=... known_limitations=...`

and a separate debugcon readiness marker. Production-linked host tests prove
that xHCI candidates carry the limitation, RTL8168 candidates do not invent
one, unknown devices remain visible as missing-driver entries, and the aggregate
does not lose devices.

## Boundary

This is reporting over the currently discovered PCI identities. It does not
claim that a candidate has initialized BARs, DMA, interrupts, USB children,
network traffic, hotplug, physical Target 001 support, or broad hardware
compatibility. A zero known-limitations count means only that no entry in the
current explicit limitation table matched.

The xHCI limitation follows the implementation boundary already documented in
`docs/USB_HID.md`. Architecture reference: Intel xHCI 1.2c, checked
2026-09-29: https://cdrdv2-public.intel.com/868295/xHCI__Rev1.2c.pdf
