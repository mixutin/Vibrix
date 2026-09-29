#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=tools/ovmf.inc
source "$ROOT/tools/ovmf.inc"

VNC=""
FEATURES="${VIBRIX_KERNEL_FEATURES:-userspace-shell}"
for argument in "$@"; do
  case "$argument" in
    --vnc) VNC=0 ;;
    --vnc=*) VNC="${argument#--vnc=}" ;;
    --kernel-console) FEATURES=qemu-debugcon ;;
    --desktop) FEATURES=userspace-desktop ;;
    --help|-h)
      echo "Usage: $0 [--vnc[=0..99]] [--kernel-console | --desktop]"
      echo "Default: build and boot the Ring 3 shell in the QEMU graphics window."
      echo "--vnc: show the same guest display at 127.0.0.1:5900 (display 0)."
      echo "--desktop: boot the native Ring 3 desktop preview with terminal and files."
      echo "--kernel-console: boot the legacy serial development console instead."
      exit 0
      ;;
    *) echo "[vibrix] unknown option: $argument" >&2; exit 2 ;;
  esac
done
if [[ -n "$VNC" && ! "$VNC" =~ ^([0-9]|[1-9][0-9])$ ]]; then
  echo "[vibrix] VNC display must be an integer from 0 to 99." >&2
  exit 2
fi
# An explicitly empty --vnc= is an error, not an accidental graphical launch.
for argument in "$@"; do
  if [[ "$argument" == "--vnc=" ]]; then
    echo "[vibrix] --vnc= requires a display number." >&2
    exit 2
  fi
done

# Check prerequisites before rebuilding; no physical disk is selected.
vibrix_find_ovmf
if ! command -v qemu-system-x86_64 >/dev/null 2>&1; then
  echo "[vibrix] qemu-system-x86_64 not found; install qemu-system-x86." >&2
  exit 1
fi

cd "$ROOT"
VIBRIX_KERNEL_FEATURES="$FEATURES" "$ROOT/tools/build-qemu.sh"

QEMU_DIR="$ROOT/build/qemu"
cp -- "$OVMF_VARS" "$QEMU_DIR/OVMF_VARS.fd"
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

if [[ -n "$VNC" ]]; then
  # Never expose an unauthenticated VNC listener on all network interfaces.
  ARGS+=(-display none -vnc "127.0.0.1:$VNC" -k en-us)
  echo "[vibrix] VNC: 127.0.0.1:$((5900 + VNC)) (display :$VNC)"
  echo "[vibrix] Remote access: use an SSH tunnel; VNC itself is not encrypted."
else
  echo "[vibrix] starting graphical QEMU; click the guest window to type"
fi

# Optional local QMP socket for developer automation and screenshot tests.
# It is never exposed over TCP. Reject QEMU option separators in this path.
if [[ -n "${VIBRIX_QEMU_QMP_SOCKET:-}" ]]; then
  if [[ "$VIBRIX_QEMU_QMP_SOCKET" == *,* ]]; then
    echo "[vibrix] QMP socket path must not contain commas." >&2
    exit 2
  fi
  ARGS+=(-qmp "unix:$VIBRIX_QEMU_QMP_SOCKET,server=on,wait=off")
fi
if [[ "${VIBRIX_QEMU_XHCI:-0}" == "1" ]]; then
  ARGS+=(-device "qemu-xhci,id=vibrix-xhci")
fi

echo "[vibrix] kernel profile: $FEATURES"
echo "[vibrix] OVMF CODE: $OVMF_CODE"
echo "[vibrix] OVMF VARS: $OVMF_VARS"
echo "[vibrix] post-firmware debugcon: $INTERACTIVE_DEBUG"
echo "[vibrix] mirrored COM1 serial: $INTERACTIVE_SERIAL"
echo "[vibrix] the userspace profile displays its own vibrix\$ prompt on the guest framebuffer"
exec qemu-system-x86_64 "${ARGS[@]}"
