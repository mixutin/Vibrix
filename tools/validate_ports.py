#!/usr/bin/env python3
from __future__ import annotations

import pathlib
import re
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parents[1]
PORTS = ROOT / "ports"
ALLOWED_KEYS = {
    "name", "version", "kind", "source", "path", "target",
    "features", "build", "install", "license",
}
NAME = re.compile(r"^[a-z0-9][a-z0-9._-]*$")
VERSION = re.compile(r"^[0-9]+(?:\.[0-9]+){1,3}$")
INSTALL = re.compile(r"^/(?:bin|lib)/[A-Za-z0-9._+-]+$")

def fail(path: pathlib.Path, message: str) -> None:
    raise SystemExit(f"{path.relative_to(ROOT)}: {message}")

def validate(path: pathlib.Path, seen: set[str]) -> None:
    with path.open("rb") as handle:
        data = tomllib.load(handle)

    unknown = set(data) - ALLOWED_KEYS
    missing = ALLOWED_KEYS - set(data)
    if unknown:
        fail(path, f"unknown keys: {sorted(unknown)}")
    if missing:
        fail(path, f"missing keys: {sorted(missing)}")

    name = data["name"]
    if not isinstance(name, str) or not NAME.fullmatch(name):
        fail(path, "invalid name")
    if name in seen:
        fail(path, f"duplicate name {name}")
    seen.add(name)

    version = data["version"]
    if not isinstance(version, str) or not VERSION.fullmatch(version):
        fail(path, "version must be dotted numeric")

    if data["kind"] not in {"application", "library"}:
        fail(path, "unsupported kind")
    if data["source"] != "workspace":
        fail(path, "only workspace sources are currently permitted")
    if data["target"] != "x86_64-unknown-none":
        fail(path, "unsupported target")
    if data["build"] != "cargo":
        fail(path, "unsupported build system")
    if data["license"] != "0BSD":
        fail(path, "license must match current workspace policy")
    if not isinstance(data["features"], list) or not all(
        isinstance(feature, str) and feature for feature in data["features"]
    ):
        fail(path, "features must be a list of non-empty strings")

    rel = pathlib.PurePosixPath(data["path"])
    if rel.is_absolute() or ".." in rel.parts or "." in rel.parts:
        fail(path, "path must be a normalized repository-relative path")
    crate = ROOT.joinpath(*rel.parts)
    manifest = crate / "Cargo.toml"
    if not manifest.is_file():
        fail(path, "path does not contain Cargo.toml")

    install = data["install"]
    if not isinstance(install, str) or not INSTALL.fullmatch(install):
        fail(path, "install must name one /bin or /lib artifact")
    if data["kind"] == "application" and not install.startswith("/bin/"):
        fail(path, "applications must install under /bin")
    if data["kind"] == "library" and not install.startswith("/lib/"):
        fail(path, "libraries must install under /lib")

def main() -> int:
    recipes = sorted(PORTS.glob("*.toml"))
    if len(recipes) < 3:
        raise SystemExit("ports: expected at least three validated recipes")
    seen: set[str] = set()
    for recipe in recipes:
        validate(recipe, seen)
    print(f"validated {len(recipes)} Vibrix build recipes")
    return 0

if __name__ == "__main__":
    sys.exit(main())
