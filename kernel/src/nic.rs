//! Allocation-free synchronous Ethernet-frame interface and RAM loopback.
//! Frames exclude preamble/FCS; no network, DMA, PCI or interrupt access occurs.

pub const ETHERNET_HEADER: usize = 14;
pub const MTU: usize = 1500;
pub const MAX_FRAME: usize = ETHERNET_HEADER + MTU;
pub const QUEUE_CAPACITY: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    InvalidMac,
    FrameSize,
    QueueFull,
    BufferTooSmall,
    LinkDown,
    Io,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MacAddress([u8; 6]);

impl MacAddress {
    pub fn new(bytes: [u8; 6]) -> Result<Self, Error> {
        if bytes == [0; 6] || bytes[0] & 1 != 0 {
            return Err(Error::InvalidMac);
        }
        Ok(Self(bytes))
    }

    pub const fn bytes(self) -> [u8; 6] {
        self.0
    }
}

#[path = "arp.rs"]
pub mod arp;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkState {
    Down,
    Up,
}

/// Copies one complete Ethernet frame per call. Implementations must reject
/// frames outside 14..=MTU+14, never retain buffers, and never silently truncate.
/// Transmit success means accepted, NOT delivered to a remote peer. Drivers
/// supply Ethernet padding/FCS as required by their hardware. No offloads/VLAN
/// contract is implied. Receive is nonblocking: None means no queued frame.
/// BufferTooSmall retains the next frame, allowing a larger-buffer retry.
/// A future DMA driver must own its DMA buffers separately from these borrows.
pub trait NetworkInterface {
    fn mac_address(&self) -> MacAddress;
    fn mtu(&self) -> usize;
    fn link_state(&self) -> LinkState;
    fn transmit(&mut self, frame: &[u8]) -> Result<(), Error>;
    fn receive(&mut self, output: &mut [u8]) -> Result<Option<usize>, Error>;
}

#[derive(Clone, Copy)]
struct Frame {
    bytes: [u8; MAX_FRAME],
    length: usize,
}

impl Frame {
    const EMPTY: Self = Self {
        bytes: [0; MAX_FRAME],
        length: 0,
    };
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Statistics {
    pub transmitted: u64,
    pub received: u64,
    pub dropped_on_link_down: u64,
}

/// Reference NIC backend. All traffic stays in this instance's bounded RAM.
/// Exclusive mutable access serializes operations; this is not an IRQ queue.
pub struct Loopback {
    address: MacAddress,
    state: LinkState,
    frames: [Frame; QUEUE_CAPACITY],
    head: usize,
    length: usize,
    statistics: Statistics,
}

impl Loopback {
    pub fn new(address: MacAddress) -> Self {
        Self {
            address,
            state: LinkState::Up,
            frames: [Frame::EMPTY; QUEUE_CAPACITY],
            head: 0,
            length: 0,
            statistics: Statistics::default(),
        }
    }

    pub const fn statistics(&self) -> Statistics {
        self.statistics
    }

    /// Dropping the link discards pending frames, preventing stale traffic
    /// from leaking into a later link session. Counters never wrap.
    pub fn set_link_state(&mut self, state: LinkState) {
        if state == LinkState::Down {
            self.statistics.dropped_on_link_down = self
                .statistics
                .dropped_on_link_down
                .saturating_add(self.length as u64);
            self.frames = [Frame::EMPTY; QUEUE_CAPACITY];
            self.head = 0;
            self.length = 0;
        }
        self.state = state;
    }
}

impl NetworkInterface for Loopback {
    fn mac_address(&self) -> MacAddress {
        self.address
    }

    fn mtu(&self) -> usize {
        MTU
    }

    fn link_state(&self) -> LinkState {
        self.state
    }

    fn transmit(&mut self, frame: &[u8]) -> Result<(), Error> {
        if self.state != LinkState::Up {
            return Err(Error::LinkDown);
        }
        if !(ETHERNET_HEADER..=MAX_FRAME).contains(&frame.len()) {
            return Err(Error::FrameSize);
        }
        if self.length == QUEUE_CAPACITY {
            return Err(Error::QueueFull);
        }
        let tail = (self.head + self.length) % QUEUE_CAPACITY;
        self.frames[tail].bytes[..frame.len()].copy_from_slice(frame);
        self.frames[tail].length = frame.len();
        self.length += 1;
        self.statistics.transmitted = self.statistics.transmitted.saturating_add(1);
        Ok(())
    }

