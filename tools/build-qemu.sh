#!/usr/bin/env bash
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT"

cargo build -p vibrix-boot --target x86_64-unknown-uefi

OUT="$ROOT/build/qemu"
ESP="$OUT/esp"
rm -rf "$OUT"
mkdir -p "$ESP/EFI/BOOT"
cp target/x86_64-unknown-uefi/debug/vibrix-boot.efi "$ESP/EFI/BOOT/BOOTX64.EFI"

echo "Built Vibrix QEMU EFI tree:"
echo "$ESP/EFI/BOOT/BOOTX64.EFI"
