# TCP transport foundation

Checked against RFC 9293 (STD 7) on 2026-09-29.

This module is a bounded active-open TCP client foundation. It implements the
20-byte base TCP header, the IPv4 pseudo-header checksum, modulo-32-bit sequence
comparisons, SYN/SYN-ACK/ACK connection establishment, cumulative ACK tracking,
strict in-order payload delivery, and an active FIN close through TIME-WAIT.

SYN and FIN consume sequence space. Received payload advances RCV.NXT only after
the caller provides enough buffer capacity and all sequence/checksum/port
validation succeeds. Acknowledgments outside SND.UNA..SND.NXT fail closed. The
production self-test deliberately crosses the 32-bit sequence-number wrap,
sends and acknowledges data in both directions, and performs active close.

## Deliberate boundary

This is not yet a generally interoperable full TCP implementation. It has no
retransmission/RTO engine, congestion control, delayed ACK policy, out-of-order
reassembly, window scaling, MSS/SACK/timestamp options, passive LISTEN,
simultaneous open/close, persist/keepalive timers or TIME-WAIT expiration.
Those pieces must exist before the broad ROADMAP `TCP` checkbox can claim a
complete network transport service.

Primary source:

- RFC 9293 — Transmission Control Protocol (TCP), Internet Standard STD 7.
