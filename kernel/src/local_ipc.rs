//! Allocation-free loopback packet queue and Unix-domain datagram sockets.

pub const LOOPBACK_PACKET_BYTES: usize = 512;
pub const LOOPBACK_QUEUE_DEPTH: usize = 8;
pub const UNIX_SOCKET_NAME_BYTES: usize = 32;
pub const UNIX_SOCKET_CAPACITY: usize = 8;
pub const UNIX_DATAGRAM_BYTES: usize = 256;
pub const UNIX_QUEUE_DEPTH: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    EmptyPacket,
    PacketTooLarge,
    BufferTooSmall,
    QueueFull,
    WouldBlock,
    InvalidName,
    NameInUse,
    NotFound,
    Capacity,
    SourceNotBound,
}

#[derive(Clone, Copy)]
struct Packet {
    bytes: [u8; LOOPBACK_PACKET_BYTES],
    len: u16,
}

impl Packet {
    const EMPTY: Self = Self {
        bytes: [0; LOOPBACK_PACKET_BYTES],
        len: 0,
    };
}

pub struct Loopback {
    queue: [Packet; LOOPBACK_QUEUE_DEPTH],
    head: usize,
    len: usize,
}

impl Loopback {
    pub const fn new() -> Self {
        Self {
            queue: [Packet::EMPTY; LOOPBACK_QUEUE_DEPTH],
            head: 0,
            len: 0,
        }
    }

    pub fn transmit(&mut self, packet: &[u8]) -> Result<(), Error> {
        if packet.is_empty() {
            return Err(Error::EmptyPacket);
        }
        if packet.len() > LOOPBACK_PACKET_BYTES {
            return Err(Error::PacketTooLarge);
        }
        if self.len == LOOPBACK_QUEUE_DEPTH {
            return Err(Error::QueueFull);
        }
        let slot = (self.head + self.len) % LOOPBACK_QUEUE_DEPTH;
        self.queue[slot].bytes[..packet.len()].copy_from_slice(packet);
        self.queue[slot].len = packet.len() as u16;
        self.len += 1;
        Ok(())
    }

    pub fn receive(&mut self, output: &mut [u8]) -> Result<usize, Error> {
        if self.len == 0 {
            return Err(Error::WouldBlock);
        }
        let packet = self.queue[self.head];
        let packet_len = usize::from(packet.len);
        if output.len() < packet_len {
            return Err(Error::BufferTooSmall);
        }
        output[..packet_len].copy_from_slice(&packet.bytes[..packet_len]);
        self.queue[self.head] = Packet::EMPTY;
        self.head = (self.head + 1) % LOOPBACK_QUEUE_DEPTH;
        self.len -= 1;
        Ok(packet_len)
    }

    pub const fn queued(&self) -> usize {
        self.len
    }
}

impl Default for Loopback {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnixName {
    bytes: [u8; UNIX_SOCKET_NAME_BYTES],
    len: u8,
}

impl UnixName {
    pub const EMPTY: Self = Self {
        bytes: [0; UNIX_SOCKET_NAME_BYTES],
        len: 0,
    };

    pub fn new(name: &[u8]) -> Result<Self, Error> {
        if name.len() < 2
            || name.len() > UNIX_SOCKET_NAME_BYTES
            || name[0] != b'/'
            || name.contains(&0)
            || name.ends_with(b"/")
        {
            return Err(Error::InvalidName);
        }
        let mut value = Self::EMPTY;
        value.bytes[..name.len()].copy_from_slice(name);
        value.len = name.len() as u8;
        Ok(value)
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }
}

#[derive(Clone, Copy)]
struct Datagram {
    source: UnixName,
    bytes: [u8; UNIX_DATAGRAM_BYTES],
    len: u16,
}

impl Datagram {
    const EMPTY: Self = Self {
        source: UnixName::EMPTY,
        bytes: [0; UNIX_DATAGRAM_BYTES],
        len: 0,
    };
}

#[derive(Clone, Copy)]
struct Socket {
    name: UnixName,
    queue: [Datagram; UNIX_QUEUE_DEPTH],
    head: u8,
    len: u8,
}

impl Socket {
    const fn new(name: UnixName) -> Self {
        Self {
            name,
            queue: [Datagram::EMPTY; UNIX_QUEUE_DEPTH],
            head: 0,
            len: 0,
        }
    }

    fn push(&mut self, source: UnixName, bytes: &[u8]) -> Result<(), Error> {
        if bytes.len() > UNIX_DATAGRAM_BYTES {
            return Err(Error::PacketTooLarge);
        }
        if usize::from(self.len) == UNIX_QUEUE_DEPTH {
            return Err(Error::QueueFull);
        }
        let slot = (usize::from(self.head) + usize::from(self.len)) % UNIX_QUEUE_DEPTH;
        let datagram = &mut self.queue[slot];
        datagram.source = source;
        datagram.bytes[..bytes.len()].copy_from_slice(bytes);
        datagram.len = bytes.len() as u16;
        self.len += 1;
        Ok(())
    }

    fn pop(&mut self, output: &mut [u8]) -> Result<(UnixName, usize), Error> {
        if self.len == 0 {
            return Err(Error::WouldBlock);
        }
        let datagram = self.queue[usize::from(self.head)];
        let len = usize::from(datagram.len);
        if output.len() < len {
            return Err(Error::BufferTooSmall);
        }
        output[..len].copy_from_slice(&datagram.bytes[..len]);
        self.queue[usize::from(self.head)] = Datagram::EMPTY;
        self.head = ((usize::from(self.head) + 1) % UNIX_QUEUE_DEPTH) as u8;
        self.len -= 1;
        Ok((datagram.source, len))
    }
}

pub struct UnixDatagramRegistry {
    sockets: [Option<Socket>; UNIX_SOCKET_CAPACITY],
}

impl UnixDatagramRegistry {
    pub const fn new() -> Self {
        Self {
            sockets: [None; UNIX_SOCKET_CAPACITY],
        }
    }

