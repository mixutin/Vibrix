#!/usr/bin/env bash
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT"

if [[ "${VIBRIX_SKIP_BUILD:-0}" != "1" ]]; then
  bash "$ROOT/tools/build-qemu.sh"
fi

find_ovmf() {
  for candidate in \
    /usr/share/OVMF/OVMF_CODE_4M.fd \
    /usr/share/OVMF/OVMF_CODE.fd \
    /usr/share/edk2/x64/OVMF_CODE.fd \
    /usr/share/edk2/ovmf/OVMF_CODE.fd; do
    if [[ -r "$candidate" ]]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done
  return 1
}

OVMF_CODE="$(find_ovmf || true)"
if [[ -z "$OVMF_CODE" ]]; then
  echo "[vibrix] readable OVMF firmware not found" >&2
  exit 1
fi

OVMF_VARS=""
for candidate in \
  /usr/share/OVMF/OVMF_VARS_4M.fd \
  /usr/share/OVMF/OVMF_VARS.fd \
  /usr/share/edk2/x64/OVMF_VARS.fd \
  /usr/share/edk2/ovmf/OVMF_VARS.fd; do
  if [[ -r "$candidate" ]]; then
    OVMF_VARS="$candidate"
    break
  fi
done

QEMU_DIR="$ROOT/build/qemu"
LOG="$QEMU_DIR/debugcon.log"
rm -f "$LOG"

ARGS=(
  -machine q35
  -accel tcg
  -cpu max
  -m 512M
  -drive "if=pflash,format=raw,readonly=on,file=$OVMF_CODE"
  -drive "format=raw,file=fat:rw:$QEMU_DIR/esp"
  -display none
  -serial none
  -monitor none
  -no-reboot
  -chardev "file,id=vibrixdbg,path=$LOG"
  -device "isa-debugcon,iobase=0xe9,chardev=vibrixdbg"
)

if [[ -n "$OVMF_VARS" ]]; then
  cp "$OVMF_VARS" "$QEMU_DIR/OVMF_VARS.test.fd"
  ARGS+=(-drive "if=pflash,format=raw,file=$QEMU_DIR/OVMF_VARS.test.fd")
fi

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
  "VIBRIX: kernel page tables verified"; do
  if ! grep -Fq "$expected" "$LOG"; then
    echo "[vibrix] missing smoke-test marker: $expected" >&2
    exit 1
  fi
done

echo "[vibrix] QEMU kernel validation smoke test passed"
