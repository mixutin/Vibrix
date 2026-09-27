#!/usr/bin/env bash
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT"

if ! command -v rustup >/dev/null 2>&1; then
  echo "rustup is required. Install it from https://rustup.rs and restart your shell." >&2
  exit 1
fi

echo "[vibrix] syncing pinned Rust toolchain"
rustup show active-toolchain >/dev/null

if ! rustup target list --installed | grep -qx 'x86_64-unknown-uefi'; then
  echo "[vibrix] installing Rust target x86_64-unknown-uefi"
  rustup target add x86_64-unknown-uefi
fi

echo "[vibrix] building UEFI loader"
cargo build -p vibrix-boot --target x86_64-unknown-uefi

OUT="$ROOT/build/qemu"
ESP="$OUT/esp"
rm -rf "$OUT"
mkdir -p "$ESP/EFI/BOOT"
cp target/x86_64-unknown-uefi/debug/vibrix-boot.efi "$ESP/EFI/BOOT/BOOTX64.EFI"

echo "[vibrix] EFI tree ready:"
echo "$ESP/EFI/BOOT/BOOTX64.EFI"
