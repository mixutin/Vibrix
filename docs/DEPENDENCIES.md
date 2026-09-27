# Dependency policy

Vibrix permits community Rust crates from crates.io, Git repositories and
appropriately licensed vendored sources in the bootloader, kernel, drivers,
system libraries, user applications, installer, provisioning tools and host
development/test tooling. This replaces the former blanket prohibition.

## Before adding or upgrading a crate

- State the concrete use case, chosen crate and version/revision in the PR.
  Prefer a maintained, scoped dependency over a large unused framework.
- Check the source and its license, including bundled assets and transitive
  dependencies. The dependency's license remains its own; compatibility with
  distribution of Vibrix matters. Do not assume that all Rust crates are 0BSD.
- Review maintenance/provenance, security advisories, unsafe code and
  build scripts or native dependencies as appropriate to the trust boundary.
- Inspect the resolved dependency graph and enabled feature set. Prefer
  explicit minimal features instead of accidentally enabling `std`, a host OS,
  a C runtime, dynamic loading or an allocator unavailable to the component.
- Demonstrate builds and relevant tests against each affected target. The
  UEFI loader uses `x86_64-unknown-uefi`; the kernel uses
  `x86_64-unknown-none` and `no_std`. A crate working on the Linux host
  alone does **not** establish compatibility with either target.
- Commit the updated `Cargo.lock` (this repository builds applications and
  an operating system, not a generic reusable library), and keep version and
  feature changes reviewable. CI should use `--locked` when dependencies are
  being resolved.
- When vendoring, retain the upstream license and attribution, document the
  exact source/revision and make local changes distinguishable.

Crates may implement parsing, UEFI services, allocation, bitflags, networking,
filesystems and other common facilities. Vibrix still owns and validates the
UEFI-to-kernel contract, safety invariants, persistent USB-only root model and
higher-level system architecture. A crate does not prove a roadmap feature
works: require the behavior to be demonstrated on the roadmap's target.

## Independence boundary

Using a library through its published Rust interface under its license is
allowed. Do not copy or lightly translate Linux/BSD/GNU/third-party OS
implementation source into first-party Vibrix code or replace the Vibrix
kernel with another operating system. Tiny documented assembly remains
permitted for hardware operations where Rust alone is insufficient.

Host tools such as QEMU and OVMF are development infrastructure rather than
shipped Vibrix code.
