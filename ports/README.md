# Vibrix ports/build recipes

This directory contains declarative, reviewable build recipes for software that
is built for the Vibrix userspace target. The initial recipes deliberately use
in-tree crates so the format and validation can be proven without depending on
network downloads or pretending third-party ports already work.

Each `.toml` recipe has a fixed schema:

- `name`: lowercase package/port name.
- `version`: dotted numeric version.
- `kind`: one of `application` or `library`.
- `source`: currently `workspace`.
- `path`: repository-relative crate directory.
- `target`: currently `x86_64-unknown-none`.
- `features`: explicit Cargo feature list, possibly empty.
- `build`: currently `cargo`.
- `install`: logical install destination used by later packaging work.
- `license`: SPDX identifier expected from the crate/workspace policy.

The validator rejects unknown keys, absolute or parent-traversing paths,
nonexistent crate manifests, duplicate names, malformed versions, unsupported
targets/build systems, and inappropriate install destinations.

This is the M14 build-recipe foundation. It does not download upstream source,
apply third-party patches, install payloads, sign repositories, or claim that
portable Unix software already builds on Vibrix.
