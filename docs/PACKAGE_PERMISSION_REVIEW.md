# Package permission review before installation

Vibrix package capability declarations are requests, not grants. The package
database now exposes a separate `install_reviewed` path that requires an
explicit reviewer-approved capability set before installation.

The review is fail-closed:

- the capability declaration must name the same package as the manifest;
- every requested capability bit must be included in the approved set;
- package/dependency validation still runs after the review gate;
- a denied or mismatched review does not mutate the package database.

The existing `Capabilities` type rejects unknown authority bits during decode,
so an older reviewer cannot silently approve a future capability it does not
understand.

This is a bounded installation-policy primitive. It does not provide a graphical
consent UI, persistent user decisions, signatures, repository trust, sandbox
enforcement, per-user policy, or a complete package manager.
