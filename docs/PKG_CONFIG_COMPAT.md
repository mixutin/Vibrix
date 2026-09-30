# Bounded pkg-config compatibility layer

Vibrix M24 needs enough build-tool compatibility for ports to consume common
`.pc` metadata without importing an external pkg-config implementation.

The dependency-free parser in `userspace/pkgconfig/src/lib.rs` implements the
portable subset needed by early ports:

- variable assignments such as `prefix=/usr`;
- `${name}` variable expansion without shell execution;
- `Name`, `Description`, `Version`, `Requires`, `Libs`, and `Cflags`;
- dotted numeric minimum-version checks;
- caller-provided output buffers and fixed-capacity metadata tables.

Malformed records, duplicate names, undefined variables, oversized output and
invalid versions fail closed. The parser does not execute command substitution,
shell syntax, environment expansion, filesystem search paths or arbitrary
directives.

This is the metadata/query core for a future native `pkg-config` command and
ports build integration. It does not claim Autoconf/CMake compatibility,
dynamic linking, installed shared libraries, dependency graph resolution, or
the ability to build arbitrary third-party software yet.
