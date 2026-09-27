#!/usr/bin/env bash
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
"$ROOT/tools/build-qemu.sh"

OVMF_CODE=""
for candidate in   /usr/share/OVMF/OVMF_CODE_4M.fd   /usr/share/OVMF/OVMF_CODE.fd   /usr/share/edk2/x64/OVMF_CODE.fd; do
  if [[ -f "$candidate" ]]; then
    OVMF_CODE="$candidate"
    break
  fi
done

if [[ -z "$OVMF_CODE" ]]; then
  echo "OVMF firmware not found. Install qemu-system-x86 and ovmf." >&2
  exit 1
fi

exec qemu-system-x86_64   -machine q35   -cpu max   -m 512M   -bios "$OVMF_CODE"   -drive format=raw,file=fat:rw:"$ROOT/build/qemu/esp"   -no-reboot
