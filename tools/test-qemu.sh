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
if [[ "${VIBRIX_QEMU_KEYBOARD_PROBE:-0}" == "1" || "${VIBRIX_QEMU_CONSOLE_PROBE:-0}" == "1" || "${VIBRIX_QEMU_FILES_PROBE:-0}" == "1" ]]; then
  MONITOR_SOCKET="$QEMU_DIR/keyboard-monitor.sock"
  rm -f "$MONITOR_SOCKET"
  MONITOR="unix:$MONITOR_SOCKET,server=on,wait=off"
fi

ARGS=(
  -machine q35
  -accel tcg
  -cpu "${VIBRIX_QEMU_CPU:-max}"
  -m "${VIBRIX_QEMU_RAM:-512M}"
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
if [[ "${VIBRIX_QEMU_USB_ENUM_PROBE:-0}" == "1" ]]; then
  ARGS+=(
    -device "qemu-xhci,id=vibrix-xhci"
    -device "usb-kbd,bus=vibrix-xhci.0"
  )
elif [[ "${VIBRIX_QEMU_XHCI:-0}" == "1" ]]; then
  ARGS+=(-device "qemu-xhci,id=vibrix-xhci")
fi

echo "[vibrix] OVMF CODE: $OVMF_CODE"
echo "[vibrix] OVMF VARS: $OVMF_VARS"
echo "[vibrix] QEMU CPU: ${VIBRIX_QEMU_CPU:-max}"
echo "[vibrix] QEMU RAM: ${VIBRIX_QEMU_RAM:-512M}"
echo "[vibrix] running headless QEMU smoke test"
if [[ "${VIBRIX_QEMU_KEYBOARD_PROBE:-0}" == "1" || "${VIBRIX_QEMU_CONSOLE_PROBE:-0}" == "1" || "${VIBRIX_QEMU_FILES_PROBE:-0}" == "1" ]]; then
  # Connect through QEMU's HMP monitor and send an actual emulated key
  # only after the independent native kernel reports its poll loop ready.
  # Python is host test infrastructure, not part of the Vibrix runtime.
  python3 - "$LOG" "$MONITOR_SOCKET" "${VIBRIX_QEMU_CONSOLE_PROBE:-0}" "${VIBRIX_QEMU_FILES_PROBE:-0}" <<'PY' &
import pathlib
import socket
import sys
import time

log = pathlib.Path(sys.argv[1])
monitor = sys.argv[2]
deadline = time.monotonic() + 9
while time.monotonic() < deadline:
    marker = "VIBRIX: kernel console prompt ready" if sys.argv[3] == "1" or sys.argv[4] == "1" else "VIBRIX: kernel PS2 polling ready"
    if log.exists() and marker in log.read_text(errors="replace"):
        try:
            with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
                client.connect(monitor)
                if sys.argv[4] == "1":
                    commands = ["write /tmp/note hello", "cat /tmp/note", "ls /dev",
                                "pipe", "rm /tmp/note", "cat /tmp/note", "reboot"]
                    special = {" ": "spc", "/": "slash", "\n": "ret"}
                    keys = tuple(special.get(ch, ch) for ch in "\n".join(commands) + "\n")
                elif sys.argv[3] == "1":
                    # Real virtual keyboard: edited help, clear, diagnostics,
                    # uptime, then an actual terminal reboot request.
                    keys = (
                        "h", "e", "l", "x", "backspace", "p", "ret",
                        "c", "l", "e", "a", "r", "ret",
                        "m", "e", "m", "ret",
                        "p", "c", "i", "ret",
                        "a", "c", "p", "i", "ret",
                        "u", "p", "t", "i", "m", "e", "ret",
                        "r", "e", "b", "o", "o", "t", "ret",
                    )
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
QEMU_TIMEOUT=12s
if [[ "${VIBRIX_QEMU_FILES_PROBE:-0}" == "1" ]]; then
  QEMU_TIMEOUT=40s
fi
timeout "$QEMU_TIMEOUT" qemu-system-x86_64 "${ARGS[@]}"
RC=$?
set -e

if [[ "${VIBRIX_QEMU_KEYBOARD_PROBE:-0}" == "1" || "${VIBRIX_QEMU_CONSOLE_PROBE:-0}" == "1" || "${VIBRIX_QEMU_FILES_PROBE:-0}" == "1" ]]; then
  if ! wait "$KEYBOARD_PID"; then
    echo "[vibrix] QEMU native keyboard injection failed" >&2
    [[ -f "$LOG" ]] && cat "$LOG"
    [[ -f "$SERIAL_LOG" ]] && cat "$SERIAL_LOG"
    exit 1
  fi
fi

if [[ ( "${VIBRIX_QEMU_CONSOLE_PROBE:-0}" == "1" || "${VIBRIX_QEMU_FILES_PROBE:-0}" == "1" ) && "$RC" -eq 124 ]]; then
  echo "[vibrix] console reboot command did not reset QEMU before timeout" >&2
  [[ -f "$LOG" ]] && cat "$LOG"
  [[ -f "$SERIAL_LOG" ]] && cat "$SERIAL_LOG"
  exit 1
fi
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
  "VIBRIX: BootInfo v3 staged" \
  "VIBRIX: ExitBootServices succeeded" \
  "VIBRIX: kernel entry after ExitBootServices" \
  "VIBRIX: kernel BootInfo v3 validated" \
  "VIBRIX: kernel GDT/TSS loaded" \
  "VIBRIX: kernel serial initialized" \
  "VIBRIX: kernel IDT installed" \
  "VIBRIX: kernel ACPI RSDP parsed" \
  "VIBRIX: kernel frame allocator initialized" \
  "VIBRIX: kernel conventional frames allocated" \
  "VIBRIX: kernel PCI segment0 enumerated" \
  "VIBRIX: kernel PCI BARs parsed" \
  "VIBRIX: kernel device model populated" \
  "VIBRIX: kernel driver binding registry ready" \
  "VIBRIX: kernel virtual mappings verified" \
  "VIBRIX: kernel ACPI XSDT and MCFG mapped and parsed" \
  "VIBRIX: kernel PCI ECAM bus0 read" \
  "VIBRIX: kernel LAPIC and IOAPIC registers read" \
  "VIBRIX: kernel heap allocation and reuse verified" \
  "VIBRIX: kernel cooperative threads verified" \
  "VIBRIX: kernel VFS and memory filesystem verified" \
  "VIBRIX: kernel file descriptors verified" \
  "VIBRIX: kernel devfs null and zero verified" \
  "VIBRIX: kernel pipe lifecycle verified" \
  "VIBRIX: kernel framebuffer wrote pixels" \
  "VIBRIX: kernel framebuffer status banner drawn"; do
  if ! grep -Fq "$expected" "$LOG"; then
    echo "[vibrix] missing smoke-test marker: $expected" >&2
    [[ -f "$SERIAL_LOG" ]] && cat "$SERIAL_LOG"
    exit 1
  fi
done

python3 - "$LOG" <<'PY'
import pathlib
import sys

text = pathlib.Path(sys.argv[1]).read_text(encoding="utf-8", errors="replace")
markers = [
    "VIBRIX: kernel thread A phase 1",
    "VIBRIX: kernel thread B phase 1",
    "VIBRIX: kernel thread A phase 2",
    "VIBRIX: kernel thread B phase 2",
    "VIBRIX: kernel cooperative threads verified",
]
positions = []
for marker in markers:
    if text.count(marker) != 1:
        raise SystemExit(f"missing or duplicate cooperative-thread marker: {marker}")
    positions.append(text.index(marker))
if positions != sorted(positions):
    raise SystemExit("cooperative-thread execution markers are out of order")
PY

if [[ ! -f "$SERIAL_LOG" ]] || ! grep -Fq "Vibrix kernel started." "$SERIAL_LOG"; then
  echo "[vibrix] missing native kernel COM1 serial output" >&2
  [[ -f "$SERIAL_LOG" ]] && cat "$SERIAL_LOG"
  exit 1
fi
grep -Fq "kernel VM: map, protect, unmap and remap verified" "$SERIAL_LOG"
grep -Eq 'Vibrix ECAM segment0 bus0: [1-9][0-9]* devices, [0-9]+ xHCI' "$SERIAL_LOG"
grep -Eq 'Vibrix APIC: LAPIC id=[0-9]+ version=0x[0-9a-f]+ max_lvt=[1-9][0-9]*, IOAPIC id=[0-9]+ version=0x[0-9a-f]+ max_redir=[1-9][0-9]*' "$SERIAL_LOG"
if [[ "${VIBRIX_QEMU_XHCI:-0}" == "1" ]]; then
  grep -Eq 'Vibrix ECAM segment0 bus0: [1-9][0-9]* devices, [1-9][0-9]* xHCI' "$SERIAL_LOG"
  grep -Eq 'Vibrix device model: [1-9][0-9]* devices, [1-9][0-9]* driver candidates, [1-9][0-9]* xHCI candidates, [0-9]+ RTL8168 candidates' "$SERIAL_LOG"
  grep -Eq 'Vibrix driver bindings: [1-9][0-9]* total, [1-9][0-9]* xHCI, [0-9]+ RTL8168, 0 failures' "$SERIAL_LOG"
fi
grep -Fq "kernel heap: aligned allocations, RAM writes and reuse verified" "$SERIAL_LOG"
if [[ "${VIBRIX_QEMU_KEYBOARD_PROBE:-0}" == "1" ]]; then
  grep -Fq "VIBRIX: kernel PS2 polling ready" "$LOG"
  grep -Fq "VIBRIX: kernel PS2 ASCII accepted" "$LOG"
  # The interactive prompt/echo may prefix the diagnostic line. Anchor the
  # numeric token at EOL so ASCII 104 can never satisfy the ASCII 10 check.
  tr -d '\r' < "$SERIAL_LOG" | grep -Eq 'kernel PS2 ascii 104$'
  tr -d '\r' < "$SERIAL_LOG" | grep -Eq 'kernel PS2 ascii 10$'
fi
cat "$SERIAL_LOG"

if [[ "${VIBRIX_EXPECT_TIMER_IRQ:-0}" == "1" ]]; then
  grep -Fq "VIBRIX: kernel timer IRQ delivered" "$LOG"
  tr -d "\\r" < "$SERIAL_LOG" | grep -Eq "^kernel timer: tick [1-9][0-9]*$"
fi
if [[ "${VIBRIX_QEMU_CONSOLE_PROBE:-0}" == "1" ]]; then
  grep -Fq "VIBRIX: kernel console prompt ready" "$LOG"
  grep -Fq "VIBRIX: kernel console backspace accepted" "$LOG"
  grep -Fq "VIBRIX: kernel console command help" "$LOG"
  grep -Fq "VIBRIX: kernel console command clear" "$LOG"
  grep -Fq "VIBRIX: kernel console command mem" "$LOG"
  grep -Fq "VIBRIX: kernel console command pci" "$LOG"
  grep -Fq "VIBRIX: kernel console command acpi" "$LOG"
  grep -Fq "VIBRIX: kernel console command uptime" "$LOG"
  grep -Fq "VIBRIX: kernel console command reboot" "$LOG"
  grep -Fq "commands: help clear info mem pci acpi uptime reboot" "$SERIAL_LOG"
  grep -Fq $'\033[2J\033[H' "$SERIAL_LOG"
  grep -Fq "reboot: requesting i8042 reset" "$SERIAL_LOG"
  tr -d '\r' < "$SERIAL_LOG" | grep -Eq 'mem: descriptors=[1-9][0-9]* claimed_frames=0x[0-9a-f]+,0x[0-9a-f]+ early_heap_bytes=65536'
  tr -d '\r' < "$SERIAL_LOG" | grep -Eq 'pci: devices=[1-9][0-9]* bars=[1-9][0-9]* xhci=[0-9]+ malformed_bars=0'
  tr -d '\r' < "$SERIAL_LOG" | grep -Eq 'acpi: mcfg_allocations=[1-9][0-9]* ecam_bus0_devices=[1-9][0-9]* ioapics=[1-9][0-9]* timer_gsi=[0-9]+'
  tr -d '\r' < "$SERIAL_LOG" | grep -Eq 'uptime: [1-9][0-9]* ticks \(~[0-9]+\.[0-9][0-9]s\)'
fi

echo "[vibrix] QEMU post-firmware kernel handoff smoke test passed"

if [[ "${VIBRIX_QEMU_FILES_PROBE:-0}" == "1" ]]; then
  for marker in \
    'VIBRIX: kernel VFS and memory filesystem verified' \
    'VIBRIX: kernel file descriptors verified' \
    'VIBRIX: kernel devfs null and zero verified' \
    'VIBRIX: kernel pipe lifecycle verified'; do
    tr -d '\r' < "$LOG" | grep -Fx "$marker"
    tr -d '\r' < "$SERIAL_LOG" | grep -Fx "$marker"
  done
  for line in 'wrote 5 bytes (RAM)' 'hello' 'null' 'zero' 'pipe roundtrip' 'fs: NotFound'; do
    tr -d '\r' < "$SERIAL_LOG" | grep -Fx "$line"
  done
  grep -Fq 'VIBRIX: kernel filesystem command completed' "$LOG"
  grep -Fq 'VIBRIX: kernel console command reboot' "$LOG"
  echo '[vibrix] QEMU retained RAM file and mounted device commands passed'
fi
