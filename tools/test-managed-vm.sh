#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
PROBE="${1:-}"
case "$PROBE" in
  write|unmap|guard|nx) ;;
  *) echo "usage: $0 {write|unmap|guard|nx}" >&2; exit 2 ;;
esac
# shellcheck source=tools/ovmf.inc
source "$ROOT/tools/ovmf.inc"
vibrix_find_ovmf
command -v qemu-system-x86_64 >/dev/null
command -v python3 >/dev/null

VIBRIX_KERNEL_FEATURES="qemu-debugcon,managed-${PROBE}-probe" bash tools/build-qemu.sh
OUT="$ROOT/build/qemu"
LOG="$OUT/managed-debugcon.log"
SERIAL="$OUT/managed-serial.log"
rm -f -- "$LOG" "$SERIAL"
cp -- "$OVMF_VARS" "$OUT/OVMF_VARS.managed.fd"

set +e
timeout 15s qemu-system-x86_64 \
  -machine q35 -accel tcg -cpu max -m 512M \
  -drive "if=pflash,format=raw,readonly=on,file=$OVMF_CODE" \
  -drive "if=pflash,format=raw,file=$OUT/OVMF_VARS.managed.fd" \
  -drive "format=raw,file=fat:rw:$OUT/esp" \
  -display none -serial "file:$SERIAL" -monitor none -no-reboot \
  -chardev "file,id=manageddbg,path=$LOG" \
  -device "isa-debugcon,iobase=0xe9,chardev=manageddbg"
RC=$?
set -e
for log in "$LOG" "$SERIAL"; do
  if [[ -f "$log" ]]; then cat "$log"; fi
done
if [[ "$RC" -ne 0 && "$RC" -ne 124 ]]; then
  echo "managed VM QEMU exited unexpectedly: $RC" >&2
  exit "$RC"
fi
python3 tools/check-managed-vm.py "$PROBE" "$LOG" "$SERIAL"
