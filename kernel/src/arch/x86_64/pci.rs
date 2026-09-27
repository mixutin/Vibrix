//! Read-only PCI segment-zero discovery using x86 configuration mechanism #1.
//!
//! Legacy 0xCF8/0xCFC accesses cover **only segment 0**, not the ACPI MCFG
//! segments. No PCI command/BAR/bridge registers are written, no BAR is sized
//! by writing all ones, and no MMIO or block device is touched.

#[cfg(not(test))]
use core::arch::asm;

#[cfg(not(test))]
const CONFIG_ADDRESS_PORT: u16 = 0xcf8;
#[cfg(not(test))]
const CONFIG_DATA_PORT: u16 = 0xcfc;
const PCI_ENABLE_BIT: u32 = 0x8000_0000;
const PCI_VENDOR_NONE: u16 = 0xffff;
const PCI_VENDOR_INVALID: u16 = 0;
const HEADER_MULTIFUNCTION: u8 = 0x80;
const HEADER_KIND_MASK: u8 = 0x7f;
const CLASS_SERIAL_BUS: u8 = 0x0c;
const SUBCLASS_USB: u8 = 0x03;
const USB_PROG_IF_XHCI: u8 = 0x30;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Bdf {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
}

/// CF8 config selection address. Byte offsets are in the 256-byte legacy
/// config header, not ECAM's 4096-byte extended config space.
pub const fn configuration_address(bdf: Bdf, offset: u8) -> Option<u32> {
    if bdf.device >= 32 || bdf.function >= 8 || offset & 3 != 0 {
        return None;
    }
    Some(
        PCI_ENABLE_BIT
            | ((bdf.bus as u32) << 16)
            | ((bdf.device as u32) << 11)
            | ((bdf.function as u32) << 8)
            | offset as u32,
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Device {
    pub bdf: Bdf,
    pub vendor: u16,
    pub id: u16,
    pub class: u8,
    pub subclass: u8,
    pub programming_interface: u8,
    pub revision: u8,
    pub header_type: u8,
}

impl Device {
    pub const fn is_usb_controller(self) -> bool {
        self.class == CLASS_SERIAL_BUS && self.subclass == SUBCLASS_USB
    }

    pub const fn is_xhci(self) -> bool {
        self.is_usb_controller() && self.programming_interface == USB_PROG_IF_XHCI
    }

    pub const fn bar_slots(self) -> u8 {
        match self.header_type & HEADER_KIND_MASK {
            0 => 6, // endpoint
            1 => 2, // PCI-to-PCI bridge
            _ => 0, // CardBus and unknown headers have different layouts
        }
    }
}

/// Decoded *assigned base only*. Size, enabling, cache type, IOMMU and safe
/// dereference require separate device-specific work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Bar {
    Io { port: u32 },
    Memory32 { physical: u32, prefetchable: bool },
    Memory64 { physical: u64, prefetchable: bool },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BarError {
    ReservedMemoryType,
    TruncatedMemory64,
}

/// Decode one BAR, returning (optional assigned BAR, consumed register count).
/// Two dwords are consumed for a 64-bit BAR even if it is unassigned.
pub fn decode_bar(low: u32, high: Option<u32>) -> Result<(Option<Bar>, u8), BarError> {
    if low == 0 || low == u32::MAX {
        return Ok((None, 1));
    }
    if low & 1 != 0 {
        let port = low & !0x3;
        return Ok(((port != 0).then_some(Bar::Io { port }), 1));
    }
    let prefetchable = low & 0x8 != 0;
    match (low >> 1) & 3 {
        0 => {
            let physical = low & !0xf;
            Ok((
                (physical != 0).then_some(Bar::Memory32 {
                    physical,
                    prefetchable,
                }),
                1,
            ))
        }
        2 => {
            let high = high.ok_or(BarError::TruncatedMemory64)?;
            let physical = (u64::from(high) << 32) | u64::from(low & !0xf);
            Ok((
                (physical != 0).then_some(Bar::Memory64 {
                    physical,
                    prefetchable,
                }),
                2,
            ))
        }
        _ => Err(BarError::ReservedMemoryType),
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Summary {
    pub devices: u32,
    pub usb_controllers: u32,
    pub xhci_controllers: u32,
    pub assigned_bars: u32,
    pub malformed_bars: u32,
}

/// Enumerate all 256 legacy segment-zero buses, 32 devices/bus, and functions
/// 1-7 only when function zero advertises multifunction support.
///
/// Read callback must implement aligned 32-bit *read-only* config access. The
/// visitor is called once per present function; caller may bind native
/// drivers later, but this function only discovers metadata.
pub fn scan_segment_zero(
    mut read: impl FnMut(Bdf, u8) -> u32,
    mut visit: impl FnMut(Device),
) -> Summary {
    let mut summary = Summary::default();
    for bus in 0u16..=255 {
        for device in 0..32u8 {
            let zero = Bdf {
                bus: bus as u8,
                device,
                function: 0,
            };
            let first = read(zero, 0);
            if !present_vendor(first) {
                continue;
            }
            let header_zero = ((read(zero, 0x0c) >> 16) & 0xff) as u8;
            let function_count = if header_zero & HEADER_MULTIFUNCTION != 0 {
                8
            } else {
                1
            };
            for function in 0..function_count {
                let bdf = Bdf {
                    bus: bus as u8,
                    device,
                    function,
                };
                let vendor_and_id = if function == 0 { first } else { read(bdf, 0) };
                if !present_vendor(vendor_and_id) {
                    continue;
                }
                let class_data = read(bdf, 0x08);
                let header_type = ((read(bdf, 0x0c) >> 16) & 0xff) as u8;
                let found = Device {
                    bdf,
                    vendor: vendor_and_id as u16,
                    id: (vendor_and_id >> 16) as u16,
                    class: (class_data >> 24) as u8,
                    subclass: (class_data >> 16) as u8,
                    programming_interface: (class_data >> 8) as u8,
                    revision: class_data as u8,
                    header_type,
                };
                summary.devices += 1;
                if found.is_usb_controller() {
                    summary.usb_controllers += 1;
                }
                if found.is_xhci() {
                    summary.xhci_controllers += 1;
                }
                visit(found);
                inspect_bars(&mut read, found, &mut summary);
            }
        }
    }
    summary
}

fn present_vendor(word: u32) -> bool {
    let vendor = word as u16;
    vendor != PCI_VENDOR_NONE && vendor != PCI_VENDOR_INVALID
}

fn inspect_bars(read: &mut impl FnMut(Bdf, u8) -> u32, device: Device, sum: &mut Summary) {
    let count = device.bar_slots();
    let mut index = 0;
    while index < count {
        let offset = 0x10 + index * 4;
        let low = read(device.bdf, offset);
        // A 64-bit BAR consumes a pair of 32-bit registers. Reject a
        // 64-bit lower dword in the final slot without reading outside
        // the device header's BAR range.
        let is_64 = low & 1 == 0 && (low >> 1) & 3 == 2;
        let high = if is_64 && index + 1 < count {
            Some(read(device.bdf, offset + 4))
        } else {
            None
        };
        match decode_bar(low, high) {
            Ok((Some(_bar), used)) => {
                sum.assigned_bars += 1;
                index += used;
            }
            Ok((None, used)) => index += used,
            Err(_) => {
                sum.malformed_bars += 1;
                index += 1;
            }
        }
    }
}

/// CF8 selects an aligned configuration DWORD, then CFC reads it. This
/// touches *only* PCI configuration address/data ports; it does not issue a
/// config-space write, enable bus mastering or access any BAR.
///
/// # Safety
/// Only ring-zero x86-64 code with port-I/O permission may call this. The
/// caller must serialize CF8/CFC access globally across CPUs/interrupts;
/// current early kernel satisfies this with one boot CPU and IF cleared.
#[cfg(not(test))]
unsafe fn read_legacy_dword(bdf: Bdf, offset: u8) -> u32 {
    let Some(address) = configuration_address(bdf, offset) else {
        return u32::MAX;
    };
    // SAFETY: PCI config mechanism #1 I/O ports in ring 0; no other
    // concurrent config-port consumer exists in the early native kernel.
    unsafe {
        asm!(
            "out dx, eax",
            in("dx") CONFIG_ADDRESS_PORT,
            in("eax") address,
            options(nostack, preserves_flags)
        );
    }
    let word: u32;
    // SAFETY: paired data-port read after selecting a valid config DWORD.
    unsafe {
        asm!(
            "in eax, dx",
            in("dx") CONFIG_DATA_PORT,
            out("eax") word,
            options(nostack, preserves_flags)
        );
    }
    word
}

/// Run the real post-ExitBootServices segment-zero scan.
///
/// # Safety
/// Sole boot CPU, IF=0, ring zero and no concurrent CF8/CFC user. Do not
/// use this mechanism for ACPI MCFG segments other than zero.
#[cfg(not(test))]
pub unsafe fn discover_legacy_segment_zero(mut visit: impl FnMut(Device)) -> Summary {
    scan_segment_zero(
        |bdf, offset| unsafe { read_legacy_dword(bdf, offset) },
        |device| visit(device),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bdf(bus: u8, device: u8, function: u8) -> Bdf {
        Bdf {
            bus,
            device,
            function,
        }
    }

    #[test]
    fn configuration_selects_only_valid_aligned_registers() {
        assert_eq!(
            configuration_address(bdf(0x12, 0x1f, 7), 0xfc),
            Some(0x8012_fffc)
        );
        assert_eq!(configuration_address(bdf(0, 32, 0), 0), None);
        assert_eq!(configuration_address(bdf(0, 0, 8), 0), None);
        assert_eq!(configuration_address(bdf(0, 0, 0), 3), None);
    }

    #[test]
    fn decodes_io_memory32_memory64_and_invalid_final_slot() {
        assert_eq!(
            decode_bar(0xc001, None),
            Ok((Some(Bar::Io { port: 0xc000 }), 1))
        );
        assert_eq!(
            decode_bar(0xe000_0008, None),
            Ok((
                Some(Bar::Memory32 {
                    physical: 0xe000_0000,
                    prefetchable: true
                }),
                1
            ))
        );
        assert_eq!(
            decode_bar(0x0000_200c, Some(0x1234_5678)),
            Ok((
                Some(Bar::Memory64 {
                    physical: 0x1234_5678_0000_2000,
                    prefetchable: true
                }),
                2
            ))
        );
        assert_eq!(decode_bar(0x4, None), Err(BarError::TruncatedMemory64));
        assert_eq!(decode_bar(0x6, None), Err(BarError::ReservedMemoryType));
        assert_eq!(decode_bar(0, None), Ok((None, 1)));
        assert_eq!(decode_bar(u32::MAX, None), Ok((None, 1)));
    }

    #[test]
    fn scans_multiple_buses_multifunction_and_only_valid_bars() {
        let mut visited = [None; 4];
        let mut count = 0usize;
        let mut calls_to_absent_functions = 0;
        let summary = scan_segment_zero(
            |where_, offset| match (where_.bus, where_.device, where_.function, offset) {
                (0, 1, 0, 0) => 0x1234_8086,
                (0, 1, 0, 8) => 0x0c03_3001, // xHCI
                (0, 1, 0, 0x0c) => 0,
                (0, 1, 0, 0x10) => 0xe000_0000,
                (0, 2, 0, 0) => 0x5678_1234,
                (0, 2, 0, 8) => 0x0604_0000,    // PCI bridge
                (0, 2, 0, 0x0c) => 0x0080_0000, // multifunction
                (0, 2, 1, 0) => 0xabcd_1234,
                (0, 2, 1, 8) => 0x0200_0000,    // Ethernet
                (0, 2, 1, 0x0c) => 0x0001_0000, // type 1 (2 BARs)
                (0, 2, 1, 0x10) => 0xd001,
                (0, 1, 1..=7, _) => {
                    calls_to_absent_functions += 1;
                    u32::MAX
                }
                _ => u32::MAX,
            },
            |dev| {
                if count < visited.len() {
                    visited[count] = Some(dev);
                }
                count += 1;
            },
        );
        assert_eq!(summary.devices, 3);
        assert_eq!(summary.usb_controllers, 1);
        assert_eq!(summary.xhci_controllers, 1);
        assert_eq!(summary.assigned_bars, 2);
        assert_eq!(summary.malformed_bars, 0);
        assert_eq!(count, 3);
        assert_eq!(visited[2].unwrap().bdf, bdf(0, 2, 1));
        assert_eq!(calls_to_absent_functions, 0);
    }

    #[test]
    fn malformed_bar_never_reads_beyond_type_one_header() {
        let mut upper_read = false;
        let result = scan_segment_zero(
            |where_, offset| match (where_.bus, where_.device, where_.function, offset) {
                (0, 0, 0, 0) => 0x1234_8086,
                (0, 0, 0, 8) => 0,
                (0, 0, 0, 0x0c) => 0x0001_0000,
                (0, 0, 0, 0x14) => 4,
                (0, 0, 0, 0x18) => {
                    upper_read = true;
                    0
                }
                _ => u32::MAX,
            },
            |_| {},
        );
        assert_eq!(result.malformed_bars, 1);
        assert!(!upper_read);
    }
}
