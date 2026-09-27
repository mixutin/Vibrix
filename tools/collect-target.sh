#!/usr/bin/env bash
set -u

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
OUT_DIR="$ROOT/targets/raw"
OUT="$OUT_DIR/target-001.txt"
mkdir -p "$OUT_DIR"

need() { command -v "$1" >/dev/null 2>&1 || echo "[missing command: $1]"; }
for c in lscpu lspci lsusb dmidecode efibootmgr lsblk blkid findmnt ip; do need "$c"; done

{
echo "=== VIBRIX TARGET 001 ==="
date -u
echo
echo "=== SYSTEM ==="
uname -a
cat /etc/os-release 2>/dev/null
hostnamectl 2>/dev/null
echo
echo "=== CPU ==="
lscpu
grep -m1 '^flags' /proc/cpuinfo
echo
echo "=== DMI / SMBIOS ==="
sudo dmidecode 2>/dev/null
echo
echo "=== UEFI ==="
[ -d /sys/firmware/efi ] && echo "Booted using UEFI: YES" || echo "Booted using UEFI: NO"
sudo efibootmgr -v 2>/dev/null
echo
echo "=== ACPI TABLES ==="
ls -lah /sys/firmware/acpi/tables 2>/dev/null
echo
echo "=== PCI / PCIE ==="
lspci -nn 2>/dev/null
lspci -nnk 2>/dev/null
sudo lspci -vvnn 2>/dev/null
echo
echo "=== STORAGE ==="
lsblk -e7 -o NAME,PATH,MAJ:MIN,TRAN,SIZE,TYPE,FSTYPE,FSVER,MODEL,SERIAL,WWN,PARTTYPE,PARTUUID,MOUNTPOINTS 2>/dev/null
sudo blkid 2>/dev/null
findmnt 2>/dev/null
echo
echo "=== NVME ==="
command -v nvme >/dev/null && sudo nvme list 2>/dev/null
echo
echo "=== USB ==="
lsusb 2>/dev/null
lsusb -t 2>/dev/null
sudo lsusb -v 2>/dev/null
echo
echo "=== NETWORK ==="
ip -details link 2>/dev/null
ip addr 2>/dev/null
lspci -nnk 2>/dev/null | grep -A4 -Ei 'ethernet|network|wireless'
echo
echo "=== GRAPHICS ==="
lspci -nnk 2>/dev/null | grep -A6 -Ei 'vga|3d|display'
for card in /sys/class/drm/card*/device; do
  [ -e "$card" ] || continue
  echo "--- $card ---"
  cat "$card/vendor" 2>/dev/null
  cat "$card/device" 2>/dev/null
done
echo
echo "=== DRM / DISPLAYS ==="
ls -l /sys/class/drm 2>/dev/null
echo
echo "=== AUDIO ==="
lspci -nnk 2>/dev/null | grep -A5 -i audio
cat /proc/asound/cards 2>/dev/null
echo
echo "=== INPUT ==="
cat /proc/bus/input/devices 2>/dev/null
echo
echo "=== IOMMU ==="
find /sys/kernel/iommu_groups -maxdepth 2 -type l 2>/dev/null
echo
echo "=== INTERRUPTS ==="
cat /proc/interrupts 2>/dev/null
echo
echo "=== MEMORY MAP ==="
cat /proc/iomem 2>/dev/null
echo
echo "=== KERNEL HARDWARE MESSAGES ==="
sudo dmesg 2>/dev/null | grep -Ei 'acpi|apic|iommu|dmar|amd-vi|pci|pcie|nvme|ahci|sata|usb|xhci|ethernet|network|wifi|wlan|drm|vga|audio'
} > "$OUT" 2>&1

echo
echo "Vibrix Target 001 profile written to:"
echo "$OUT"
echo
echo "WARNING: raw profiles may contain serial numbers, UUIDs and other identifiers."
echo "targets/raw/ is gitignored. Send the file for sanitization before committing a target profile."
wc -l "$OUT"
