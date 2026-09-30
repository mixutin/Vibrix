# Loopback and Unix-domain sockets

This M22 slice provides two allocation-free local communication primitives:

- a packet-boundary-preserving loopback queue for packets that must remain on
  the local host; and
- a fixed-capacity Unix-domain datagram registry with pathname binding,
  source-address preservation, bounded per-socket queues and fail-closed
  destination/source validation.

Unix-domain names are absolute byte paths with a fixed maximum size. A sender
must itself be bound; a destination must exist. Datagram delivery is atomic
with respect to queue capacity: a full destination returns an error without
discarding an older datagram. Receive with an undersized buffer also preserves
the pending datagram.

The production kernel self-test exercises bidirectional Unix datagrams and
loopback packet delivery after firmware exit.

This is not a POSIX socket ABI. Stream Unix sockets, descriptor integration,
poll/select, credentials passing, filesystem inode socket nodes, blocking
waiters, cross-process syscall exposure and networking namespace integration
remain separate work.
