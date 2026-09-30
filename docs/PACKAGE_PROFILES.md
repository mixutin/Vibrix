# Transactional package profile foundation

M17 profiles are bounded groups of packages applied to the existing VPKG
database. The database now exposes atomic profile install/remove operations.

- profile transactions contain at most 8 package entries;
- installation is evaluated on a private database copy in dependency order;
- removal is evaluated on a private copy in dependent-first order;
- any missing dependency, duplicate package, capacity failure, missing package,
  or reverse-dependency conflict leaves the live database unchanged;
- only a completely successful trial state replaces the live database.

This is the transactional database primitive for future Minimal, Developer,
Server, Recovery and optional Security-lab profiles. It does not yet define the
membership of those profiles, persist package state, fetch payloads, verify
repository signatures, or modify the filesystem.
