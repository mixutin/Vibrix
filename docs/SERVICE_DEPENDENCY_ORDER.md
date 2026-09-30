# Ordered boot/service dependency policy

The M21 service-order primitive models up to sixteen named services, each with
up to eight dependencies. Names use a strict bounded ASCII grammar. Graph
publication fails on duplicate services, duplicate/self dependencies, missing
dependencies or cycles.

For a valid graph, Vibrix computes a deterministic topological start order using
registration order as the tie-breaker. The output is fixed-capacity and
allocation-free.

This policy does not yet launch, supervise, restart, stop or persistently enable
services. Those behaviors belong to the separate service-manager item. The
purpose here is to make dependency ordering explicit, testable and fail-closed
before service execution exists.
