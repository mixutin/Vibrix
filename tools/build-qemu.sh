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

for target in x86_64-unknown-uefi x86_64-unknown-none; do
  if ! rustup target list --installed | grep -qx "$target"; then
    echo "[vibrix] installing Rust target $target"
    rustup target add "$target"
  fi
done

echo "[vibrix] building UEFI loader"
cargo build --locked -p vibrix-boot --features qemu-debugcon --target x86_64-unknown-uefi

echo "[vibrix] building Rust init userspace ELF"
cargo rustc --locked -p vibrix-init --bin vibrix-init --target x86_64-unknown-none -- \
  -C debuginfo=0 \
  -C relocation-model=static \
  -C link-arg=-no-pie \
  -C link-arg=-Tuserspace/linker.ld

echo "[vibrix] building Rust shell userspace ELF"
cargo rustc --locked -p vibrix-shell --bin vibrix-shell --target x86_64-unknown-none -- \
  -C debuginfo=0 \
  -C relocation-model=static \
  -C link-arg=-no-pie \
  -C link-arg=-Tuserspace/linker.ld

echo "[vibrix] building kernel"
cargo rustc --locked -p vibrix-kernel --bin vibrix-kernel --features "${VIBRIX_KERNEL_FEATURES:-qemu-debugcon}" --target x86_64-unknown-none -- -C code-model=kernel -C no-redzone=yes -C relocation-model=static -C link-arg=-no-pie -C link-arg=-Tkernel/linker.ld

OUT="$ROOT/build/qemu"
ESP="$OUT/esp"
rm -rf "$OUT"
mkdir -p "$ESP/EFI/BOOT" "$ESP/vibrix"

cp target/x86_64-unknown-uefi/debug/vibrix-boot.efi "$ESP/EFI/BOOT/BOOTX64.EFI"

KERNEL="$ROOT/target/x86_64-unknown-none/debug/vibrix-kernel"
if [[ ! -f "$KERNEL" ]]; then
  echo "[vibrix] kernel artifact not found: $KERNEL" >&2
  exit 1
fi

cp "$KERNEL" "$ESP/vibrix/kernel.elf"

echo "[vibrix] EFI loader: $ESP/EFI/BOOT/BOOTX64.EFI"
echo "[vibrix] kernel ELF: $ESP/vibrix/kernel.elf"
