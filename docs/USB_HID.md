# Bounded native USB HID input

The keyboard and mouse probes use the existing PCI-discovered xHCI owner after
ExitBootServices. One directly attached full/low-speed boot-subclass device is
supported per boot. The ordinary desktop and shell still use PS/2 input.

The driver reads a bounded configuration, selects one alternate-zero boot
interface, configures a native interrupt-IN endpoint, selects Boot Protocol and
disables idle reports. Each report has a dedicated matching transfer event.
Keyboard decoding handles modifiers, held-key reordering, release, duplicate
rejection and rollover errors. Mouse decoding preserves three buttons and signed
relative motion. No arbitrary HID report-descriptor interpreter is included.

The transport polls xHCI event memory with interrupts disabled; an interrupt USB
endpoint does not imply a CPU interrupt handler. A fixed 32-report ring budget
and one outstanding transfer avoid wrap/reuse races. Success disables the slot
before unmapping; all DMA backing stays reserved. Errors fail boot without
freeing controller-owned memory. General device lifetime/recovery, hubs for HID,
hotplug, international layouts, key repeat, LEDs and desktop integration remain
separate work.

## Research and alternatives

Checked 2026-09-29:

- [USB-IF HID 1.11](https://www.usb.org/sites/default/files/hid1_11.pdf),
  sections 7.2.4/7.2.6 and appendices B/C: explicit Boot Protocol, idle policy,
  keyboard state reports and signed boot mouse movement.
- [Intel xHCI 1.2c](https://cdrdv2-public.intel.com/868295/xHCI__Rev1.2c.pdf),
  sections 4.6.6, 4.11, 6.2.3 and table 6-12: Configure Endpoint, transfer
  completion identity/residue, endpoint context and interval conversion.
- [QEMU USB manual](https://www.qemu.org/docs/master/system/devices/usb.html):
  controller/device topology and emulated keyboard/mouse support.
- [xhci 0.9.2](https://docs.rs/xhci/0.9.2/xhci/) provides no_std register and
  context types; [usbh](https://docs.rs/usbh/latest/usbh/) describes an
  experimental controller/class abstraction. Neither replaces Vibrix's DMA and
  mapping ownership. This bounded extension reuses the existing owner instead
  of migrating its register model. No dependency added or upstream implementation
  copied. These alternatives can be reconsidered for a general host stack.

## Validation and completion

`cargo test --locked -p vibrix-kernel --lib usb_hid::` tests production descriptor
and report decoders. Canonical CI builds both native probes, waits for a guest
readiness marker, and injects `h`, Shift-B, button changes and movement through
QEMU HMP. Success requires actual interrupt transfers including key release and
mouse release, matching event identity, and successful Disable Slot. No host
script manufactures guest output. Existing direct enumeration, hub, shell,
desktop, storage and fault regressions remain required.

Roadmap completion requires a merged implementation and its observed exact-head
CI/QEMU run. Physical Target 001 remains untested.
