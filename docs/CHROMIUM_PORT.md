# Chromium on Vibrix: native port gates

Author: **GPT-6 Astra Pro**. Research date: 29 September 2026.

**Status: Chromium is not built for, installed on, or running inside Vibrix.** The [native desktop preview](DESKTOP.md) provides a real userspace display/input boundary and a terminal. Its F4 view is an explicit status panel, not a browser. This document is a port plan and gap analysis, not a successful build report or an Ozone implementation.

## What the desktop does and does not unblock

Chromium's [Ozone overview](https://chromium.googlesource.com/chromium/src/+/main/docs/ozone_overview.md) describes low-level graphics/input integration through interfaces including PlatformWindow and SurfaceFactoryOzone. A future Vibrix backend could present software-rendered surfaces and translate native input through these interfaces. Ozone does **not** supply the OS process, memory, threading, file or networking runtime. Naming a backend or drawing a browser icon cannot replace those services.

The current image embeds one bounded Rust userspace ELF, runs one foreground desktop session, and exposes a tiny RAM filesystem. Existing kernel-thread/managed-VM/process probes are important foundations, but they do not establish arbitrary user-thread creation, native Chromium processes, shared user mappings or a complete C/C++ platform runtime. Kernel NIC/packet milestones likewise do not establish a browser-facing TCP socket, resolver or TLS service.

Upstream's [threading documentation](https://chromium.googlesource.com/chromium/src/+/main/docs/threading_and_tasks.md) describes multiple processes, OS-provided threads, task queues and I/O threads. [Mojo](https://chromium.googlesource.com/chromium/src/+/main/mojo/README.md) supplies cross-process communication abstractions that still need functioning platform primitives. Vibrix's native implementations must be proven before a Chromium integration can rely on them.

## An explicit application/runtime policy decision

The [independence policy](INDEPENDENCE.md) currently admits Rust runtime libraries and excludes third-party libc and GNU/BSD userspace. Chromium's supported Linux build uses Clang and libc++, per its [upstream build instructions](https://chromium.googlesource.com/chromium/src/+/main/docs/linux/build_instructions.md). A native browser therefore needs a reviewed, narrowly scoped external-application/toolchain policy and native C ABI support, not a silent import of a Linux distribution or system libc. This change does not weaken the policy or import those components.

The native-port approach would retain the Vibrix-owned kernel, boot chain, drivers and removable root, then implement the required platform runtime and maintain an upstream-based browser port. A broad Linux ABI layer could reduce some application-port work, but is a different, much larger compatibility commitment; it is not implemented here. Running host Chromium through VNC would only demonstrate remote display and is not accepted as Chromium running on Vibrix. No hidden guest or remote-browser substitute is used.

## Ordered acceptance gates

| Gate | Work still required | Evidence required before calling it complete |
| --- | --- | --- |
| 1. Native application substrate | User process spawn/exec, scheduling, threads, per-thread TLS, synchronization, signals or equivalent platform contracts | Two independent real user processes/threads; cross-process isolation/fault tests; deterministic wake/join/exit behavior |
| 2. Memory and IPC | Dynamic user mappings/protection, shared memory, large allocation, file mapping, handles and asynchronous message transport | Native memory/IPC tests, W^X and invalid-handle rejection, cleanup after process termination, stress and resource-limit evidence |
| 3. Application runtime | Native C ABI, compiler support/runtime, C++ library decisions, clocks, entropy and filesystem semantics | A pinned cross-toolchain/sysroot and small C/C++ native test programs; no accidental Linux imports or unsupported host libraries |
| 4. Networking and durable profile | User TCP/UDP sockets, DNS, secure randomness, TLS certificate validation, persistent profile storage and file operations | Native socket/resolver tests; controlled HTTPS server with valid and invalid certificate cases; profile survives a supported removable-root reboot |
| 5. Browser platform build | A pinned upstream Chromium revision, GN platform/toolchain configuration, base/Mojo platform adaptation and software Ozone surface/input backend | Reproducible actual-target build, dependency/license inventory, runnable native content_shell with offline HTML/JS and keyboard/mouse input |
| 6. Browser integration and confinement | Browser UI/process topology, font/text support, crash handling, renderer sandbox/permissions and update packaging | Actual Chromium build identity inside Vibrix, isolated renderer tests, multiple independent pages, native HTTPS navigation and crash/restart evidence |

Work can proceed in parallel after its contracts are defined. An offline development content_shell may use an explicitly ephemeral profile; durable storage and online security remain separate release gates, not excuses to postpone an offline rendering experiment. A small HTML renderer written for this desktop would also not be Chromium and must never be labelled as such.

[Chromium's sandbox design](https://chromium.googlesource.com/chromium/src/+/main/docs/design/sandbox.md) depends on operating-system enforcement. The current single bootstrap desktop has no browser-grade multi-process capability boundary. An unsandboxed local test page could be a clearly marked development experiment after a real port exists; it must not become an internet-facing release recipe. This plan does not recommend disabling sandboxing as a substitute for implementing the boundary.

## Reproducibility, resources and supply chain

Before downloading a browser checkout, select and record an exact upstream commit plus matching build-tool revisions, design the native sysroot, and establish adequate host resources. The current upstream Linux-host instructions list at least 8 GB RAM (more than 16 recommended), 100 GB free space and Python 3.9+, but these are **host build requirements**, not a statement that a Linux-built executable runs on Vibrix. No Chromium checkout, binary or successful build is claimed in this patch.

Chromium would add a substantial non-Cargo dependency graph. Inventory all selected bundled libraries/assets, licenses/notices, host build hooks and downloaded compiler tools; record exact revisions and permitted features, and add their own update/advisory coverage. Existing cargo-audit/cargo-deny do not automatically scan that graph. No broad license/source/advisory bypass is acceptable. Software rendering is a useful initial target; GPU acceleration, audio/video and hardware codec support are independent follow-on work, not prerequisites for claiming a narrowly demonstrated offline page.

## What a real success report must include

Publish the exact Vibrix and Chromium source commits, reproducible native build commands, boot artifact hashes, native process/build identity, screenshots captured from the guest framebuffer, and page/input test logs. Show a controlled HTML/JavaScript fixture first, then an HTTPS fixture with certificate validation, independently tracked renderer processes and confinement tests. Clearly separate offline bring-up, online browsing, persistence and physical hardware. A status panel, screenshot of a host browser, synthetic process table or green compile-only job satisfies none of these runtime gates.

The upstream links above were inspected as moving `main` documentation on 2026-09-29; they are research references, not an adopted immutable browser dependency. Detailed platform/shared-memory and Linux-specific sandbox source URLs were unavailable in this review, so no claim is made that their implementation has been audited. Revisit the exact selected upstream revision before implementing those contracts. The next concrete native milestone is user process/thread execution plus checked user memory/IPC, not an untestable browser launcher.
