# Process operation promises

Vibrix exposes an additive ABI-v1 `Promises` syscall for monotonic reduction of
ambient process operations. This is an independently designed capability
reduction mechanism informed by the general least-privilege goal of interfaces
such as OpenBSD pledge, but it does not copy pledge's promise vocabulary or
implementation.

## Current promise classes

The shared ABI defines four independent bits:

- `PROMISE_IO`: read, write and close existing descriptors;
- `PROMISE_FILESYSTEM`: open/create/mkdir/remove/read-directory operations;
- `PROMISE_PROCESS`: wait, exec, process inspection and kill policy;
- `PROMISE_CREDENTIALS`: real/effective/saved UID/GID queries and transitions.

Exit, yield, getpid, no-new-privileges and the promise syscall itself remain
available so a restricted process can terminate, cooperate with scheduling,
identify itself, tighten privilege policy further, and inspect/reduce its own
promise mask.

A new process begins with `PROMISE_ALL`. Child process-table entries inherit
the exact current mask of their parent. The kernel accepts only masks contained
in `PROMISE_ALL`, and a process may only replace its mask with a subset of the
existing mask. Attempts to add a previously removed bit fail with permission
denied and do not mutate state.

## Enforcement boundary

Enforcement occurs before the process-facing syscall dispatcher constructs an
action, so a denied operation cannot reach pointer-copy, VFS, credential or
process mutation handling through that path.

The current desktop display/input fast path is dispatched by the architecture
layer before the generic process dispatcher and is therefore outside this first
promise vocabulary. Network, device, namespace and descriptor-specific rights
also remain future work.

This slice is not a sandbox by itself. It provides one monotonic primitive that
later service profiles and application sandboxes can compose with path
visibility, descriptor rights, namespaces and resource limits.
