# No-new-privileges process flag

Vibrix exposes an additive ABI v1 process-control syscall for a monotonic
`no_new_privileges` flag.

A new process starts with the flag clear unless its parent already set it.
Writing value 1 sets the flag permanently for the process; children inherit the
set state. Value 0 queries the flag. Other values fail closed. There is no ABI
operation that clears the flag.

The flag is the process-state prerequisite for future set-ID execution,
capability transitions and sandbox policy. Those future paths must refuse any
transition that would increase authority when this bit is set.

This slice does not claim set-ID executable semantics, a capability system,
exec-time privilege transitions or a complete sandbox. It establishes the
monotonic process flag and real userspace/kernel ABI needed by those later
features.
