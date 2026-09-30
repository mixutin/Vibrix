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
reassembly, live window scaling/SACK/timestamp negotiation, simultaneous
open/close, persist/keepalive timers and TIME-WAIT expiration remain missing.
The option walker now validates the common negotiation framing for MSS, Window
Scale, SACK-Permitted and Timestamps, rejects malformed or duplicate known
options, clamps Window Scale shifts above 14 as required by RFC 7323, and keeps
unknown well-formed TLVs forward-compatible. Parsing alone does not enable any
of those negotiated behaviors. The current
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

## Live bounded loss recovery

The client now has an optional one-segment retransmission buffer for the current
early transport model. `send_reliable` retains one payload and rejects another
retained send until cumulative acknowledgment advances it. ACK-only input is
validated through the normal TCP parser and port/sequence checks.

Three duplicate ACKs for SND.UNA enter the existing RFC 5681 fast-recovery
policy and reproduce the retained segment without advancing SND.NXT. A partial
cumulative ACK trims the retained prefix transactionally; a full ACK releases
the buffer and deflates the congestion window to ssthresh. Timeout-driven
retransmission reuses the retained unacknowledged suffix and applies the existing
timeout congestion backoff.

The production post-firmware TCP self-test exercises the integrated duplicate
ACK / fast-retransmit / recovery-ACK path. This closes the previous gap where
duplicate-ACK recovery existed only as an isolated congestion-control method.

The broad TCP roadmap item remains open: this is intentionally one retained
segment, not an arbitrary retransmit queue, and delayed ACK, window scaling,
SACK/timestamps, persist/keepalive timers, simultaneous open/close, large-window
reassembly and TIME-WAIT expiry remain unimplemented.

## Bounded common option parsing

The transport parses MSS (kind 2), Window Scale (kind 3), SACK-Permitted
(kind 4) and Timestamp (kind 8) option framing through one bounded option
walker. Duplicate known options, invalid fixed lengths, zero MSS and malformed
TLV boundaries fail closed before segment state is exposed. Unknown well-formed
options remain skippable for forward compatibility.

Window Scale values above 14 are retained as 14 rather than rejected, matching
RFC 7323's receive behavior. This change intentionally does not negotiate or
apply scaled windows, generate/consume SACK blocks, or update RTT state from
timestamps. Those remain transport-state work before the broad TCP roadmap item
can close.

Primary references checked 2026-09-30:
- RFC 7323, TCP Window Scale and Timestamps.
- RFC 2018, TCP SACK-Permitted and SACK options.