    fn receive(&mut self, output: &mut [u8]) -> Result<Option<usize>, Error> {
        if self.state != LinkState::Up {
            return Err(Error::LinkDown);
        }
        if self.length == 0 {
            return Ok(None);
        }
        let frame = &mut self.frames[self.head];
        if output.len() < frame.length {
            return Err(Error::BufferTooSmall);
        }
        let length = frame.length;
        output[..length].copy_from_slice(&frame.bytes[..length]);
        *frame = Frame::EMPTY;
        self.head = (self.head + 1) % QUEUE_CAPACITY;
        self.length -= 1;
        self.statistics.received = self.statistics.received.saturating_add(1);
        Ok(Some(length))
    }
}

pub fn self_test() -> Result<(), Error> {
    let address = MacAddress::new([2, 0, 0, 0, 0, 1])?;
    arp::self_test().map_err(|_| Error::Io)?;
    let mut nic = Loopback::new(address);
    let interface: &mut dyn NetworkInterface = &mut nic;
    let mut frame = [0xa5; 60];
    frame[..6].copy_from_slice(&address.bytes());
    frame[6..12].copy_from_slice(&address.bytes());
    frame[12..14].copy_from_slice(&[0x88, 0xb5]);
    interface.transmit(&frame)?;
    if interface.receive(&mut [0; 14]) != Err(Error::BufferTooSmall) {
        return Err(Error::Io);
    }
    let mut output = [0u8; 60];
    if interface.receive(&mut output)? != Some(60)
        || output != frame
        || interface.receive(&mut output)?.is_some()
    {
        return Err(Error::Io);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nic() -> Loopback {
        Loopback::new(MacAddress::new([2, 0, 0, 0, 0, 1]).unwrap())
    }

    #[test]
    fn production_trait_round_trip() {
        assert_eq!(self_test(), Ok(()));
    }

    #[test]
    fn invalid_addresses_and_frame_sizes() {
        for address in [[0; 6], [0xff; 6], [1, 0, 0, 0, 0, 1]] {
            assert_eq!(MacAddress::new(address), Err(Error::InvalidMac));
        }
        let mut nic = nic();
        assert_eq!(nic.mtu(), 1500);
        assert_eq!(nic.mac_address().bytes(), [2, 0, 0, 0, 0, 1]);
        assert_eq!(nic.transmit(&[]), Err(Error::FrameSize));
        assert_eq!(nic.transmit(&[0; 13]), Err(Error::FrameSize));
        assert_eq!(nic.transmit(&[0; MAX_FRAME + 1]), Err(Error::FrameSize));
        assert_eq!(nic.transmit(&[0; MAX_FRAME]), Ok(()));
    }

    #[test]
    fn queue_wraps_and_backpressure_preserves_fifo() {
        let mut nic = nic();
        let mut output = [0; 60];
        for cycle in 0..8u8 {
            for index in 0..QUEUE_CAPACITY {
                nic.transmit(&[cycle + index as u8; 60]).unwrap();
            }
            assert_eq!(nic.transmit(&[0xff; 60]), Err(Error::QueueFull));
            for index in 0..QUEUE_CAPACITY {
                assert_eq!(nic.receive(&mut output), Ok(Some(60)));
                assert_eq!(output, [cycle + index as u8; 60]);
            }
            assert_eq!(nic.receive(&mut output), Ok(None));
        }
        assert_eq!(nic.statistics().transmitted, 32);
        assert_eq!(nic.statistics().received, 32);
    }

    #[test]
    fn short_buffer_is_unchanged_and_does_not_dequeue() {
        let mut nic = nic();
        nic.transmit(&[0xab; 60]).unwrap();
        let mut short = [0x55; 59];
        assert_eq!(nic.receive(&mut short), Err(Error::BufferTooSmall));
        assert_eq!(short, [0x55; 59]);
        let mut output = [0x33; 64];
        assert_eq!(nic.receive(&mut output), Ok(Some(60)));
        assert_eq!(&output[..60], &[0xab; 60]);
        assert_eq!(&output[60..], &[0x33; 4]);
    }

    #[test]
    fn link_down_discards_stale_frames() {
        let mut nic = nic();
        nic.transmit(&[0; 60]).unwrap();
        nic.set_link_state(LinkState::Down);
        assert_eq!(nic.link_state(), LinkState::Down);
        assert_eq!(nic.transmit(&[0; 60]), Err(Error::LinkDown));
        assert_eq!(nic.receive(&mut [0; 60]), Err(Error::LinkDown));
        nic.set_link_state(LinkState::Up);
        assert_eq!(nic.receive(&mut [0; 60]), Ok(None));
        assert_eq!(nic.statistics().dropped_on_link_down, 1);
    }
}
