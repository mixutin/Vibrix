# Privilege-separated base-service sandbox pattern

Vibrix base services should not remain root-equivalent after startup. The
bounded service-sandbox helper composes existing M11/M23 primitives into one
trusted launch sequence:

1. reduce the process promise mask;
2. install a component-aware filesystem allow-list;
3. enable no-new-privileges;
4. drop real/effective/saved credentials to a dedicated service identity.

The current profiles cover the resolver, NTP client and future SSH service.
Children inherit the already-reduced credentials, promise mask, path policy and
no-new-privileges state.

Network scope is recorded as part of the service profile (`LoopbackOnly`,
`ClientOnly`, or `Listener`) so the future socket layer has an explicit
policy input. The current kernel has no general network-socket enforcement hook,
so this change does **not** claim the M22 per-service network sandbox checkbox.

Failure is fail-closed: path/promise/no-new-privilege changes are monotonic, and
the credential drop happens last. An error can therefore leave a process with
less authority, never more authority.

This is the bounded base-system privilege-separation pattern for M23. It does
not provide chroot roots, network namespaces, device mediation, a complete SSH
daemon, or authenticated multi-user service supervision.
