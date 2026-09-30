#!/usr/bin/env python3
from __future__ import annotations
import hashlib
import pathlib
import subprocess
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
TOOL = ROOT / "tools" / "check-port-upstreams.py"

def run(root: pathlib.Path):
    return subprocess.run(
        ["python3", str(TOOL), "--root", str(root)],
        text=True, capture_output=True, check=False,
    )

with tempfile.TemporaryDirectory() as tmp:
    root = pathlib.Path(tmp)
    port = root / "demo"
    port.mkdir()
    patch = port / "0001-demo.patch"
    patch.write_text("demo patch\n", encoding="utf-8")
    digest = hashlib.sha256(patch.read_bytes()).hexdigest()
    (port / "tracking.toml").write_text(
        'name = "demo"\n'
        'github_repo = "example/demo"\n'
        'tracked_ref = "main"\n'
        'pinned_commit = "' + ("a" * 40) + '"\n'
        'patches = [{ path = "0001-demo.patch", sha256 = "' + digest + '" }]\n',
        encoding="utf-8",
    )
    ok = run(root)
    assert ok.returncode == 0, ok.stderr
    patch.write_text("tampered\n", encoding="utf-8")
    bad = run(root)
    assert bad.returncode == 2 and "sha256 mismatch" in bad.stderr

with tempfile.TemporaryDirectory() as tmp:
    empty = pathlib.Path(tmp)
    ok = run(empty)
    assert ok.returncode == 0
print("port upstream tracking tests passed")
