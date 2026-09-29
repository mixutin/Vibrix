# TCP transport foundation

Checked against RFC 9293 (STD 7) on 2026-09-29.

This module is a bounded TCP transport foundation. It implements the 20-byte
base TCP header, the IPv4 pseudo-header checksum, modulo-32-bit sequence
comparisons, active-open and passive-open SYN/SYN-ACK/ACK establishment,
cumulative ACK tracking, strict in-order payload delivery, a four-slot bounded
out-of-order receive reassembly queue for segments up to 256 bytes, an active
FIN close through TIME-WAIT, a bounded retransmission/RTO policy with
exponential backoff and a hard retry limit, and a conservative RFC 5681 sender
congestion-control policy.

SYN and FIN consume sequence space. Received payload advances RCV.NXT only after
the caller provides enough buffer capacity and all sequence/checksum/port
validation succeeds. Acknowledgments outside SND.UNA..SND.NXT fail closed. The
production self-test deliberately crosses the 32-bit sequence-number wrap,
sends and acknowledges data in both directions, and performs active close.

## Deliberate boundary

This is not yet a generally interoperable full TCP implementation. The RTO
object is a transport policy primitive: callers still own segment retention,
clock calibration and actual retransmission I/O. The passive-open path proves a
strict one-connection base-header handshake, not a backlog or accept queue.
Full loss-recovery integration, delayed ACK policy, general/large-window receive
reassembly, window scaling, MSS/SACK/timestamp options, simultaneous open/close,
persist/keepalive timers and TIME-WAIT expiration remain missing. The current
reassembly queue deliberately rejects overlap, stale sequence space, oversized
segments and capacity exhaustion rather than silently dropping or overwriting
queued bytes. Those pieces must exist before the broad ROADMAP `TCP` checkbox
can claim a complete network transport service.

Primary source:

- RFC 9293 — Transmission Control Protocol (TCP), Internet Standard STD 7.

## Congestion-control boundary

The sender now gates new payload by the minimum of the peer advertised window
and a congestion window, accounts only newly cumulatively acknowledged bytes,
uses slow start followed by byte-counted congestion avoidance, reduces to one
SMSS after retransmission timeout, and provides the RFC 5681 three-duplicate-ACK
fast-recovery state transition. The initial window is deliberately conservative
at one SMSS, below RFC 5681's allowed upper bound.

This is a policy primitive integrated with the current client send/ACK path. It
does not yet retain/retransmit arbitrary outstanding segments, detect duplicate
ACKs from live transport input, estimate RTT, implement SACK/ECN, or provide a
production timer scheduler. Those remain required before broad TCP completion.

Additional primary source checked 2026-09-29:

- RFC 5681 — TCP Congestion Control: https://www.rfc-editor.org/rfc/rfc5681.html
