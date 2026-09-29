#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
source "$ROOT/tools/ovmf.inc"
vibrix_find_ovmf
command -v qemu-system-x86_64 >/dev/null
bash tools/build-qemu.sh
for cpus in 1 4 16; do
  out="$ROOT/build/qemu/cpus-$cpus"
  mkdir -p "$out"
  cp -- "$OVMF_VARS" "$out/vars.fd"
  set +e
  timeout 25s qemu-system-x86_64 \
    -machine q35 -accel tcg -cpu max -smp "$cpus" -m 512M \
    -drive "if=pflash,format=raw,readonly=on,file=$OVMF_CODE" \
    -drive "if=pflash,format=raw,file=$out/vars.fd" \
    -drive "format=raw,file=fat:rw:$ROOT/build/qemu/esp" \
    -display none -serial "file:$out/serial.log" -monitor none -no-reboot \
    -chardev "file,id=dbg,path=$out/debugcon.log" \
    -device "isa-debugcon,iobase=0xe9,chardev=dbg"
  status=$?
  set -e
  cat "$out/debugcon.log" "$out/serial.log"
  if [[ "$status" != 124 ]]; then
    echo "CPU enumeration VM exited unexpectedly: $status" >&2
    exit 1
  fi
  python3 - "$out" "$cpus" <<'PY'
import pathlib
import re
import sys
root = pathlib.Path(sys.argv[1])
expected = int(sys.argv[2])
serial = (root / "serial.log").read_text().splitlines()
debug = (root / "debugcon.log").read_text().splitlines()
marker = "VIBRIX: kernel CPU enumeration verified"
assert marker in serial and marker in debug, "missing independent CPU evidence"
assert f"kernel CPUs: enabled={expected} online_capable=0 total={expected}" in serial
per_cpu_marker = "VIBRIX: kernel per-CPU topology published"
bsp_marker = "VIBRIX: kernel per-CPU BSP bound"
assert per_cpu_marker in serial and per_cpu_marker in debug, "missing per-CPU publication evidence"
assert bsp_marker in serial and bsp_marker in debug, "missing BSP per-CPU binding evidence"
assert f"kernel per-CPU table: slots={expected} published=true" in serial
records = [re.fullmatch(r"kernel CPU enabled: uid=(\d+) apic=(\d+) x2apic=(true|false)", line) for line in serial]
records = [record for record in records if record]
assert len(records) == expected, "CPU entry count mismatch"
assert len({record[1] for record in records}) == expected, "duplicate CPU UIDs"
assert len({record[2] for record in records}) == expected, "duplicate APIC IDs"
bsp = [re.fullmatch(r"kernel per-CPU BSP: uid=(\\d+) apic=(\\d+) slots=(\\d+)", line) for line in serial]
bsp = [record for record in bsp if record]
assert len(bsp) == 1, "expected exactly one BSP per-CPU record"
assert int(bsp[0][3]) == expected, "per-CPU slot count mismatch"
assert bsp[0][2] in {record[2] for record in records}, "BSP APIC ID missing from firmware inventory"
assert "VIBRIX: kernel console prompt ready" in debug, "console boot regression"
assert "VIBRIX: kernel timer IRQ delivered" in debug, "timer IRQ regression"
assert not any("kernel panic" in line for line in serial + debug), "unexpected panic"
print(f"CPU enumeration: {expected} distinct firmware CPUs verified; BSP-only execution")
PY
done
