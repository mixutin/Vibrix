# TCP MSS option negotiation

This M10 slice extends the existing bounded TCP transport with the RFC 9293
maximum-segment-size option on connection setup.

The active client advertises Vibrix's current local TCP payload ceiling in its
SYN. The passive listener does the same in SYN-ACK. Incoming option lists are
walked with explicit bounds: EOL/NOP are understood, malformed lengths,
duplicate MSS options and MSS=0 fail closed. Unknown well-formed options are
skipped by their encoded length.

A peer MSS is capped at Vibrix's own payload limit and becomes the congestion
controller SMSS for the active connection. Data sends are rejected if one
segment would exceed the negotiated peer MSS, the peer receive window, or the
congestion window.

This does not complete the broad TCP roadmap item. Window scaling, SACK,
timestamps, retained retransmission data/live fast retransmit, delayed ACK,
persist/keepalive policy, simultaneous open/close, general receive windows and
TIME-WAIT expiry remain separate.
