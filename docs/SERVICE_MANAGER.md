# Service manager state machine

The bounded M21 service manager registers up to sixteen named services and owns
their enable/disable and runtime state.

Supported operations are enable, disable, start, stop, reload and status.
Starting requires an enabled, stopped service. Reload requires a running service.
Disabling a running service changes future boot policy but deliberately does not
pretend the process stopped. Each successful start/reload publishes a monotonic
generation so stale supervision observations can be distinguished.

This is the control-state core. It does not yet spawn or supervise userspace
processes, persist enablement, apply dependency ordering, restart crashed
services or expose the manager through an administrator command. Those are
separate integration layers.