    pub fn bind(&mut self, name: UnixName) -> Result<(), Error> {
        if self
            .sockets
            .iter()
            .flatten()
            .any(|socket| socket.name == name)
        {
            return Err(Error::NameInUse);
        }
        let slot = self
            .sockets
            .iter()
            .position(Option::is_none)
            .ok_or(Error::Capacity)?;
        self.sockets[slot] = Some(Socket::new(name));
        Ok(())
    }

    pub fn unbind(&mut self, name: UnixName) -> Result<(), Error> {
        let slot = self
            .sockets
            .iter()
            .position(|socket| socket.is_some_and(|socket| socket.name == name))
            .ok_or(Error::NotFound)?;
        self.sockets[slot] = None;
        Ok(())
    }

    pub fn send_to(
        &mut self,
        source: UnixName,
        destination: UnixName,
        bytes: &[u8],
    ) -> Result<(), Error> {
        if !self
            .sockets
            .iter()
            .flatten()
            .any(|socket| socket.name == source)
        {
            return Err(Error::SourceNotBound);
        }
        let target = self
            .sockets
            .iter_mut()
            .flatten()
            .find(|socket| socket.name == destination)
            .ok_or(Error::NotFound)?;
        target.push(source, bytes)
    }

    pub fn recv_from(
        &mut self,
        name: UnixName,
        output: &mut [u8],
    ) -> Result<(UnixName, usize), Error> {
        let socket = self
            .sockets
            .iter_mut()
            .flatten()
            .find(|socket| socket.name == name)
            .ok_or(Error::NotFound)?;
        socket.pop(output)
    }
}

impl Default for UnixDatagramRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub fn self_test() -> Result<(), Error> {
    let mut loopback = Loopback::new();
    loopback.transmit(b"ipv4-loopback")?;
    let mut packet = [0u8; 32];
    let count = loopback.receive(&mut packet)?;
    if &packet[..count] != b"ipv4-loopback" || loopback.queued() != 0 {
        return Err(Error::WouldBlock);
    }

    let server = UnixName::new(b"/run/server")?;
    let client = UnixName::new(b"/run/client")?;
    let mut registry = UnixDatagramRegistry::new();
    registry.bind(server)?;
    registry.bind(client)?;
    registry.send_to(client, server, b"ping")?;
    let mut bytes = [0u8; 8];
    let (source, count) = registry.recv_from(server, &mut bytes)?;
    if source != client || &bytes[..count] != b"ping" {
        return Err(Error::NotFound);
    }
    registry.send_to(server, client, b"pong")?;
    let (source, count) = registry.recv_from(client, &mut bytes)?;
    if source != server || &bytes[..count] != b"pong" {
        return Err(Error::NotFound);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_self_test_passes() {
        self_test().unwrap();
    }

    #[test]
    fn loopback_preserves_packet_boundaries_and_failure_atomicity() {
        let mut loopback = Loopback::new();
        loopback.transmit(b"a").unwrap();
        loopback.transmit(b"bc").unwrap();
        let mut too_small = [0u8; 1];
        assert_eq!(loopback.receive(&mut too_small), Ok(1));
        assert_eq!(loopback.receive(&mut too_small), Err(Error::BufferTooSmall));
        assert_eq!(loopback.queued(), 1);
        let mut output = [0u8; 2];
        assert_eq!(loopback.receive(&mut output), Ok(2));
        assert_eq!(&output, b"bc");
        assert_eq!(loopback.receive(&mut output), Err(Error::WouldBlock));
    }

    #[test]
    fn unix_datagrams_require_bound_endpoints_and_preserve_source() {
        let a = UnixName::new(b"/a").unwrap();
        let b = UnixName::new(b"/b").unwrap();
        let missing = UnixName::new(b"/missing").unwrap();
        let mut registry = UnixDatagramRegistry::new();
        registry.bind(a).unwrap();
        registry.bind(b).unwrap();
        assert_eq!(
            registry.send_to(missing, b, b"x"),
            Err(Error::SourceNotBound)
        );
        assert_eq!(registry.send_to(a, missing, b"x"), Err(Error::NotFound));
        registry.send_to(a, b, b"hello").unwrap();
        let mut output = [0u8; 16];
        let (source, count) = registry.recv_from(b, &mut output).unwrap();
        assert_eq!(source, a);
        assert_eq!(&output[..count], b"hello");
    }

    #[test]
    fn unix_queue_capacity_is_bounded() {
        let a = UnixName::new(b"/a").unwrap();
        let b = UnixName::new(b"/b").unwrap();
        let mut registry = UnixDatagramRegistry::new();
        registry.bind(a).unwrap();
        registry.bind(b).unwrap();
        for _ in 0..UNIX_QUEUE_DEPTH {
            registry.send_to(a, b, b"x").unwrap();
        }
        assert_eq!(registry.send_to(a, b, b"x"), Err(Error::QueueFull));
    }

    #[test]
    fn names_fail_closed() {
        assert_eq!(UnixName::new(b""), Err(Error::InvalidName));
        assert_eq!(UnixName::new(b"relative"), Err(Error::InvalidName));
        assert_eq!(UnixName::new(b"/bad/"), Err(Error::InvalidName));
        assert_eq!(UnixName::new(b"/bad\0name"), Err(Error::InvalidName));
    }
}
