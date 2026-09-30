# Package dependency graph inspection

The canonical `vibrix-package` database exposes allocation-free inspection of
both direct dependency and direct reverse-dependency edges.

- `direct_dependencies(name)` returns the manifest's declared dependency slice.
- `direct_dependents(name, output)` enumerates installed packages that directly
  require the named package into a caller-owned fixed-capacity buffer.
- Unknown package names fail explicitly instead of looking like empty nodes.
- Enumeration order is deterministic installed-database order.

The native package metadata probe executes the same APIs in Vibrix Ring 3 after
installing a dependency pair. This is graph inspection over the current bounded
package database; it does not claim persistent package state, a `vpm` command,
repository metadata, transitive closure, or dependency solving.
