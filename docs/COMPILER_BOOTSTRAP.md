# Vibrix Rust compiler bootstrap plan

This document completes the M14 planning milestone only. It does not claim a
native compiler or self-hosted build exists yet.

## Goal

Reach a point where Vibrix can run a Rust toolchain from its removable USB
system, compile a Vibrix userspace program, and later rebuild progressively more
of the operating system without depending on another installed OS.

## Current starting point

Today the repository pins a nightly Rust toolchain and cross-compiles the
kernel and userspace for bare-metal x86-64. That is a target-only workflow: the
compiler itself still runs on an external supported host.

Rust distinguishes target support from a platform that can run host tools such
as rustc and Cargo. Vibrix therefore needs a real hosted Rust target before a
native compiler can be treated as supported rather than as an ad-hoc bare-metal
binary.

## Bootstrap stages

### Stage A — stabilize the Vibrix userspace ABI

Prerequisites before asking rustc to run natively:

1. process creation/exit/wait and argv/environment;
2. files, directories and file descriptors through VFS;
3. virtual memory suitable for compiler workloads;
4. clocks, randomness and robust error reporting;
5. enough POSIX-compatible runtime surface for Rust host tools;
6. a native linker path and object/archive handling;
7. persistent package/toolchain storage on the USB root.

Define a dedicated hosted triple, provisionally `x86_64-unknown-vibrix`,
instead of pretending `x86_64-unknown-none` is a hosted OS target.

### Stage B — external cross-built native toolchain

From a trusted tier-1 Rust host:

1. pin an exact upstream Rust source commit and its stage0 metadata;
2. add the Vibrix hosted target definition and target libraries;
3. build a rustc/Cargo toolchain whose **host** is Vibrix;
4. package the resulting binaries, sysroot, source metadata and licenses as a
   signed Vibrix package;
5. transfer it only onto the removable Vibrix USB system partition/root;
6. verify package signatures and hashes before activation.

This imported toolchain becomes Vibrix's first native stage0. It is a bootstrap
artifact, not proof of self-hosting.

### Stage C — native program compilation

On Vibrix:

1. run `rustc --version --verbose` and `cargo --version`;
2. compile a minimal no_std userspace program;
3. compile a normal Vibrix userspace crate using the native syscall/runtime
   libraries;
4. execute the produced ELF through the normal process loader;
5. repeat after reboot from the same USB.

Passing this stage completes the first M15 native-compile milestone.

### Stage D — native Rust source bootstrap

Use the previously verified Vibrix toolchain as stage0 and an exact vendored or
signed Rust source tree.

Follow upstream bootstrap staging:

- stage0: previously built compiler and standard libraries;
- stage1: current compiler sources built by stage0;
- stage2: current compiler rebuilt using the in-tree compiler and libraries;
- optional stage3 comparison/testing when practical.

The source revision, bootstrap configuration, Cargo dependency graph and
toolchain package hashes must be recorded in the build manifest.

Initially Vibrix may reuse a separately packaged prebuilt codegen backend and
native linker to avoid making a full LLVM/C++ source build a prerequisite for
the first Rust self-hosting proof. That is an explicit transitional trust
dependency and must be displayed by the package manager. Full from-source
toolchain production belongs to later M20 work.

### Stage E — expand the self-hosting boundary

In order:

1. build ordinary userspace packages;
2. build all first-party userspace;
3. build the Vibrix kernel;
4. build bootloader and USB image tooling;
5. build signed packages and update metadata;
6. produce and verify a complete bootable USB release from Vibrix.

Every stage must retain the USB-only system-target rule.

## Reproducibility and provenance

Every native toolchain package must record:

- upstream Rust commit and release/channel;
- stage0 compiler identity and hashes;
- target specification revision;
- enabled bootstrap options;
- Cargo.lock / dependency inventory;
- codegen backend and linker identity;
- source archive hashes and signatures;
- build machine architecture and Vibrix release;
- resulting rustc, Cargo and sysroot hashes.

Offline rebuild inputs must be cacheable on the Vibrix USB. A repository mirror
or package mirror may supply bytes but may not bypass signature/hash
verification.

## Target strategy

Prefer an upstreamable built-in hosted target once the ABI is stable. Custom
target JSON is acceptable only during bring-up because rustc documents custom
target properties as unstable and compiler-version-sensitive.

Do not silently overload the current built-in bare-metal
`x86_64-unknown-none` target with hosted semantics.

## Failure policy

Bootstrap stops on:

- mismatched source/toolchain hashes;
- unsigned or invalid package metadata;
- unsupported target specification revision;
- missing runtime ABI capability;
- linker/codegen backend mismatch;
- stage output that cannot execute the required native compile test.

Do not fall back to an internal disk, host-mounted filesystem or another OS to
make a native bootstrap appear successful.

## Evidence required for later milestones

This plan alone completes only **M14 — compiler bootstrap plan**.

M15/M20 checkboxes require real execution evidence on Vibrix:

- native rustc/Cargo starts;
- a program is compiled;
- that exact output runs under Vibrix;
- later stages rebuild userspace/kernel/release artifacts;
- the final release is produced and verified from the removable USB system.

## Primary references checked 2026-09-29

- Rust Compiler Development Guide, bootstrapping stages:
  https://rustc-dev-guide.rust-lang.org/building/bootstrapping/what-bootstrapping-does.html
- Rust Compiler Development Guide, building a compiler:
  https://rustc-dev-guide.rust-lang.org/building/how-to-build-and-run
- rustc platform support and host-tools distinction:
  https://doc.rust-lang.org/rustc/platform-support.html
- rustc custom targets:
  https://doc.rust-lang.org/rustc/targets/custom.html
- Cargo target configuration:
  https://doc.rust-lang.org/cargo/reference/config.html
