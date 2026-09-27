#!/usr/bin/env bash
# Host-only discovery checks; dummy firmware files are never passed to QEMU.
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
helper="$ROOT/tools/ovmf.inc"
tmp="$(mktemp -d)"
trap 'rm -rf -- "$tmp"' EXIT
mkdir -p "$tmp/debian" "$tmp/arch" "$tmp/empty" "$tmp/custom"

touch "$tmp/debian/OVMF_CODE_4M.fd" "$tmp/debian/OVMF_VARS_4M.fd"
touch "$tmp/debian/OVMF_CODE.fd" "$tmp/debian/OVMF_VARS.fd"
touch "$tmp/arch/OVMF_CODE.4m.fd" "$tmp/arch/OVMF_VARS.4m.fd"
touch "$tmp/custom/code.fd" "$tmp/custom/vars.fd"

(
  unset OVMF_CODE OVMF_VARS
  export OVMF_SEARCH_DIRS="$tmp/debian"
  source "$helper"
  vibrix_find_ovmf
  [[ "$OVMF_CODE" == "$tmp/debian/OVMF_CODE_4M.fd" ]]
  [[ "$OVMF_VARS" == "$tmp/debian/OVMF_VARS_4M.fd" ]]
)

(
  unset OVMF_CODE OVMF_VARS
  export OVMF_SEARCH_DIRS="$tmp/arch"
  source "$helper"
  vibrix_find_ovmf
  [[ "$OVMF_CODE" == "$tmp/arch/OVMF_CODE.4m.fd" ]]
  [[ "$OVMF_VARS" == "$tmp/arch/OVMF_VARS.4m.fd" ]]
)

(
  export OVMF_SEARCH_DIRS="$tmp/empty"
  export OVMF_CODE="$tmp/custom/code.fd"
  export OVMF_VARS="$tmp/custom/vars.fd"
  source "$helper"
  vibrix_find_ovmf
  [[ "$OVMF_CODE" == "$tmp/custom/code.fd" ]]
  [[ "$OVMF_VARS" == "$tmp/custom/vars.fd" ]]
)

if (
  unset OVMF_CODE OVMF_VARS
  export OVMF_SEARCH_DIRS="$tmp/empty"
  source "$helper"
  vibrix_find_ovmf
) >"$tmp/missing.log" 2>&1; then
  echo "missing CODE unexpectedly succeeded" >&2
  exit 1
fi
grep -Fq 'readable OVMF firmware not found' "$tmp/missing.log"

if (
  export OVMF_CODE="$tmp/custom/missing.fd"
  export OVMF_VARS="$tmp/custom/vars.fd"
  source "$helper"
  vibrix_find_ovmf
) >"$tmp/bad-code.log" 2>&1; then
  echo "invalid explicit CODE unexpectedly succeeded" >&2
  exit 1
fi
grep -Fq 'OVMF_CODE is not a readable file' "$tmp/bad-code.log"

if (
  export OVMF_CODE="$tmp/custom/code.fd"
  unset OVMF_VARS
  source "$helper"
  vibrix_find_ovmf
) >"$tmp/unknown-pair.log" 2>&1; then
  echo "unrecognized CODE unexpectedly inferred VARS" >&2
  exit 1
fi
grep -Fq 'set OVMF_VARS explicitly' "$tmp/unknown-pair.log"

rm "$tmp/debian/OVMF_VARS_4M.fd"
if (
  unset OVMF_CODE OVMF_VARS
  export OVMF_SEARCH_DIRS="$tmp/debian"
  source "$helper"
  vibrix_find_ovmf
) >"$tmp/mismatch.log" 2>&1; then
  echo "mismatched firmware-size pair unexpectedly succeeded" >&2
  exit 1
fi
grep -Fq 'OVMF_VARS_4M.fd not found' "$tmp/mismatch.log"

echo "[vibrix] OVMF discovery regression tests passed"
