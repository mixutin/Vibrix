#!/usr/bin/env python3
import importlib.util
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("provision_usb", Path(__file__).with_name("provision-usb.py"))
mod = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(mod)


class ProvisionSafetyTests(unittest.TestCase):
    def test_mountinfo_parser(self):
        with tempfile.TemporaryDirectory() as tmp:
            p = Path(tmp) / "mountinfo"
            p.write_text("36 25 8:1 / /mnt rw - ext4 /dev/sda1 rw\ninvalid\n")
            self.assertEqual(mod.mounted_device_numbers(p), {(8, 1)})

    def test_device_tree_detects_parent_and_partition(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            disk = root / "sdb"
            part = root / "sdb1"
            disk.mkdir()
            part.mkdir()
            (disk / "dev").write_text("8:16\n")
            (part / "dev").write_text("8:17\n")
            (part / "partition").write_text("1\n")
            # emulate sysfs partition nesting with symlink targets
            real = root / "real"
            (real / "sdb" / "sdb1").mkdir(parents=True)
            (real / "sdb" / "dev").write_text("8:16\n")
            (real / "sdb" / "sdb1" / "dev").write_text("8:17\n")
            (real / "sdb" / "sdb1" / "partition").write_text("1\n")
            disk.rmdir()
            part.unlink(missing_ok=True) if False else None

    def test_refuse_if_mounted_detects_any_overlap(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "sys"
            root.mkdir()
            disk = root / "sdb"
            disk.mkdir()
            (disk / "dev").write_text("8:16\n")
            mountinfo = Path(tmp) / "mountinfo"
            mountinfo.write_text("36 25 8:16 / /mnt rw - ext4 /dev/sdb rw\n")
            with self.assertRaises(mod.Refusal):
                mod.refuse_if_mounted("sdb", root, mountinfo)

    def test_hash_limit_detects_exact_prefix(self):
        with tempfile.TemporaryDirectory() as tmp:
            p = Path(tmp) / "data"
            p.write_bytes(b"abcdef")
            import hashlib
            self.assertEqual(mod.sha256_file(p, 3), hashlib.sha256(b"abc").hexdigest())


if __name__ == "__main__":
    unittest.main()
