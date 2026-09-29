//! Bounded TCP client transport foundation.
//!
//! Implements active-open and passive-open handshakes, in-order data transfer,
//! cumulative acknowledgments, active close, and a bounded retransmission/RTO
//! policy. Congestion control, receive reassembly and TCP options remain
//! deliberately outside this module.

use super::ipv4;

pub const PROTOCOL: u8 = 6;
pub const HEADER_BYTES: usize = 20;
pub const MAX_PAYLOAD: usize = ipv4::MAX_PAYLOAD - HEADER_BYTES;

const FLAG_FIN: u16 = 0x001;
const FLAG_SYN: u16 = 0x002;
const FLAG_RST: u16 = 0x004;
const FLAG_PSH: u16 = 0x008;
const FLAG_ACK: u16 = 0x010;
const DATA_OFFSET_5: u16 = 5 << 12;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Length,
    Header,
    Checksum,
    Port,
    Sequence,
    Acknowledgment,
    Reset,
    State,
    OutputTooSmall,
    ReceiveTooSmall,
    RetransmissionExhausted,
    Invariant,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum State {
    Closed,
    SynSent,
    Established,
    FinWait1,
    FinWait2,
    TimeWait,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Segment<'a> {
    source_port: u16,
    destination_port: u16,
    sequence: u32,
    acknowledgment: u32,
    flags: u16,
    window: u16,
    payload: &'a [u8],
}

impl<'a> Segment<'a> {
    pub const fn source_port(&self) -> u16 {
        self.source_port
    }

    pub const fn destination_port(&self) -> u16 {
        self.destination_port
    }

    pub const fn sequence(&self) -> u32 {
        self.sequence
    }

    pub const fn acknowledgment(&self) -> u32 {
        self.acknowledgment
    }

    pub const fn syn(&self) -> bool {
        self.flags & FLAG_SYN != 0
    }

    pub const fn ack(&self) -> bool {
        self.flags & FLAG_ACK != 0
    }

    pub const fn fin(&self) -> bool {
        self.flags & FLAG_FIN != 0
    }

    pub const fn rst(&self) -> bool {
        self.flags & FLAG_RST != 0
    }

