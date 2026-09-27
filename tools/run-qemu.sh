#!/usr/bin/env bash
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
"$ROOT/tools/build-qemu.sh"

find_ovmf() {
  for candidate in     /usr/share/OVMF/OVMF_CODE_4M.fd     /usr/share/OVMF/OVMF_CODE.fd     /usr/share/edk2/x64/OVMF_CODE.fd     /usr/share/edk2/ovmf/OVMF_CODE.fd; do
    if [[ -r "$candidate" ]]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done
  return 1
}

OVMF_CODE="$(find_ovmf || true)"
if [[ -z "$OVMF_CODE" ]]; then
  echo "[vibrix] readable OVMF firmware not found." >&2
  echo "[vibrix] Try: dpkg -L ovmf | grep -E 'OVMF.*CODE.*fd$'" >&2
  exit 1
fi

OVMF_VARS=""
for candidate in   /usr/share/OVMF/OVMF_VARS_4M.fd   /usr/share/OVMF/OVMF_VARS.fd   /usr/share/edk2/x64/OVMF_VARS.fd   /usr/share/edk2/ovmf/OVMF_VARS.fd; do
  if [[ -r "$candidate" ]]; then
    OVMF_VARS="$candidate"
    break
  fi
done

QEMU_DIR="$ROOT/build/qemu"
ARGS=(
  -machine q35
  -cpu max
  -m 512M
  -drive "if=pflash,format=raw,readonly=on,file=$OVMF_CODE"
  -drive "format=raw,file=fat:rw:$QEMU_DIR/esp"
  -no-reboot
)

if [[ -n "$OVMF_VARS" ]]; then
  cp "$OVMF_VARS" "$QEMU_DIR/OVMF_VARS.fd"
  ARGS+=(-drive "if=pflash,format=raw,file=$QEMU_DIR/OVMF_VARS.fd")
fi

echo "[vibrix] OVMF: $OVMF_CODE"
echo "[vibrix] starting QEMU"
exec qemu-system-x86_64 "${ARGS[@]}"
