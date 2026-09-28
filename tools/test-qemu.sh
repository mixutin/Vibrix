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

# Opt-in *real QEMU keyboard injection*, never manufactured kernel log text.
MONITOR=none
if [[ "${VIBRIX_QEMU_KEYBOARD_PROBE:-0}" == "1" || "${VIBRIX_QEMU_CONSOLE_PROBE:-0}" == "1" ]]; then
  MONITOR_SOCKET="$QEMU_DIR/keyboard-monitor.sock"
  rm -f "$MONITOR_SOCKET"
  MONITOR="unix:$MONITOR_SOCKET,server=on,wait=off"
fi

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
  -monitor "$MONITOR"
  -no-reboot
  -chardev "file,id=vibrixdbg,path=$LOG"
  -device "isa-debugcon,iobase=0xe9,chardev=vibrixdbg"
)

# Optionally expose a virtual PCI xHCI controller to the *native kernel*.
# This does not connect a persistent USB system disk or enable a USB driver.
if [[ "${VIBRIX_QEMU_XHCI:-0}" == "1" ]]; then
  ARGS+=(-device "qemu-xhci,id=vibrix-xhci")
fi

echo "[vibrix] OVMF CODE: $OVMF_CODE"
echo "[vibrix] OVMF VARS: $OVMF_VARS"
echo "[vibrix] running headless QEMU smoke test"
if [[ "${VIBRIX_QEMU_KEYBOARD_PROBE:-0}" == "1" || "${VIBRIX_QEMU_CONSOLE_PROBE:-0}" == "1" ]]; then
  # Connect through QEMU's HMP monitor and send an actual emulated key
  # only after the independent native kernel reports its poll loop ready.
  # Python is host test infrastructure, not part of the Vibrix runtime.
  python3 - "$LOG" "$MONITOR_SOCKET" "${VIBRIX_QEMU_CONSOLE_PROBE:-0}" <<'PY' &
import pathlib
import socket
import sys
import time

log = pathlib.Path(sys.argv[1])
monitor = sys.argv[2]
deadline = time.monotonic() + 9
while time.monotonic() < deadline:
    marker = "VIBRIX: kernel console prompt ready" if sys.argv[3] == "1" else "VIBRIX: kernel PS2 polling ready"
    if log.exists() and marker in log.read_text(errors="replace"):
        try:
            with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
                client.connect(monitor)
                if sys.argv[3] == "1":
                    # Real virtual keyboard: "helx", Backspace, "p", Return.
                    keys = ("h", "e", "l", "x", "backspace", "p", "ret")
                else:
                    keys = ("h", "ret")
                for key in keys:
                    client.sendall(("sendkey " + key + "\n").encode("ascii"))
                    time.sleep(0.18)
                time.sleep(0.4)  # keep HMP alive through key release
                break
        except OSError:
            pass
    time.sleep(0.05)
else:
    sys.exit("kernel PS2 polling marker/keyboard monitor not ready")
PY
  KEYBOARD_PID=$!
fi
set +e
timeout 12s qemu-system-x86_64 "${ARGS[@]}"
RC=$?
set -e

if [[ "${VIBRIX_QEMU_KEYBOARD_PROBE:-0}" == "1" || "${VIBRIX_QEMU_CONSOLE_PROBE:-0}" == "1" ]]; then
  if ! wait "$KEYBOARD_PID"; then
    echo "[vibrix] QEMU native keyboard injection failed" >&2
    [[ -f "$LOG" ]] && cat "$LOG"
    [[ -f "$SERIAL_LOG" ]] && cat "$SERIAL_LOG"

if [[ "${VIBRIX_EXPECT_TIMER_IRQ:-0}" == "1" ]]; then
  grep -Fq "VIBRIX: kernel timer IRQ delivered" "$LOG"
  tr -d '\r' < "$SERIAL_LOG" | grep -Eq '^kernel timer: tick [1-9][0-9]*$'
fi

if [[ "${VIBRIX_QEMU_CONSOLE_PROBE:-0}" == "1" ]]; then
  grep -Fq "VIBRIX: kernel console prompt ready" "$LOG"
  grep -Fq "VIBRIX: kernel console backspace accepted" "$LOG"
  grep -Fq "VIBRIX: kernel console command help" "$LOG"
  grep -Fq "commands: help info" "$SERIAL_LOG"
fi

echo "[vibrix] QEMU post-firmware kernel handoff smoke test passed"
