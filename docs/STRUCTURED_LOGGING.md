# Structured kernel logging

M18 starts with a bounded, allocation-free structured event buffer in the kernel. It is intentionally smaller than the eventual persistent userspace journal.

Each record contains:

- a monotonic boot-local sequence number;
- a severity level;
- a subsystem identifier;
- a stable numeric event code;
- two 64-bit event values.

The production buffer holds 128 publish-once records. Writers atomically reserve a unique slot, initialize the record, then publish it with Release ordering. Readers require an Acquire load before copying the record. Published slots are never mutated or reused during the boot. Capacity exhaustion is returned as an explicit error; old records are never silently overwritten.\n\nA single packed atomic filter controls the minimum accepted severity and an eight-bit subsystem mask. Filter updates publish atomically, so readers never observe a torn level/mask pair. Filtered events are explicitly reported to the caller and do not consume log capacity.

This design follows Rust's documented rule that UnsafeCell does not itself make concurrent access safe. Synchronization is provided by unique atomic slot reservation plus Release/Acquire publication. The design deliberately avoids a spin lock so a future interrupt-context producer cannot deadlock by interrupting a lock holder.

## Current boundary

This milestone establishes **structured kernel logging** plus the kernel-side **log level and subsystem filtering** policy. It does not yet provide:

- persistent userspace journal storage;
- wall-clock or monotonic timestamps in records;
- log filtering or a vlog interface;
- rotation/retention policy;
- previous-boot logs;
- persisted crash records;
- SMP stress validation or per-CPU log shards.

The buffer is suitable for the current kernel and its future callers, but later M18 work may extend the record schema through a versioned interface instead of reinterpreting existing event codes.
