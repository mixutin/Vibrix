# Vibrix package metadata v1

VPKG v1 is the bounded metadata/database foundation for Vibrix packages. It is
implemented by the first-party `vibrix-package` no_std crate.

## Scope

Each manifest is exactly 512 bytes and contains:

- magic/version `VPKGv001`;
- a validated package name (1..32 lowercase ASCII letters, digits, `-`, `_`,
  or `.`);
- numeric `MAJOR.MINOR.PATCH` version fields;
- payload byte length;
- a 32-byte payload digest field;
- up to eight dependencies, each naming one package and a minimum numeric
  version.

All unused bytes are zero and decoders reject non-zero reserved storage. This
makes one canonical encoding and gives future signatures a deterministic
metadata object to authenticate.

The digest field is metadata only in this milestone. No hash algorithm,
signature scheme, repository trust root or archive payload layout is selected
by VPKG v1 yet, so callers must not treat a non-zero digest as verified
integrity.

## Dependency semantics

VPKG v1 deliberately supports only **minimum-version** dependencies. A version
`A.B.C` satisfies a minimum when its numeric tuple is greater than or equal to
that tuple. Package installation requires every declared dependency to already
exist at a sufficient version.

The fixed-capacity database stores at most 32 manifests. Install validation is
transactional: duplicate names, missing dependencies, insufficient versions or
capacity exhaustion leave the database unchanged. Removal is refused while an
installed package directly depends on the requested package.

This is intentionally smaller than Cargo's rich version-requirement language.
Semantic Versioning 2.0.0 and Cargo's dependency/version documentation were
reviewed for terminology and alternatives, but the VPKG wire format and
resolver policy are original Vibrix contracts.

## Deliberate boundary

This milestone does **not** implement:

- package archives or payload extraction;
- filesystem installation/removal;
- a `vpm` command;
- signed repositories or trust roots;
- dependency fetching;
- dependency backtracking/alternative versions;
- upgrade transactions;
- package provenance/license UI;
- persistent package database storage.

Those remain separate M14/M16/M17 tasks.

## Primary references checked 2026-09-29

- Semantic Versioning 2.0.0: https://semver.org/
- Cargo manifest format:
  https://doc.rust-lang.org/cargo/reference/manifest.html
- Cargo dependency requirements:
  https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html

Cargo was considered as a mature example of package identity and version
requirements. Vibrix does not copy Cargo's manifest syntax, resolver, registry
protocol or implementation.
