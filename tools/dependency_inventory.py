#!/usr/bin/env python3
"""Report the resolved Cargo graph; never build or execute dependency code."""

import argparse
import json
import re
from pathlib import Path
from urllib.parse import parse_qs, urlsplit

SHA = re.compile(r"[0-9a-fA-F]{40}\Z")


def validate_git_source(source: str) -> None:
    if not source.startswith("git+"):
        return
    url = urlsplit(source[4:])
    query = parse_qs(url.query, keep_blank_values=True)
    revisions = query.get("rev", [])
    if (
        len(revisions) != 1
        or not SHA.fullmatch(revisions[0])
        or not SHA.fullmatch(url.fragment)
        or revisions[0].lower() != url.fragment.lower()
        or "branch" in query
        or "tag" in query
    ):
        raise ValueError("Git dependency needs a full, matching 40-character rev and locked commit: " + source)


def inventory(metadata: dict) -> dict:
    packages = metadata.get("packages")
    resolve = metadata.get("resolve")
    members = metadata.get("workspace_members")
    if not isinstance(packages, list) or not isinstance(resolve, dict) or not isinstance(members, list):
        raise ValueError("Expected complete cargo metadata --locked --all-features --format-version 1")
    nodes = {node["id"]: node for node in resolve["nodes"]}
    result = []
    for package in sorted(packages, key=lambda p: (p["name"], p["version"], p["id"])):
        source = package.get("source")
        if source is not None:
            validate_git_source(source)
        node = nodes.get(package["id"], {})
        kinds = {kind for target in package.get("targets", []) for kind in target.get("kind", [])}
        result.append({
            "id": package["id"],
            "name": package["name"],
            "version": package["version"],
            "workspace_member": package["id"] in members,
            "source": source or "workspace-or-path (inspect provenance for vendored code)",
            "license": package.get("license"),
            "license_file": package.get("license_file"),
            "repository": package.get("repository"),
            "enabled_features": sorted(node.get("features", [])),
            "dependencies": sorted(node.get("dependencies", [])),
            "build_script": "custom-build" in kinds,
            "proc_macro": "proc-macro" in kinds,
            "native_links": package.get("links"),
        })
    return {
        "schema_version": 1,
        "scope": "Cargo all-features graph; not an installed-system SBOM or malware audit",
        "package_count": len(result),
        "external_package_count": sum(not p["workspace_member"] for p in result),
        "packages": result,
    }


def check_workflow_pins(text: str) -> list[str]:
    """Lint block-style uses entries. actionlint separately validates the YAML."""
    errors = []
    for line_number, line in enumerate(text.splitlines(), 1):
        match = re.match(r"^\s*(?:-\s+)?[\"']?uses[\"']?\s*:\s*(.*?)\s*$", line)
        if not match:
            continue
        value = match.group(1).split(" #", 1)[0].strip().strip("\"'")
        if value.startswith("./") and "${{" not in value:
            continue
        if value.startswith("docker://"):
            valid = re.fullmatch(r"docker://[^\s@]+@sha256:[0-9a-f]{64}", value)
        else:
            valid = re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_./-]+@[0-9a-fA-F]{40}", value)
        if not valid:
            errors.append(f"line {line_number}: external uses must have an immutable full SHA: {value}")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--metadata", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--workflows", type=Path)
    args = parser.parse_args()
    if not args.metadata and not args.workflows:
        parser.error("provide --metadata or --workflows")
    try:
        if args.metadata:
            report = inventory(json.loads(args.metadata.read_text(encoding="utf-8")))
            encoded = json.dumps(report, indent=2) + "\n"
            if args.output:
                args.output.parent.mkdir(parents=True, exist_ok=True)
                args.output.write_text(encoded, encoding="utf-8")
            else:
                print(encoded, end="")
        if args.workflows:
            paths = sorted([*args.workflows.glob("*.yml"), *args.workflows.glob("*.yaml")])
            if not paths:
                raise ValueError("No workflows found")
            errors = [f"{path}: {error}" for path in paths
                      for error in check_workflow_pins(path.read_text(encoding="utf-8"))]
            if errors:
                raise ValueError("\n".join(errors))
            print(f"Immutable action-reference lint passed for {len(paths)} workflows")
    except (OSError, ValueError, KeyError, TypeError) as error:
        print(f"Dependency policy failed: {error}")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
