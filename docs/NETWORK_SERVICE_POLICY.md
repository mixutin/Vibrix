# Network service default-off policy

M22 requires network services to remain disabled unless the administrator
explicitly enables them.

The kernel library owns a small deny-by-default startup policy. A fresh
`Policy` has SSH, resolver and NTP service slots disabled. Enabling one service
does not affect any other service, and disablement is reversible. Future service
manager/daemon startup code must query `may_start()` before opening a listening
socket or beginning network service activity.

The normal kernel subsystem self-test exercises the same production policy and
emits `VIBRIX: kernel network service default-off policy verified` only after
all services are observed disabled, one exact service is enabled without
cross-enabling the others, and it is disabled again.

This is a bounded control-plane guarantee. It does not claim that SSH, NTP, a
caching resolver, a service manager or listening sockets exist yet. When those
components land, their startup path must be wired through this gate and receive
its own runtime evidence.
