//! USB HID 1.11 boot-protocol contracts, independent of the host controller.
//! No arbitrary report-descriptor interpreter or allocator is required.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Malformed,
    Unsupported,
    Rollover,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protocol {
    Keyboard = 1,
    Mouse = 2,
}

impl Protocol {
    pub const fn report_bytes(self) -> usize {
        match self {
            Self::Keyboard => 8,
            Self::Mouse => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Endpoint {
    pub configuration: u8,
    pub interface: u8,
    pub address: u8,
    pub max_packet: u16,
    pub interval: u8,
}

impl Endpoint {
    pub fn context_index(self) -> u8 {
        (self.address & 0x0f) * 2 + 1
    }
}

/// Select exactly one alternate-zero boot interface and its interrupt-IN
/// endpoint from a complete configuration (bounded by the transport to 256B).
pub fn endpoint(bytes: &[u8], protocol: Protocol) -> Result<Endpoint, Error> {
    if bytes.len() < 9 || bytes.len() > 256 || bytes[0] != 9 || bytes[1] != 2 {
        return Err(Error::Malformed);
    }
    if usize::from(u16::from_le_bytes([bytes[2], bytes[3]])) != bytes.len() || bytes[5] == 0 {
        return Err(Error::Malformed);
    }
    let mut interface = None;
    let mut selected = None;
    let mut offset = 9;
    while offset < bytes.len() {
        if bytes.len() - offset < 2 {
            return Err(Error::Malformed);
        }
        let len = usize::from(bytes[offset]);
        if len < 2 || len > bytes.len() - offset {
            return Err(Error::Malformed);
        }
        let descriptor = &bytes[offset..offset + len];
        match descriptor[1] {
            4 => {
                if len < 9 {
                    return Err(Error::Malformed);
                }
                interface = if descriptor[3] == 0
                    && descriptor[5] == 3
                    && descriptor[6] == 1
                    && descriptor[7] == protocol as u8
                {
                    Some(descriptor[2])
                } else {
                    None
                };
            }
            5 => {
                if len < 7 {
                    return Err(Error::Malformed);
                }
                if let Some(interface) = interface
                    && descriptor[2] & 0x80 != 0
                    && descriptor[3] & 3 == 3
                {
                    let packet = u16::from_le_bytes([descriptor[4], descriptor[5]]);
                    if descriptor[2] & 0x70 != 0
                        || descriptor[2] & 0x0f == 0
                        || packet < protocol.report_bytes() as u16
                        || packet > 64
                        || descriptor[6] == 0
                    {
                        return Err(Error::Unsupported);
                    }
                    if selected.is_some() {
                        return Err(Error::Unsupported);
                    }
                    selected = Some(Endpoint {
                        configuration: bytes[5],
                        interface,
                        address: descriptor[2],
                        max_packet: packet,
                        interval: descriptor[6],
                    });
                }
            }
            _ => {}
        }
        offset += len;
    }
    selected.ok_or(Error::Unsupported)
}

/// xHCI 1.2c table 6-12: full/low-speed intervals round down in microframes.
pub fn interval(speed: u8, value: u8) -> Result<u8, Error> {
    match speed {
        // xHCI speed IDs 1/2 are full/low speed. HID bInterval is in
        // milliseconds; xHCI stores a power-of-two microframe exponent.
        1 | 2 if value != 0 => Ok((u32::from(value) * 8).ilog2() as u8),
        // High-speed interrupt endpoints encode bInterval as an exponent
        // from 1..=16; xHCI's Interval field is that exponent minus one.
        3 if (1..=16).contains(&value) => Ok(value - 1),
        _ => Err(Error::Unsupported),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Keyboard {
    pub modifiers: u8,
    pub keys: [u8; 6],
}

impl Keyboard {
    pub const RELEASED: Self = Self {
        modifiers: 0,
        keys: [0; 6],
    };

    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() != 8 {
            return Err(Error::Malformed);
        }
        let mut keys = [0; 6];
        keys.copy_from_slice(&bytes[2..]);
        if keys.iter().any(|key| matches!(*key, 1..=3)) {
            return Err(Error::Rollover);
        }
        for (index, &key) in keys.iter().enumerate() {
            if key != 0 && keys[..index].contains(&key) {
                return Err(Error::Malformed);
            }
        }
        Ok(Self {
            modifiers: bytes[0],
            keys,
        })
    }

    /// Newly pressed non-modifier usages; reordered held keys do not repeat.
    pub fn presses(self, previous: Self) -> [u8; 6] {
        let mut pressed = [0; 6];
        for (index, key) in self.keys.into_iter().enumerate() {
            if key != 0 && !previous.keys.contains(&key) {
                pressed[index] = key;
            }
        }
        pressed
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mouse {
    pub buttons: u8,
    pub dx: i8,
    pub dy: i8,
}

impl Mouse {
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() != 3 {
            return Err(Error::Malformed);
        }
        Ok(Self {
            buttons: bytes[0] & 7,
            dx: bytes[1] as i8,
            dy: bytes[2] as i8,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: [u8; 25] = [
        9, 2, 25, 0, 1, 1, 0, 0x80, 50, 9, 4, 0, 0, 1, 3, 1, 1, 0, 7, 5, 0x81, 3, 8, 0, 10,
    ];

    #[test]
    fn selects_only_the_requested_boot_interface() {
        let ep = endpoint(&CONFIG, Protocol::Keyboard).unwrap();
        assert_eq!(ep.context_index(), 3);
        assert_eq!((ep.max_packet, ep.interval), (8, 10));
        assert_eq!(endpoint(&CONFIG, Protocol::Mouse), Err(Error::Unsupported));
        let mut config = CONFIG;
        config[16] = 2;
        assert!(endpoint(&config, Protocol::Mouse).is_ok());
        assert_eq!(interval(1, 10), Ok(6));
        assert_eq!(interval(2, 255), Ok(10));
        assert_eq!(interval(1, 0), Err(Error::Unsupported));
        assert_eq!(interval(4, 10), Err(Error::Unsupported));
    }

    #[test]
    fn malformed_or_unsupported_descriptors_fail_closed() {
        for len in 0..CONFIG.len() {
            assert!(endpoint(&CONFIG[..len], Protocol::Keyboard).is_err());
        }
        for (offset, value) in [(9, 0), (9, 1), (9, 255), (20, 0x80), (22, 0), (24, 0)] {
            let mut config = CONFIG;
            config[offset] = value;
            assert!(endpoint(&config, Protocol::Keyboard).is_err());
        }
        let mut config = CONFIG;
        config[12] = 1; // alternate setting not enabled
        assert_eq!(
            endpoint(&config, Protocol::Keyboard),
            Err(Error::Unsupported)
        );
    }

    #[test]
    fn keyboard_state_handles_modifiers_release_and_reordering() {
        let first = Keyboard::decode(&[2, 0, 4, 5, 0, 0, 0, 0]).unwrap();
        assert_eq!(first.modifiers, 2);
        assert_eq!(first.presses(Keyboard::RELEASED), [4, 5, 0, 0, 0, 0]);
        let reordered = Keyboard::decode(&[2, 0, 5, 4, 0, 0, 0, 0]).unwrap();
        assert_eq!(reordered.presses(first), [0; 6]);
        assert_eq!(Keyboard::decode(&[0; 8]), Ok(Keyboard::RELEASED));
        assert_eq!(Keyboard::decode(&[0; 7]), Err(Error::Malformed));
        assert_eq!(
            Keyboard::decode(&[0, 0, 1, 1, 1, 1, 1, 1]),
            Err(Error::Rollover)
        );
        assert_eq!(
            Keyboard::decode(&[0, 0, 4, 4, 0, 0, 0, 0]),
            Err(Error::Malformed)
        );
    }

    #[test]
    fn mouse_reports_have_signed_motion_and_three_buttons() {
        assert_eq!(
            Mouse::decode(&[0xf9, 7, 251]),
            Ok(Mouse {
                buttons: 1,
                dx: 7,
                dy: -5
            })
        );
        assert_eq!(Mouse::decode(&[0, 0]), Err(Error::Malformed));
    }
}
