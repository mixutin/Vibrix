#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=tools/ovmf.inc
source "$ROOT/tools/ovmf.inc"

# Check host prerequisites *before* rebuilding two Rust artifacts.
vibrix_find_ovmf
if ! command -v qemu-system-x86_64 >/dev/null 2>&1; then
  echo "[vibrix] qemu-system-x86_64 not found; install qemu-system-x86." >&2
  exit 1
fi

"$ROOT/tools/build-qemu.sh"

QEMU_DIR="$ROOT/build/qemu"
# Firmware variables are writable per-run copies, never the system template.
cp -- "$OVMF_VARS" "$QEMU_DIR/OVMF_VARS.fd"

ARGS=(
  -machine q35
  -cpu max
  -m 512M
  -drive "if=pflash,format=raw,readonly=on,file=$OVMF_CODE"
  -drive "if=pflash,format=raw,file=$QEMU_DIR/OVMF_VARS.fd"
  -drive "format=raw,file=fat:rw:$QEMU_DIR/esp"
  -no-reboot
)

echo "[vibrix] OVMF CODE: $OVMF_CODE"
echo "[vibrix] OVMF VARS: $OVMF_VARS"
echo "[vibrix] starting QEMU"
exec qemu-system-x86_64 "${ARGS[@]}"
