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

echo "[vibrix] building kernel"
cargo -Z build-std=core,compiler_builtins   -Z build-std-features=compiler-builtins-mem   rustc -p vibrix-kernel   --target kernel/x86_64-vibrix.json   --   -C link-arg=-Tkernel/linker.ld

OUT="$ROOT/build/qemu"
ESP="$OUT/esp"
rm -rf "$OUT"
mkdir -p "$ESP/EFI/BOOT" "$ESP/vibrix"

cp target/x86_64-unknown-uefi/debug/vibrix-boot.efi "$ESP/EFI/BOOT/BOOTX64.EFI"

KERNEL="$(find target/x86_64-vibrix/debug -maxdepth 1 -type f -name 'vibrix-kernel' | head -n1)"
if [[ -z "$KERNEL" ]]; then
  echo "[vibrix] kernel artifact not found" >&2
  exit 1
fi

cp "$KERNEL" "$ESP/vibrix/kernel.elf"

echo "[vibrix] EFI loader: $ESP/EFI/BOOT/BOOTX64.EFI"
echo "[vibrix] kernel ELF: $ESP/vibrix/kernel.elf"
