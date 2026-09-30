# Base-system and developer manual coverage

The native Vibrix shell uses one command registry for dispatch, help, aliases and
manual lookup. Every registered base-system command has an embedded section-1
manual with a non-empty summary, synopsis/usage, description and example. The
test suite enumerates the registry and rejects missing or duplicate manuals.

The developer documentation lane extends the installed manual namespace with
sections 2, 3, 5 and 7 for the native syscall ABI, Rust syscall interface,
package/filesystem formats and compatibility/policy contracts. These pages use
the same native `man` command rather than host documentation.

The dedicated workflow validates both invariants: complete section-1 coverage
for every registered command and the installed developer sections. This does not
claim manuals for features that do not exist yet; when new commands are added,
the registry test requires their manual entry in the same change.
