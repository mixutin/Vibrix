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

# Optionally expose a virtual PCI xHCI controller to the *native kernel*.
# This does not connect a persistent USB system disk or enable a USB driver.
if [[ "${VIBRIX_QEMU_XHCI:-0}" == "1" ]]; then
  ARGS+=(-device "qemu-xhci,id=vibrix-xhci")
fi

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
  "VIBRIX: kernel virtual mappings verified" \
  "VIBRIX: kernel ACPI XSDT and MCFG mapped and parsed" \
  "VIBRIX: kernel PCI ECAM bus0 read" \
  "VIBRIX: kernel heap allocation and reuse verified" \
  "VIBRIX: kernel framebuffer wrote pixels" \
  "VIBRIX: kernel framebuffer status banner drawn"; do
  if ! grep -Fq "$expected" "$LOG"; then
    echo "[vibrix] missing smoke-test marker: $expected" >&2
    [[ -f "$SERIAL_LOG" ]] && cat "$SERIAL_LOG"
    exit 1
  fi
done

if [[ ! -f "$SERIAL_LOG" ]] || ! grep -Fq "Vibrix kernel started." "$SERIAL_LOG"; then
  echo "[vibrix] missing native kernel COM1 serial output" >&2
  [[ -f "$SERIAL_LOG" ]] && cat "$SERIAL_LOG"
  exit 1
fi
grep -Fq "kernel VM: map, protect, unmap and remap verified" "$SERIAL_LOG"
grep -Eq 'Vibrix ECAM segment0 bus0: [1-9][0-9]* devices, [0-9]+ xHCI' "$SERIAL_LOG"
if [[ "${VIBRIX_QEMU_XHCI:-0}" == "1" ]]; then
  grep -Eq 'Vibrix ECAM segment0 bus0: [1-9][0-9]* devices, [1-9][0-9]* xHCI' "$SERIAL_LOG"
fi
grep -Fq "kernel heap: aligned allocations, RAM writes and reuse verified" "$SERIAL_LOG"
cat "$SERIAL_LOG"

echo "[vibrix] QEMU post-firmware kernel handoff smoke test passed"
