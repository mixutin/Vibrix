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

# The UEFI text console stops reporting progress at ExitBootServices and the
# TianoCore splash can remain visible. Preserve the *kernel's own* separate
# debug port and COM1 logs so interactive runs have post-firmware evidence.
INTERACTIVE_DEBUG="$QEMU_DIR/interactive-debugcon.log"
INTERACTIVE_SERIAL="$QEMU_DIR/interactive-serial.log"
rm -f -- "$INTERACTIVE_DEBUG" "$INTERACTIVE_SERIAL"

ARGS=(
  -machine q35
  -cpu max
  -m 512M
  -drive "if=pflash,format=raw,readonly=on,file=$OVMF_CODE"
  -drive "if=pflash,format=raw,file=$QEMU_DIR/OVMF_VARS.fd"
  -drive "format=raw,file=fat:rw:$QEMU_DIR/esp"
  -serial "file:$INTERACTIVE_SERIAL"
  -chardev "file,id=vibrixdbg,path=$INTERACTIVE_DEBUG"
  -device "isa-debugcon,iobase=0xe9,chardev=vibrixdbg"
  -no-reboot
)

# Optional read-only PCI xHCI discovery proof; no native xHCI driver,
# attached boot USB or persistent root is implied by this virtual device.
if [[ "${VIBRIX_QEMU_XHCI:-0}" == "1" ]]; then
  ARGS+=(-device "qemu-xhci,id=vibrix-xhci")
fi

echo "[vibrix] OVMF CODE: $OVMF_CODE"
echo "[vibrix] OVMF VARS: $OVMF_VARS"
echo "[vibrix] starting graphical QEMU"
echo "[vibrix] native post-firmware debugcon: $INTERACTIVE_DEBUG"
echo "[vibrix] native kernel COM1 serial: $INTERACTIVE_SERIAL"
echo "[vibrix] in another terminal: tail -f '$INTERACTIVE_DEBUG' '$INTERACTIVE_SERIAL'"
exec qemu-system-x86_64 "${ARGS[@]}"
