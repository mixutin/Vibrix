#!/usr/bin/env python3
"""Validate Vibrix port tracking metadata and optionally compare upstream refs."""

from __future__ import annotations
import argparse
import hashlib
import json
import pathlib
import re
import sys
import tomllib
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parents[1]
PORTS = ROOT / "ports"
SHA40 = re.compile(r"^[0-9a-f]{40}$")

def load_records(root: pathlib.Path) -> list[tuple[pathlib.Path, dict]]:
    if not root.exists():
        return []
    out = []
    for path in sorted(root.glob("*/tracking.toml")):
        with path.open("rb") as handle:
            out.append((path, tomllib.load(handle)))
    return out

def validate(path: pathlib.Path, record: dict) -> list[str]:
    errors: list[str] = []
    required = ("name", "github_repo", "tracked_ref", "pinned_commit", "patches")
    for key in required:
        if key not in record:
            errors.append(f"{path}: missing {key}")
    if errors:
        return errors
    if "/" not in record["github_repo"] or record["github_repo"].count("/") != 1:
        errors.append(f"{path}: github_repo must be owner/repository")
    if not SHA40.fullmatch(record["pinned_commit"]):
        errors.append(f"{path}: pinned_commit must be a lowercase 40-hex commit")
    patches = record["patches"]
    if not isinstance(patches, list):
        errors.append(f"{path}: patches must be an array")
        return errors
    seen: set[str] = set()
    for item in patches:
        if not isinstance(item, dict) or set(item) != {"path", "sha256"}:
            errors.append(f"{path}: each patch requires exactly path and sha256")
            continue
        rel = item["path"]
        if rel in seen:
            errors.append(f"{path}: duplicate patch {rel}")
            continue
        seen.add(rel)
        patch = path.parent / rel
        if not patch.is_file():
            errors.append(f"{path}: missing patch {rel}")
            continue
        digest = hashlib.sha256(patch.read_bytes()).hexdigest()
        if digest != item["sha256"]:
            errors.append(f"{path}: sha256 mismatch for {rel}")
    return errors

def github_head(repo: str, ref: str) -> str:
    url = f"https://api.github.com/repos/{repo}/commits/{ref}"
    request = urllib.request.Request(
        url,
        headers={"Accept": "application/vnd.github+json", "User-Agent": "vibrix-port-tracker"},
    )
    with urllib.request.urlopen(request, timeout=20) as response:
        data = json.load(response)
    sha = data.get("sha", "")
    if not SHA40.fullmatch(sha):
        raise RuntimeError(f"invalid upstream response for {repo}:{ref}")
    return sha

def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=pathlib.Path, default=PORTS)
    parser.add_argument("--network", action="store_true")
    args = parser.parse_args()

    records = load_records(args.root)
    errors: list[str] = []
    stale: list[str] = []
    for path, record in records:
        errors.extend(validate(path, record))
        if args.network and not errors:
            head = github_head(record["github_repo"], record["tracked_ref"])
            if head != record["pinned_commit"]:
                stale.append(
                    f"{record['name']}: {record['pinned_commit']} -> {head} "
                    f"({record['github_repo']} {record['tracked_ref']})"
                )
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 2
    if stale:
        print("upstream movement detected:", file=sys.stderr)
        print("\n".join(stale), file=sys.stderr)
        return 3
    print(f"validated {len(records)} port tracking record(s)")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
