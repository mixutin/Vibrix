#!/usr/bin/env python3
"""Fail-closed Vibrix USB image provisioner for Linux hosts."""

from __future__ import annotations

import argparse
import fcntl
import hashlib
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile

BLKGETSIZE64 = 0x80081272
CHUNK = 1024 * 1024


class Refusal(RuntimeError):
    pass


def sha256_file(path: Path, limit: int | None = None) -> str:
    digest = hashlib.sha256()
    remaining = limit
    with path.open("rb", buffering=0) as handle:
        while True:
            size = CHUNK if remaining is None else min(CHUNK, remaining)
            if size == 0:
                break
            chunk = handle.read(size)
            if not chunk:
                break
            digest.update(chunk)
            if remaining is not None:
                remaining -= len(chunk)
    if remaining not in (None, 0):
        raise Refusal("short read while verifying destination")
    return digest.hexdigest()


def canonical_usb_target(target: Path, sys_class_block: Path = Path("/sys/class/block")) -> tuple[Path, str]:
    raw = str(target)
    if not raw.startswith("/dev/disk/by-id/usb-"):
        raise Refusal("target must be an explicit /dev/disk/by-id/usb-* whole-disk path")
    if not target.is_symlink():
        raise Refusal("USB by-id target must be a symlink")
    resolved = target.resolve(strict=True)
    name = resolved.name
    sys_entry = sys_class_block / name
    if not sys_entry.exists():
        raise Refusal("resolved target is not present in /sys/class/block")
    if (sys_entry / "partition").exists():
        raise Refusal("refusing a partition; select the whole removable USB disk")
    removable = (sys_entry / "removable").read_text().strip()
    if removable != "1":
        raise Refusal("kernel does not mark this whole disk removable")
    mode = resolved.stat().st_mode
    if not stat.S_ISBLK(mode):
        raise Refusal("resolved USB target is not a block device")
    return resolved, name


def mounted_device_numbers(mountinfo: Path = Path("/proc/self/mountinfo")) -> set[tuple[int, int]]:
    mounted: set[tuple[int, int]] = set()
    for line in mountinfo.read_text(errors="replace").splitlines():
        fields = line.split()
        if len(fields) < 3 or ":" not in fields[2]:
            continue
        major, minor = fields[2].split(":", 1)
        try:
            mounted.add((int(major), int(minor)))
        except ValueError:
            continue
    return mounted


def device_tree_numbers(name: str, sys_class_block: Path = Path("/sys/class/block")) -> set[tuple[int, int]]:
    result: set[tuple[int, int]] = set()
    root = sys_class_block / name

    def add(entry: Path) -> None:
        dev = (entry / "dev").read_text().strip()
        major, minor = dev.split(":", 1)
        result.add((int(major), int(minor)))

    add(root)
    for entry in sys_class_block.iterdir():
        partition = entry / "partition"
        if not partition.exists():
            continue
        try:
            if entry.resolve().parent == root.resolve():
                add(entry)
        except FileNotFoundError:
            continue
    return result


def refuse_if_mounted(name: str, sys_class_block: Path = Path("/sys/class/block"), mountinfo: Path = Path("/proc/self/mountinfo")) -> None:
    if device_tree_numbers(name, sys_class_block) & mounted_device_numbers(mountinfo):
        raise Refusal("target disk or one of its partitions is mounted")


def validate_vibrix_image(image: Path, sector_size: int) -> None:
    if sector_size not in (512, 4096):
        raise Refusal("sector size must be 512 or 4096")
    repo = Path(__file__).resolve().parent.parent
    inspector_source = repo / "tools" / "inspect-gpt.rs"
    if not inspector_source.is_file():
        raise Refusal("repository GPT inspector source is missing")
    with tempfile.TemporaryDirectory(prefix="vibrix-inspect-") as tmp:
        inspector = Path(tmp) / "inspect-gpt"
        try:
            build = subprocess.run(
                ["rustc", "--edition=2024", str(inspector_source), "-O", "-o", str(inspector)],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                check=False,
            )
        except FileNotFoundError as error:
            raise Refusal("rustc is required to validate the Vibrix image") from error
        if build.returncode != 0:
            raise Refusal("failed to build the repository GPT inspector")
        checked = subprocess.run(
            [str(inspector), str(image), str(sector_size)],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False,
        )
        if checked.returncode != 0:
            detail = checked.stderr.strip() or "GPT inspector rejected the image"
            raise Refusal(detail)


def device_size(handle) -> int:
    buf = bytearray(8)
    fcntl.ioctl(handle.fileno(), BLKGETSIZE64, buf, True)
    return int.from_bytes(buf, "little")


def copy_image(image: Path, target: Path, image_size: int) -> None:
    flags = os.O_WRONLY | os.O_SYNC
    fd = os.open(target, flags)
    try:
        with image.open("rb", buffering=0) as source, os.fdopen(fd, "wb", buffering=0, closefd=False) as destination:
            remaining = image_size
            while remaining:
                chunk = source.read(min(CHUNK, remaining))
                if not chunk:
                    raise Refusal("source image ended unexpectedly")
                destination.write(chunk)
                remaining -= len(chunk)
            os.fsync(destination.fileno())
    finally:
        os.close(fd)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Safely write a verified Vibrix image to one removable USB disk.")
    parser.add_argument("image", type=Path)
    parser.add_argument("target", type=Path)
    parser.add_argument(
        "--confirm",
        metavar="CANONICAL_DEVICE",
        help="must exactly equal the resolved whole-disk device, for example /dev/sdb",
    )
    parser.add_argument("--sector-size", type=int, choices=(512, 4096), default=512)
    parser.add_argument("--dry-run", action="store_true", help="perform every safety check without writing")
    args = parser.parse_args(argv)

    try:
        image = args.image.resolve(strict=True)
        if not image.is_file() or image.is_symlink():
            raise Refusal("image must be a regular file")
        image_size = image.stat().st_size
        if image_size == 0:
            raise Refusal("image is empty")
        validate_vibrix_image(image, args.sector_size)

        target, name = canonical_usb_target(args.target)
        refuse_if_mounted(name)

        with target.open("rb", buffering=0) as handle:
            target_size = device_size(handle)
        if target_size < image_size:
            raise Refusal("target device is smaller than the image")

        print(f"image:  {image} ({image_size} bytes)")
        print(f"target: {target} ({target_size} bytes)")
        print(f"source sha256: {sha256_file(image)}")

        if args.dry_run:
            print("dry-run: safety checks passed; nothing written")
            return 0
        if args.confirm != str(target):
            raise Refusal(f"destructive confirmation required: --confirm {target}")

        copy_image(image, target, image_size)
        with target.open("rb", buffering=0) as handle:
            os.fsync(handle.fileno())
        source_hash = sha256_file(image)
        target_hash = sha256_file(target, image_size)
        if source_hash != target_hash:
            raise Refusal("read-back SHA-256 mismatch after write")
        print(f"verified sha256: {target_hash}")
        print("provisioning complete")
        return 0
    except (OSError, Refusal) as error:
        print(f"refusing to provision: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
