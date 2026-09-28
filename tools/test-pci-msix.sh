#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
# shellcheck source=tools/ovmf.inc
source "$ROOT/tools/ovmf.inc"
vibrix_find_ovmf
command -v qemu-system-x86_64 >/dev/null
command -v python3 >/dev/null

OUT="$ROOT/build/qemu"
SOCKET="$OUT/ivshmem-test.sock"
SERVER_LOG="$OUT/msix-server.log"
DEBUG_LOG="$OUT/msix-debugcon.log"
SERIAL_LOG="$OUT/msix-serial.log"
QEMU_LOG="$OUT/msix-qemu.log"

VIBRIX_KERNEL_FEATURES=qemu-debugcon,pci-msix-probe bash tools/build-qemu.sh
rm -f -- "$SOCKET" "$SERVER_LOG" "$DEBUG_LOG" "$SERIAL_LOG" "$QEMU_LOG"
cp -- "$OVMF_VARS" "$OUT/OVMF_VARS.msix.fd"

python3 tools/ivshmem_test_server.py --socket "$SOCKET" --debug-log "$DEBUG_LOG" >"$SERVER_LOG" 2>&1 &
server_pid=$!
cleanup() {
  kill "$server_pid" 2>/dev/null || true
  wait "$server_pid" 2>/dev/null || true
}
trap cleanup EXIT

for _ in $(seq 1 200); do
  [[ -S "$SOCKET" ]] && break
  sleep 0.02
done
[[ -S "$SOCKET" ]] || { echo "ivshmem test server did not create socket" >&2; exit 1; }

set +e
timeout 25s qemu-system-x86_64 \
  -machine q35 -accel tcg -cpu max -smp 1 -m 512M \
  -drive "if=pflash,format=raw,readonly=on,file=$OVMF_CODE" \
  -drive "if=pflash,format=raw,file=$OUT/OVMF_VARS.msix.fd" \
  -drive "format=raw,file=fat:rw:$OUT/esp" \
  -chardev "socket,path=$SOCKET,id=ivshmem" \
  -device "ivshmem-doorbell,vectors=1,chardev=ivshmem" \
  -net none -display none -monitor none -no-reboot \
  -serial "file:$SERIAL_LOG" \
  -chardev "file,id=msixdbg,path=$DEBUG_LOG" \
  -device isa-debugcon,iobase=0xe9,chardev=msixdbg \
  >"$QEMU_LOG" 2>&1
status=$?
set -e

cat "$SERVER_LOG" "$QEMU_LOG" "$DEBUG_LOG" "$SERIAL_LOG"
test "$status" -eq 124
grep -Fq 'VIBRIX: kernel entry after ExitBootServices' "$DEBUG_LOG"
grep -Fq 'VIBRIX: IVSHMEM MSI-X capability found' "$DEBUG_LOG"
grep -Fq 'VIBRIX: native IVSHMEM MSI-X armed' "$DEBUG_LOG"
grep -Fq 'VIBRIX: native MSI-X first delivery observed' "$DEBUG_LOG"
grep -Fq 'VIBRIX: native MSI-X repeated delivery verified' "$DEBUG_LOG"
grep -Fq 'VIBRIX: native MSI-X disabled' "$DEBUG_LOG"
grep -Fq 'VIBRIX: native MSI-X post-disable silence verified' "$DEBUG_LOG"
grep -Fq 'VIBRIX: kernel console prompt ready' "$DEBUG_LOG"
grep -Fq 'kernel MSI-X: delivered=2 disabled=true bus_master=false post_disable_silent=true' "$SERIAL_LOG"
for number in 1 2 3; do
  grep -Fq "trigger $number: eventfd += 1" "$SERVER_LOG"
done
echo 'Native ivshmem MSI-X: repeated eventfd delivery, disable and post-disable silence verified.'
