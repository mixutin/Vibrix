#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
# shellcheck source=tools/ovmf.inc
source "$ROOT/tools/ovmf.inc"
vibrix_find_ovmf
command -v qemu-system-x86_64 >/dev/null
bash tools/build-qemu.sh
OUT="$ROOT/build/qemu"
cp -- "$OVMF_VARS" "$OUT/OVMF_VARS.pci.fd"

set +e
timeout 25s qemu-system-x86_64 \
  -machine q35 -accel tcg -cpu max -smp 1 -m 512M \
  -drive "if=pflash,format=raw,readonly=on,file=$OVMF_CODE" \
  -drive "if=pflash,format=raw,file=$OUT/OVMF_VARS.pci.fd" \
  -drive "format=raw,file=fat:$OUT/esp,readonly=on" \
  -device edu -net none -display none -monitor none -no-reboot \
  -serial "file:$OUT/pci-serial.log" \
  -chardev "file,id=pcidbg,path=$OUT/pci-debugcon.log" \
  -device isa-debugcon,iobase=0xe9,chardev=pcidbg \
  >"$OUT/pci-qemu.log" 2>&1
status=$?
set -e
cat "$OUT/pci-qemu.log" "$OUT/pci-debugcon.log" "$OUT/pci-serial.log"
test "$status" -eq 124
grep -Fq 'VIBRIX: kernel entry after ExitBootServices' "$OUT/pci-debugcon.log"
grep -Fq 'VIBRIX: EDU MSI capability found' "$OUT/pci-debugcon.log"
grep -Fq 'VIBRIX: PCI interrupt inventory complete' "$OUT/pci-debugcon.log"
grep -Fq 'PCI MSI 1234:11e8' "$OUT/pci-serial.log"
grep -Fq 'Vibrix kernel started.' "$OUT/pci-serial.log"
if [[ "${VIBRIX_EXPECT_MSI_IRQ:-0}" == "1" ]]; then
  grep -Fq 'VIBRIX: native EDU MSI armed' "$OUT/pci-debugcon.log"
  grep -Fq 'VIBRIX: native MSI repeated delivery verified' "$OUT/pci-debugcon.log"
  grep -Fq 'VIBRIX: kernel console prompt ready' "$OUT/pci-debugcon.log"
  grep -Fq 'kernel MSI: delivered=2 acknowledged=2 disabled=true bus_master=false' "$OUT/pci-serial.log"
  echo 'Native EDU MSI: repeated delivery, acknowledgement, disable and continued boot verified.'
else
  if grep -Fq 'VIBRIX: native EDU MSI armed' "$OUT/pci-debugcon.log"; then
    echo 'Default inventory unexpectedly activated the MSI diagnostic.' >&2
    exit 1
  fi
  echo 'Native read-only PCI interrupt inventory verified against QEMU EDU.'
fi
