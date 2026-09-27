#!/usr/bin/env bash
set -euo pipefail

fail=0

while IFS=: read -r file line text; do
  case "$file" in
    kernel/src/*|boot/src/*|shared/*) ;;
    *) continue ;;
  esac

  case "$text" in
    *"unsafe {"*|*"unsafe fn "*|*"unsafe impl "*|*"asm!("*|*"global_asm!("*)
      start=$(( line > 8 ? line - 8 : 1 ))
      if ! sed -n "${start},${line}p" "$file" | grep -Eq 'SAFETY:|Safety invariant|safety invariant'; then
        echo "::error file=$file,line=$line::unsafe boundary lacks a nearby SAFETY/invariant explanation"
        fail=1
      fi
      ;;
  esac
done < <(grep -RInE 'unsafe[[:space:]]*(\{|fn|impl)|(^|[^[:alnum:]_])(asm|global_asm)!\(' kernel/src boot/src shared 2>/dev/null || true)

if (( fail )); then
  echo "Document each low-level unsafe boundary with a nearby SAFETY: comment or safety invariant."
  exit 1
fi

echo "Unsafe-boundary documentation check passed."
