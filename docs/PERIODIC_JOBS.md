# Periodic job scheduler

The M21 periodic scheduler is allocation-free and driven by a caller-supplied
monotonic tick. Up to sixteen named jobs can be registered with non-zero fixed
intervals.

When time advances, each due job is reported at most once for that observation,
even if multiple periods were missed. Its next deadline is advanced to the first
future period so a delayed caller does not create an unbounded catch-up burst.
Registration order provides deterministic delivery order. Duplicate jobs,
zero intervals, backward clocks, capacity overflow and deadline arithmetic
overflow fail explicitly; deadline overflow is calculated transactionally before
publishing scheduler state.

This is the scheduling core only. Persistent schedule configuration, calendar
time/cron syntax, user identity, command execution and service-manager
supervision are separate integration layers.
