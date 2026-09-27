#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
# shellcheck source=tools/ovmf.inc
source "$ROOT/tools/ovmf.inc"

# Fail early on host setup problems, not after a potentially long rebuild.
vibrix_find_ovmf
if ! command -v qemu-system-x86_64 >/dev/null 2>&1; then
  echo "[vibrix] qemu-system-x86_64 not found; install qemu-system-x86." >&2
  exit 1
fi

if [[ "${VIBRIX_SKIP_BUILD:-0}" != "1" ]]; then
  bash "$ROOT/tools/build-qemu.sh"
fi

QEMU_DIR="$ROOT/build/qemu"
LOG="$QEMU_DIR/debugcon.log"
SERIAL_LOG="$QEMU_DIR/serial.log"
rm -f "$LOG" "$SERIAL_LOG"
cp -- "$OVMF_VARS" "$QEMU_DIR/OVMF_VARS.test.fd"

ARGS=(
  -machine q35
  -accel tcg
  -cpu max
  -m 512M
  -drive "if=pflash,format=raw,readonly=on,file=$OVMF_CODE"
  -drive "if=pflash,format=raw,file=$QEMU_DIR/OVMF_VARS.test.fd"
  -drive "format=raw,file=fat:rw:$QEMU_DIR/esp"
  -display none
  -serial "file:$SERIAL_LOG"
  -monitor none
  -no-reboot
  -chardev "file,id=vibrixdbg,path=$LOG"
  -device "isa-debugcon,iobase=0xe9,chardev=vibrixdbg"
)

echo "[vibrix] OVMF CODE: $OVMF_CODE"
echo "[vibrix] OVMF VARS: $OVMF_VARS"
echo "[vibrix] running headless QEMU smoke test"
set +e
timeout 12s qemu-system-x86_64 "${ARGS[@]}"
RC=$?
set -e

if [[ "$RC" -ne 0 && "$RC" -ne 124 ]]; then
  echo "[vibrix] QEMU exited unexpectedly with status $RC" >&2
  [[ -f "$LOG" ]] && cat "$LOG"
  exit "$RC"
fi

if [[ ! -f "$LOG" ]]; then
  echo "[vibrix] QEMU debug log not created: $LOG" >&2
  exit 1
fi
cat "$LOG"

for expected in \
  "VIBRIX: bootloader entered" \
  "VIBRIX: kernel.elf found" \
  "VIBRIX: ELF64 valid" \
  "VIBRIX: x86_64 executable validated" \
  "VIBRIX: PT_LOAD parsed" \
  "VIBRIX: kernel validated" \
  "VIBRIX: ACPI RSDP validated" \
  "VIBRIX: GOP framebuffer discovered" \
  "VIBRIX: kernel segments staged" \
  "VIBRIX: kernel page tables verified" \
  "VIBRIX: transition mappings verified" \
  "VIBRIX: final memory map captured" \
  "VIBRIX: BootInfo v2 staged" \
  "VIBRIX: ExitBootServices succeeded" \
  "VIBRIX: kernel entry after ExitBootServices" \
  "VIBRIX: kernel BootInfo v2 validated" \
  "VIBRIX: kernel GDT/TSS loaded" \
  "VIBRIX: kernel serial initialized" \
  "VIBRIX: kernel IDT installed" \
  "VIBRIX: kernel framebuffer wrote pixels"; do
  if ! grep -Fq "$expected" "$LOG"; then
    echo "[vibrix] missing smoke-test marker: $expected" >&2
    exit 1
  fi
done

if [[ ! -f "$SERIAL_LOG" ]] || ! grep -Fq "Vibrix kernel started." "$SERIAL_LOG"; then
  echo "[vibrix] missing native kernel COM1 serial output" >&2
  [[ -f "$SERIAL_LOG" ]] && cat "$SERIAL_LOG"
  exit 1
fi
cat "$SERIAL_LOG"

echo "[vibrix] QEMU post-firmware kernel handoff smoke test passed"
