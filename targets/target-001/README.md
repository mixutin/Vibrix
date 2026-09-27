# Target 001 — Gigabyte B550M AORUS ELITE

Vibrix's first physical reference platform.

This profile is derived from a local hardware inventory and intentionally excludes machine-unique identifiers such as serial numbers, UUIDs, MAC addresses, IP addresses and partition identifiers.

## Platform

| Component | Target hardware |
|---|---|
| Architecture | x86-64, little-endian |
| Motherboard | Gigabyte B550M AORUS ELITE |
| Firmware | American Megatrends UEFI, Gigabyte FDd |
| CPU | AMD Ryzen 7 3700X (Zen 2 / Matisse), 8 cores / 16 threads |
| Memory | 32 GiB DDR4-3200 |
| GPU | AMD Radeon RX 6600 family / Navi 23, PCI 1002:73ff |
| Primary storage | Samsung NVMe, controller 144d:a808 |
| SATA | AMD 500 Series / AHCI, PCI 1022:43eb |
| Ethernet | Realtek RTL8111/8168 family, PCI 10ec:8168 |
| USB | AMD 500 Series xHCI 1022:43ee + AMD Matisse xHCI 1022:149c |
| Onboard audio | AMD HD Audio 1022:1487 / Realtek ALC1220 |
| GPU audio | AMD Navi HDMI/DP Audio 1002:ab28 |
| TPM | AMD TPM 2.0 |
| IOMMU | AMD-Vi capable |

## Firmware interfaces observed

UEFI boot is confirmed. ACPI exposes, among others:

- APIC
- FACP
- HPET
- MCFG
- IVRS
- TPM2
- DSDT/SSDT tables

## CPU capabilities relevant to Vibrix

The processor exposes long mode, NX, SYSCALL, APIC/x2APIC, SSE/SSE2, XSAVE, AVX/AVX2, FSGSBASE, SMEP, SMAP, RDRAND/RDSEED, 1 GiB pages and AMD-V.

Reported address widths are 43-bit physical and 48-bit virtual.

## Initial bare-metal enablement order

1. UEFI loader + GOP framebuffer
2. x86-64 kernel entry and memory map handoff
3. ACPI discovery
4. APIC/x2APIC and timers
5. PCI/PCIe enumeration via ACPI MCFG/ECAM
6. xHCI + USB HID for keyboard/mouse
7. NVMe block I/O
8. persistent Vibrix filesystem / portable USB storage
9. Realtek 8168-family Ethernet
10. AHCI/SATA
11. HDA audio
12. native Navi 23 graphics acceleration (late milestone)

Early graphical output should use the UEFI-provided framebuffer rather than requiring a native AMD GPU driver.

## Reference peripherals observed

These are useful compatibility targets but are not required for the first boot:

- Logitech USB HID keyboard
- Logitech USB receiver exposing HID mouse/keyboard interfaces
- ASUS USB-BT500 Bluetooth adapter
- HyperX Cloud Stinger 2 Wireless USB audio/HID device
- ITE USB HID RGB controller

## Safety

The original inventory is deliberately not stored in the public repository. Re-run `tools/collect-target.sh` locally whenever low-level diagnostic information is needed.
