# Vibrix package core

This document defines the first bounded M14 package-format/database/dependency
foundation. It is deliberately smaller than the future `vpm` package manager.

## Package metadata

The `vibrix-package-core` crate is `no_std` and dependency-free. It parses a
bounded VPK1 metadata record containing:

- magic `VPK1` and format version 1;
- a lowercase ASCII package name of at most 31 bytes;
- a semantic version triple;
- declared payload byte length;
- a 32-byte SHA-256 payload digest field;
- up to eight dependencies, each with a package name and minimum version.

The parser rejects truncation, unsupported versions, malformed names, duplicate
dependencies, dependency overflow and trailing bytes. The digest is metadata
only: this slice does not implement hashing, signatures or repository trust.

## Installed-package database

The core exposes a caller-sized fixed-capacity database. Entries contain name,
version and payload digest. Duplicate package names and capacity overflow fail
explicitly. Dependency evaluation requires every declared dependency to be
installed at or above the requested minimum version and can return a compact
missing/outdated dependency mask.

The database is currently in memory. It does not yet persist to VibrixFS and
does not implement transactions, file ownership, uninstall, upgrades, reverse
dependencies or garbage collection.

## Completion boundary

This PR advances **M14 package format/database/dependencies**, but should not
check the roadmap item until exact-head CI succeeds and a later integration
step records that evidence. It does not complete the package manager, signed
repositories, persistent package database, profile management or update system.

The wire format is intentionally versioned so later signed repository metadata
and package archives can evolve without silently reinterpreting v1 records.