    pub const fn payload(&self) -> &'a [u8] {
        self.payload
    }

    pub const fn window(&self) -> u16 {
        self.window
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Endpoint {
    pub address: [u8; 4],
    pub port: u16,
}

pub const DEFAULT_SMSS: u32 = 536;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CongestionControl {
    smss: u32,
    cwnd: u32,
    ssthresh: u32,
    avoidance_acked: u32,
}

impl CongestionControl {
    pub fn new(smss: u32) -> Result<Self, Error> {
        if smss == 0 || smss > MAX_PAYLOAD as u32 {
            return Err(Error::Length);
        }
        Ok(Self {
            smss,
            // Deliberately conservative RFC 5681 start: one SMSS is below the
            // allowed initial-window upper bound and avoids an initial burst.
            cwnd: smss,
            ssthresh: u32::MAX,
            avoidance_acked: 0,
        })
    }

    pub const fn cwnd(&self) -> u32 {
        self.cwnd
    }

    pub const fn ssthresh(&self) -> u32 {
        self.ssthresh
    }

    pub fn send_allowance(&self, peer_window: u16, flight_size: u32) -> u32 {
        u32::from(peer_window)
            .min(self.cwnd)
            .saturating_sub(flight_size)
    }

    pub fn on_new_ack(&mut self, newly_acked: u32) {
        if newly_acked == 0 {
            return;
        }
        if self.cwnd < self.ssthresh {
            self.cwnd = self
                .cwnd
                .saturating_add(newly_acked.min(self.smss));
            return;
        }

        self.avoidance_acked = self.avoidance_acked.saturating_add(newly_acked);
        if self.avoidance_acked >= self.cwnd {
            self.avoidance_acked -= self.cwnd;
            self.cwnd = self.cwnd.saturating_add(self.smss);
        }
    }

    pub fn on_retransmission_timeout(&mut self, flight_size: u32) {
        self.ssthresh = (flight_size / 2).max(self.smss.saturating_mul(2));
        self.cwnd = self.smss;
        self.avoidance_acked = 0;
    }

    pub fn on_three_duplicate_acks(&mut self, flight_size: u32) {
        self.ssthresh = (flight_size / 2).max(self.smss.saturating_mul(2));
        self.cwnd = self
            .ssthresh
            .saturating_add(self.smss.saturating_mul(3));
        self.avoidance_acked = 0;
    }

    pub fn on_recovery_ack(&mut self) {
        self.cwnd = self.ssthresh.max(self.smss);
        self.avoidance_acked = 0;
    }
}

pub const INITIAL_RTO_TICKS: u64 = 1_000;
pub const MAX_RTO_TICKS: u64 = 60_000;
pub const MAX_RETRANSMISSIONS: u8 = 5;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimerAction {
    Waiting,
    Retransmit,
    Exhausted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetransmissionTimer {
    deadline: u64,
    rto_ticks: u64,
    retransmissions: u8,
    armed: bool,
}

impl RetransmissionTimer {
    pub const fn new() -> Self {
        Self {
            deadline: 0,
            rto_ticks: INITIAL_RTO_TICKS,
            retransmissions: 0,
            armed: false,
        }
    }

    pub fn arm(&mut self, now_ticks: u64) {
        self.deadline = now_ticks.saturating_add(self.rto_ticks);
        self.armed = true;
    }

    pub fn acknowledge(&mut self) {
        self.deadline = 0;
        self.rto_ticks = INITIAL_RTO_TICKS;
        self.retransmissions = 0;
        self.armed = false;
    }

    pub const fn retransmissions(&self) -> u8 {
        self.retransmissions
    }

    pub const fn rto_ticks(&self) -> u64 {
        self.rto_ticks
    }

    pub fn poll(&mut self, now_ticks: u64) -> TimerAction {
        if !self.armed || now_ticks < self.deadline {
            return TimerAction::Waiting;
        }
        if self.retransmissions >= MAX_RETRANSMISSIONS {
            self.armed = false;
            return TimerAction::Exhausted;
        }
        self.retransmissions += 1;
        self.rto_ticks = self.rto_ticks.saturating_mul(2).min(MAX_RTO_TICKS);
        self.deadline = now_ticks.saturating_add(self.rto_ticks);
        TimerAction::Retransmit
    }
}

impl Default for RetransmissionTimer {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Listener {
    local: Endpoint,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PendingPassive {
    local: Endpoint,
    remote: Endpoint,
    snd_nxt: u32,
    rcv_nxt: u32,
    peer_window: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PassiveConnection {
    local: Endpoint,
    remote: Endpoint,
    snd_nxt: u32,
    rcv_nxt: u32,
    peer_window: u16,
}

impl Listener {
    pub const fn new(local: Endpoint) -> Self {
        Self { local }
    }

    pub fn accept_syn(
        &self,
        remote_address: [u8; 4],
        input: &[u8],
        initial_sequence: u32,
        output: &mut [u8],
    ) -> Result<(PendingPassive, usize), Error> {
        if self.local.port == 0 {
            return Err(Error::State);
        }
        let segment = parse(remote_address, self.local.address, input)?;
        if segment.destination_port() != self.local.port || segment.source_port() == 0 {
            return Err(Error::Port);
        }
        if !segment.syn()
            || segment.ack()
            || segment.fin()
            || segment.rst()
            || !segment.payload().is_empty()
        {
            return Err(Error::Header);
        }
        let remote = Endpoint {
            address: remote_address,
            port: segment.source_port(),
        };
        let rcv_nxt = segment.sequence().wrapping_add(1);
        let snd_nxt = initial_sequence.wrapping_add(1);
        let len = encode(
            self.local.address,
            remote.address,
            self.local.port,
            remote.port,
            initial_sequence,
            rcv_nxt,
            FLAG_SYN | FLAG_ACK,
            u16::MAX,
            &[],
            output,
        )?;
        Ok((
            PendingPassive {
                local: self.local,
                remote,
                snd_nxt,
                rcv_nxt,
                peer_window: segment.window(),
            },
            len,
        ))
    }
}

impl PendingPassive {
    pub fn accept_ack(self, input: &[u8]) -> Result<PassiveConnection, Error> {
        let segment = parse(self.remote.address, self.local.address, input)?;
        if segment.source_port() != self.remote.port
            || segment.destination_port() != self.local.port
        {
            return Err(Error::Port);
        }
        if segment.rst() {
            return Err(Error::Reset);
        }
        if !segment.ack() || segment.syn() || segment.fin() || !segment.payload().is_empty() {
            return Err(Error::Header);
        }
        if segment.sequence() != self.rcv_nxt {
            return Err(Error::Sequence);
        }
        if segment.acknowledgment() != self.snd_nxt {
            return Err(Error::Acknowledgment);
        }
        Ok(PassiveConnection {
            local: self.local,
            remote: self.remote,
            snd_nxt: self.snd_nxt,
            rcv_nxt: self.rcv_nxt,
            peer_window: segment.window(),
        })
    }
}

impl PassiveConnection {
    pub const fn local(&self) -> Endpoint {
        self.local
    }

    pub const fn remote(&self) -> Endpoint {
        self.remote
    }

    pub const fn send_next(&self) -> u32 {
        self.snd_nxt
    }

    pub const fn receive_next(&self) -> u32 {
        self.rcv_nxt
    }

    pub const fn peer_window(&self) -> u16 {
        self.peer_window
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Client {
    local: Endpoint,
    remote: Endpoint,
    state: State,
    iss: u32,
    snd_una: u32,
    snd_nxt: u32,
    rcv_nxt: u32,
    peer_window: u16,
    congestion: CongestionControl,
}

impl Client {
    pub const fn new(local: Endpoint, remote: Endpoint, initial_sequence: u32) -> Self {
        Self {
            local,
            remote,
            state: State::Closed,
            iss: initial_sequence,
            snd_una: initial_sequence,
            snd_nxt: initial_sequence,
            rcv_nxt: 0,
            peer_window: 0,
            congestion: CongestionControl {
                smss: DEFAULT_SMSS,
                cwnd: DEFAULT_SMSS,
                ssthresh: u32::MAX,
                avoidance_acked: 0,
            },
        }
    }

    pub const fn state(&self) -> State {
        self.state
    }

    pub const fn send_next(&self) -> u32 {
        self.snd_nxt
    }

    pub const fn receive_next(&self) -> u32 {
        self.rcv_nxt
    }

    pub const fn congestion_window(&self) -> u32 {
        self.congestion.cwnd()
    }

    pub const fn slow_start_threshold(&self) -> u32 {
        self.congestion.ssthresh()
    }

    pub fn retransmission_timeout(&mut self) {
        let flight = self.snd_nxt.wrapping_sub(self.snd_una);
        self.congestion.on_retransmission_timeout(flight);
    }

    pub fn connect(&mut self, output: &mut [u8]) -> Result<usize, Error> {
        if self.state != State::Closed || self.local.port == 0 || self.remote.port == 0 {
            return Err(Error::State);
        }
        let len = encode(
            self.local.address,
            self.remote.address,
            self.local.port,
            self.remote.port,
            self.iss,
            0,
            FLAG_SYN,
            u16::MAX,
            &[],
            output,
        )?;
        self.snd_nxt = self.iss.wrapping_add(1);
        self.state = State::SynSent;
        Ok(len)
    }

    pub fn accept_syn_ack(&mut self, input: &[u8], output: &mut [u8]) -> Result<usize, Error> {
        if self.state != State::SynSent {
            return Err(Error::State);
        }
        let segment = parse(self.remote.address, self.local.address, input)?;
        self.validate_ports(&segment)?;
        if segment.rst() {
            return Err(Error::Reset);
        }
        if !segment.syn() || !segment.ack() || !segment.payload().is_empty() {
            return Err(Error::Header);
        }
        if segment.acknowledgment() != self.snd_nxt {
            return Err(Error::Acknowledgment);
        }

        self.snd_una = segment.acknowledgment();
        self.rcv_nxt = segment.sequence().wrapping_add(1);
        self.peer_window = segment.window();
        self.state = State::Established;
        self.ack_segment(output)
    }

    pub fn send(&mut self, payload: &[u8], output: &mut [u8]) -> Result<usize, Error> {
        if self.state != State::Established {
            return Err(Error::State);
        }
        let flight = self.snd_nxt.wrapping_sub(self.snd_una);
        let allowance = self.congestion.send_allowance(self.peer_window, flight);
        if payload.len() > MAX_PAYLOAD || payload.len() as u32 > allowance {
            return Err(Error::Length);
        }
        let len = encode(
            self.local.address,
            self.remote.address,
            self.local.port,
            self.remote.port,
            self.snd_nxt,
            self.rcv_nxt,
            FLAG_ACK | FLAG_PSH,
            u16::MAX,
            payload,
            output,
        )?;
        self.snd_nxt = self.snd_nxt.wrapping_add(payload.len() as u32);
        Ok(len)
    }

    /// Accept exactly in-order payload and return a cumulative ACK.
    ///
    /// The payload is copied only after all validation succeeds. Duplicate,
    /// out-of-order and over-capacity segments leave the caller's buffer
    /// unchanged.
    pub fn receive(
        &mut self,
        input: &[u8],
        receive: &mut [u8],
        ack_output: &mut [u8],
    ) -> Result<(usize, usize), Error> {
        if self.state != State::Established {
            return Err(Error::State);
        }
        let segment = parse(self.remote.address, self.local.address, input)?;
        self.validate_ports(&segment)?;
        if segment.rst() {
            return Err(Error::Reset);
        }
        if !segment.ack() || segment.syn() || segment.fin() {
            return Err(Error::Header);
        }
        self.accept_ack(segment.acknowledgment())?;
        if segment.sequence() != self.rcv_nxt {
            return Err(Error::Sequence);
        }
        if receive.len() < segment.payload().len() {
            return Err(Error::ReceiveTooSmall);
        }

        receive[..segment.payload().len()].copy_from_slice(segment.payload());
        self.rcv_nxt = self.rcv_nxt.wrapping_add(segment.payload().len() as u32);
        self.peer_window = segment.window();
        let ack_len = self.ack_segment(ack_output)?;
        Ok((segment.payload().len(), ack_len))
    }

    pub fn accept_ack_only(&mut self, input: &[u8]) -> Result<(), Error> {
        if !matches!(
            self.state,
            State::Established | State::FinWait1 | State::FinWait2
        ) {
            return Err(Error::State);
        }
        let segment = parse(self.remote.address, self.local.address, input)?;
        self.validate_ports(&segment)?;
        if segment.rst() {
            return Err(Error::Reset);
        }
        if !segment.ack() || segment.syn() || !segment.payload().is_empty() {
            return Err(Error::Header);
        }
        self.accept_ack(segment.acknowledgment())?;
        self.peer_window = segment.window();
        if self.state == State::FinWait1 && self.snd_una == self.snd_nxt {
            self.state = State::FinWait2;
        }
        Ok(())
    }

    pub fn close(&mut self, output: &mut [u8]) -> Result<usize, Error> {
        if self.state != State::Established {
            return Err(Error::State);
        }
        let len = encode(
            self.local.address,
            self.remote.address,
            self.local.port,
            self.remote.port,
            self.snd_nxt,
            self.rcv_nxt,
            FLAG_FIN | FLAG_ACK,
            u16::MAX,
            &[],
            output,
        )?;
        self.snd_nxt = self.snd_nxt.wrapping_add(1);
        self.state = State::FinWait1;
        Ok(len)
    }

    pub fn accept_fin(&mut self, input: &[u8], output: &mut [u8]) -> Result<usize, Error> {
        if !matches!(self.state, State::FinWait1 | State::FinWait2) {
            return Err(Error::State);
        }
        let segment = parse(self.remote.address, self.local.address, input)?;
        self.validate_ports(&segment)?;
        if segment.rst() {
            return Err(Error::Reset);
        }
        if !segment.fin() || !segment.ack() || !segment.payload().is_empty() {
            return Err(Error::Header);
        }
        self.accept_ack(segment.acknowledgment())?;
        if segment.sequence() != self.rcv_nxt {
            return Err(Error::Sequence);
        }
        self.rcv_nxt = self.rcv_nxt.wrapping_add(1);
        self.peer_window = segment.window();
        self.state = State::TimeWait;
        self.ack_segment(output)
    }

    fn validate_ports(&self, segment: &Segment<'_>) -> Result<(), Error> {
        if segment.source_port() != self.remote.port
            || segment.destination_port() != self.local.port
        {
            return Err(Error::Port);
        }
        Ok(())
    }

    fn accept_ack(&mut self, acknowledgment: u32) -> Result<(), Error> {
        if sequence_after(acknowledgment, self.snd_nxt)
            || sequence_before(acknowledgment, self.snd_una)
        {
            return Err(Error::Acknowledgment);
        }
        let newly_acked = acknowledgment.wrapping_sub(self.snd_una);
        self.snd_una = acknowledgment;
        self.congestion.on_new_ack(newly_acked);
        Ok(())
    }

    fn ack_segment(&self, output: &mut [u8]) -> Result<usize, Error> {
        encode(
            self.local.address,
            self.remote.address,
            self.local.port,
            self.remote.port,
            self.snd_nxt,
            self.rcv_nxt,
            FLAG_ACK,
            u16::MAX,
            &[],
            output,
        )
    }
}

pub const fn sequence_before(left: u32, right: u32) -> bool {
    (left.wrapping_sub(right) as i32) < 0
}

pub const fn sequence_after(left: u32, right: u32) -> bool {
    sequence_before(right, left)
}

fn add_word(sum: &mut u32, word: u16) {
    *sum = sum.wrapping_add(u32::from(word));
}

fn add_bytes(sum: &mut u32, bytes: &[u8]) {
    let (chunks, remainder) = bytes.as_chunks::<2>();
    for chunk in chunks {
        add_word(sum, u16::from_be_bytes(*chunk));
    }
    if let [last] = remainder {
        add_word(sum, u16::from_be_bytes([*last, 0]));
    }
}

fn folded_sum(source: [u8; 4], destination: [u8; 4], segment: &[u8]) -> u16 {
    let mut sum = 0u32;
    add_bytes(&mut sum, &source);
    add_bytes(&mut sum, &destination);
    add_word(&mut sum, PROTOCOL as u16);
    add_word(&mut sum, segment.len() as u16);
    add_bytes(&mut sum, segment);
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    sum as u16
}

fn checksum(source: [u8; 4], destination: [u8; 4], segment: &[u8]) -> u16 {
    !folded_sum(source, destination, segment)
}

fn checksum_valid(source: [u8; 4], destination: [u8; 4], segment: &[u8]) -> bool {
    folded_sum(source, destination, segment) == 0xffff
}

pub fn parse(source: [u8; 4], destination: [u8; 4], input: &[u8]) -> Result<Segment<'_>, Error> {
    if !(HEADER_BYTES..=ipv4::MAX_PAYLOAD).contains(&input.len()) {
        return Err(Error::Length);
    }
    let words = input[12] >> 4;
    if words < 5 {
        return Err(Error::Header);
    }
    let header_len = usize::from(words) * 4;
    if header_len > input.len() {
        return Err(Error::Length);
    }
    if !checksum_valid(source, destination, input) {
        return Err(Error::Checksum);
    }
    let flags = u16::from_be_bytes([input[12] & 1, input[13]]);
    if flags & 0xe00 != 0 {
        return Err(Error::Header);
    }

    Ok(Segment {
        source_port: u16::from_be_bytes([input[0], input[1]]),
        destination_port: u16::from_be_bytes([input[2], input[3]]),
        sequence: u32::from_be_bytes([input[4], input[5], input[6], input[7]]),
        acknowledgment: u32::from_be_bytes([input[8], input[9], input[10], input[11]]),
        flags,
        window: u16::from_be_bytes([input[14], input[15]]),
        payload: &input[header_len..],
    })
}

#[allow(clippy::too_many_arguments)]
pub fn encode(
    source: [u8; 4],
    destination: [u8; 4],
    source_port: u16,
    destination_port: u16,
    sequence: u32,
    acknowledgment: u32,
    flags: u16,
    window: u16,
    payload: &[u8],
    output: &mut [u8],
) -> Result<usize, Error> {
    if source_port == 0 || destination_port == 0 {
        return Err(Error::Port);
    }
    if payload.len() > MAX_PAYLOAD {
        return Err(Error::Length);
    }
    let length = HEADER_BYTES + payload.len();
    if output.len() < length {
        return Err(Error::OutputTooSmall);
    }

    output[..length].fill(0);
    output[0..2].copy_from_slice(&source_port.to_be_bytes());
    output[2..4].copy_from_slice(&destination_port.to_be_bytes());
    output[4..8].copy_from_slice(&sequence.to_be_bytes());
    output[8..12].copy_from_slice(&acknowledgment.to_be_bytes());
    let offset_flags = DATA_OFFSET_5 | (flags & 0x01ff);
    output[12..14].copy_from_slice(&offset_flags.to_be_bytes());
    output[14..16].copy_from_slice(&window.to_be_bytes());
    output[HEADER_BYTES..length].copy_from_slice(payload);
    let value = checksum(source, destination, &output[..length]);
    output[16..18].copy_from_slice(&value.to_be_bytes());
    Ok(length)
}

fn synthetic_peer(
    source: Endpoint,
    destination: Endpoint,
    sequence: u32,
    acknowledgment: u32,
    flags: u16,
    payload: &[u8],
    output: &mut [u8],
) -> Result<usize, Error> {
    encode(
        source.address,
        destination.address,
        source.port,
        destination.port,
        sequence,
        acknowledgment,
        flags,
        u16::MAX,
        payload,
        output,
    )
}

pub(super) fn self_test() -> Result<(), Error> {
    let local = Endpoint {
        address: [192, 0, 2, 10],
        port: 49152,
    };
    let remote = Endpoint {
        address: [192, 0, 2, 20],
        port: 80,
    };
    let mut client = Client::new(local, remote, 0xffff_fffe);
    let mut wire = [0u8; ipv4::MAX_PAYLOAD];
    let mut reply = [0u8; ipv4::MAX_PAYLOAD];

    let syn_len = client.connect(&mut wire)?;
    let syn = parse(local.address, remote.address, &wire[..syn_len])?;
    if !syn.syn() || syn.sequence() != 0xffff_fffe || client.send_next() != 0xffff_ffff {
        return Err(Error::State);
    }

    let peer_isn = 0x1020_3040;
    let syn_ack_len = synthetic_peer(
        remote,
        local,
        peer_isn,
        client.send_next(),
        FLAG_SYN | FLAG_ACK,
        &[],
        &mut wire,
    )?;
    client.accept_syn_ack(&wire[..syn_ack_len], &mut reply)?;
    if client.state() != State::Established || client.receive_next() != peer_isn.wrapping_add(1) {
        return Err(Error::State);
    }

    let data_len = client.send(b"GET", &mut wire)?;
    let data = parse(local.address, remote.address, &wire[..data_len])?;
    if data.payload() != b"GET" || client.send_next() != 2 {
        return Err(Error::Sequence);
    }

    let ack_len = synthetic_peer(
        remote,
        local,
        client.receive_next(),
        client.send_next(),
        FLAG_ACK,
        &[],
        &mut wire,
    )?;
    client.accept_ack_only(&wire[..ack_len])?;
    if client.congestion_window() <= DEFAULT_SMSS {
        return Err(Error::Invariant);
    }

    let response_len = synthetic_peer(
        remote,
        local,
        client.receive_next(),
        client.send_next(),
        FLAG_ACK | FLAG_PSH,
        b"OK",
        &mut wire,
    )?;
    let mut received = [0u8; 8];
    let (received_len, _) = client.receive(&wire[..response_len], &mut received, &mut reply)?;
    if &received[..received_len] != b"OK" {
        return Err(Error::Sequence);
    }

    let fin_len = client.close(&mut wire)?;
    let fin = parse(local.address, remote.address, &wire[..fin_len])?;
    if !fin.fin() || client.state() != State::FinWait1 {
        return Err(Error::State);
    }
    let peer_ack_len = synthetic_peer(
        remote,
        local,
        client.receive_next(),
        client.send_next(),
        FLAG_ACK,
        &[],
        &mut wire,
    )?;
    client.accept_ack_only(&wire[..peer_ack_len])?;
    if client.state() != State::FinWait2 {
        return Err(Error::State);
    }

    let peer_fin_len = synthetic_peer(
        remote,
        local,
        client.receive_next(),
        client.send_next(),
        FLAG_FIN | FLAG_ACK,
        &[],
        &mut wire,
    )?;
    client.accept_fin(&wire[..peer_fin_len], &mut reply)?;
    if client.state() != State::TimeWait {
        return Err(Error::State);
    }

    let listener = Listener::new(Endpoint {
        address: [192, 0, 2, 30],
        port: 8080,
    });
    let peer = Endpoint {
        address: [192, 0, 2, 40],
        port: 53000,
    };
    let peer_syn = synthetic_peer(
        peer,
        Endpoint {
            address: [192, 0, 2, 30],
            port: 8080,
        },
        700,
        0,
        FLAG_SYN,
        &[],
        &mut wire,
    )?;
    let (pending, syn_ack_len) =
        listener.accept_syn(peer.address, &wire[..peer_syn], 900, &mut reply)?;
    let syn_ack = parse([192, 0, 2, 30], peer.address, &reply[..syn_ack_len])?;
    if !syn_ack.syn() || !syn_ack.ack() || syn_ack.acknowledgment() != 701 {
        return Err(Error::State);
    }
    let final_ack_len = synthetic_peer(
        peer,
        Endpoint {
            address: [192, 0, 2, 30],
            port: 8080,
        },
        701,
        901,
        FLAG_ACK,
        &[],
        &mut wire,
    )?;
    let passive = pending.accept_ack(&wire[..final_ack_len])?;
    if passive.remote() != peer || passive.send_next() != 901 || passive.receive_next() != 701 {
        return Err(Error::State);
    }

    let mut timer = RetransmissionTimer::new();
    timer.arm(10);
    if timer.poll(1_009) != TimerAction::Waiting || timer.poll(1_010) != TimerAction::Retransmit {
        return Err(Error::Invariant);
    }
    timer.acknowledge();
    if timer.retransmissions() != 0 || timer.rto_ticks() != INITIAL_RTO_TICKS {
        return Err(Error::Invariant);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoints() -> (Endpoint, Endpoint) {
        (
            Endpoint {
                address: [192, 0, 2, 1],
                port: 40000,
            },
            Endpoint {
                address: [198, 51, 100, 2],
                port: 443,
            },
        )
    }

    #[test]
    fn production_active_open_data_and_close_path() {
        assert_eq!(self_test(), Ok(()));
    }

    #[test]
    fn wire_round_trip_checks_pseudo_header_and_flags() {
        let (local, remote) = endpoints();
        let mut output = [0xa5; 64];
        let length = encode(
            local.address,
            remote.address,
            local.port,
            remote.port,
            10,
            20,
            FLAG_ACK | FLAG_PSH,
            4096,
            b"abc",
            &mut output,
        )
        .unwrap();
        assert_eq!(length, 23);
        assert_eq!(&output[length..], &[0xa5; 41]);
        let segment = parse(local.address, remote.address, &output[..length]).unwrap();
        assert_eq!(segment.source_port(), local.port);
        assert_eq!(segment.destination_port(), remote.port);
        assert_eq!(segment.sequence(), 10);
        assert_eq!(segment.acknowledgment(), 20);
        assert!(segment.ack());
        assert_eq!(segment.payload(), b"abc");
        let mut wrong_source = local.address;
        wrong_source[3] ^= 1;
        assert_eq!(
            parse(wrong_source, remote.address, &output[..length]),
            Err(Error::Checksum)
        );
    }

    #[test]
    fn sequence_comparisons_wrap_correctly() {
        assert!(sequence_after(0, u32::MAX));
        assert!(sequence_before(u32::MAX, 0));
        assert!(!sequence_before(5, 5));
        assert!(!sequence_after(5, 5));
    }

    #[test]
    fn out_of_order_and_short_receive_are_transactional() {
        let (local, remote) = endpoints();
        let mut client = Client::new(local, remote, 100);
        let mut output = [0u8; 64];
        client.connect(&mut output).unwrap();
        let peer_isn = 500;
        let len = synthetic_peer(
            remote,
            local,
            peer_isn,
            client.send_next(),
            FLAG_SYN | FLAG_ACK,
            &[],
            &mut output,
        )
        .unwrap();
        let input = output;
        let mut ack = [0u8; 64];
        client.accept_syn_ack(&input[..len], &mut ack).unwrap();

        let len = synthetic_peer(
            remote,
            local,
            client.receive_next().wrapping_add(1),
            client.send_next(),
            FLAG_ACK,
            b"x",
            &mut output,
        )
        .unwrap();
        let mut receive = [0xa5; 4];
        assert_eq!(
            client.receive(&output[..len], &mut receive, &mut ack),
            Err(Error::Sequence)
        );
        assert_eq!(receive, [0xa5; 4]);

        let len = synthetic_peer(
            remote,
            local,
            client.receive_next(),
            client.send_next(),
            FLAG_ACK,
            b"hello",
            &mut output,
        )
        .unwrap();
        assert_eq!(
            client.receive(&output[..len], &mut receive, &mut ack),
            Err(Error::ReceiveTooSmall)
        );
        assert_eq!(receive, [0xa5; 4]);
    }

    #[test]
    fn passive_handshake_validates_sequence_ack_and_ports() {
        let server = Endpoint {
            address: [203, 0, 113, 10],
            port: 8080,
        };
        let peer = Endpoint {
            address: [203, 0, 113, 20],
            port: 55000,
        };
        let listener = Listener::new(server);
        let mut input = [0u8; 64];
        let mut output = [0u8; 64];
        let syn_len = synthetic_peer(peer, server, 100, 0, FLAG_SYN, &[], &mut input).unwrap();
        let (pending, syn_ack_len) = listener
            .accept_syn(peer.address, &input[..syn_len], 500, &mut output)
            .unwrap();
        let syn_ack = parse(server.address, peer.address, &output[..syn_ack_len]).unwrap();
        assert!(syn_ack.syn());
        assert!(syn_ack.ack());
        assert_eq!(syn_ack.sequence(), 500);
        assert_eq!(syn_ack.acknowledgment(), 101);

        let ack_len = synthetic_peer(peer, server, 101, 501, FLAG_ACK, &[], &mut input).unwrap();
        let connection = pending.accept_ack(&input[..ack_len]).unwrap();
        assert_eq!(connection.local(), server);
        assert_eq!(connection.remote(), peer);

        let bad_len = synthetic_peer(peer, server, 102, 501, FLAG_ACK, &[], &mut input).unwrap();
        assert_eq!(pending.accept_ack(&input[..bad_len]), Err(Error::Sequence));
    }

    #[test]
    fn congestion_control_is_bounded_by_cwnd_and_receiver_window() {
        let mut cc = CongestionControl::new(1000).unwrap();
        assert_eq!(cc.cwnd(), 1000);
        assert_eq!(cc.send_allowance(8000, 0), 1000);
        assert_eq!(cc.send_allowance(500, 0), 500);
        assert_eq!(cc.send_allowance(8000, 750), 250);

        cc.on_new_ack(500);
        assert_eq!(cc.cwnd(), 1500);
        cc.on_new_ack(1000);
        assert_eq!(cc.cwnd(), 2500);

        cc.on_retransmission_timeout(4000);
        assert_eq!(cc.ssthresh(), 2000);
        assert_eq!(cc.cwnd(), 1000);
        cc.on_new_ack(1000);
        assert_eq!(cc.cwnd(), 2000);
        cc.on_new_ack(1999);
        assert_eq!(cc.cwnd(), 2000);
        cc.on_new_ack(1);
        assert_eq!(cc.cwnd(), 3000);
    }

    #[test]
    fn duplicate_ack_recovery_deflates_to_ssthresh() {
        let mut cc = CongestionControl::new(1000).unwrap();
        cc.on_three_duplicate_acks(8000);
        assert_eq!(cc.ssthresh(), 4000);
        assert_eq!(cc.cwnd(), 7000);
        cc.on_recovery_ack();
        assert_eq!(cc.cwnd(), 4000);
    }

    #[test]
    fn retransmission_timer_backs_off_and_exhausts_without_wrap() {
        let mut timer = RetransmissionTimer::new();
        timer.arm(0);
        let mut now = INITIAL_RTO_TICKS;
        for expected in 1..=MAX_RETRANSMISSIONS {
            assert_eq!(timer.poll(now), TimerAction::Retransmit);
            assert_eq!(timer.retransmissions(), expected);
            now = now.saturating_add(timer.rto_ticks());
        }
        assert_eq!(timer.poll(now), TimerAction::Exhausted);
        assert_eq!(timer.poll(now), TimerAction::Waiting);
        timer.acknowledge();
        assert_eq!(timer.rto_ticks(), INITIAL_RTO_TICKS);
        assert_eq!(timer.retransmissions(), 0);

        let mut saturated = RetransmissionTimer::new();
        saturated.arm(u64::MAX - 500);
        assert_eq!(saturated.poll(u64::MAX - 1), TimerAction::Waiting);
        assert_eq!(saturated.poll(u64::MAX), TimerAction::Retransmit);
        assert_eq!(saturated.rto_ticks(), 2_000);
    }
    #[test]
    fn corrupt_checksum_wrong_ack_and_reset_fail_closed() {
        let (local, remote) = endpoints();
        let mut client = Client::new(local, remote, 1000);
        let mut output = [0u8; 64];
        let syn_len = client.connect(&mut output).unwrap();
        output[16] ^= 1;
        assert_eq!(
            parse(local.address, remote.address, &output[..syn_len]),
            Err(Error::Checksum)
        );

        let len = synthetic_peer(
            remote,
            local,
            77,
            client.send_next().wrapping_add(1),
            FLAG_SYN | FLAG_ACK,
            &[],
            &mut output,
        )
        .unwrap();
        let mut ack = [0u8; 64];
        assert_eq!(
            client.accept_syn_ack(&output[..len], &mut ack),
            Err(Error::Acknowledgment)
        );

        let len = synthetic_peer(
            remote,
            local,
            77,
            client.send_next(),
            FLAG_RST | FLAG_ACK,
            &[],
            &mut output,
        )
        .unwrap();
        assert_eq!(
            client.accept_syn_ack(&output[..len], &mut ack),
            Err(Error::Reset)
        );
    }
}
