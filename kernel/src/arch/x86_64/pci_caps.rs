//! Read-only production capability inventory in the native PCI scan.

use super::pci::Device;
use super::pci_interrupts::{Capabilities, Error};

#[derive(Default)]
pub(super) struct Inventory {
    msi: u32,
    msix: u32,
    malformed: u32,
}

impl Inventory {
    pub(super) fn observe(&mut self, device: Device, result: Result<Capabilities, Error>) {
        let capabilities = match result {
            Ok(found) => found,
            Err(error) => {
                self.malformed += 1;
                crate::println!(
                    "PCI capability rejected {:02x}:{:02x}.{}: {:?}",
                    device.bdf.bus,
                    device.bdf.device,
                    device.bdf.function,
                    error
                );
                return;
            }
        };
        if let Some(msi) = capabilities.msi {
            self.msi += 1;
            crate::println!(
                "PCI MSI {:04x}:{:04x} caps={} offset={:#x} control={:#x} messages={} addr64={} mask={}",
                device.vendor,
                device.id,
                capabilities.count,
                msi.offset(),
                msi.control(),
                msi.message_capacity(),
                msi.address_64_bit(),
                msi.per_vector_masking()
            );
            if device.vendor == 0x1234 && device.id == 0x11e8 {
                crate::debugcon::write("VIBRIX: EDU MSI capability found\r\n");
            }
        }
        if let Some(msix) = capabilities.msix {
            self.msix += 1;
            crate::println!(
                "PCI MSI-X {:04x}:{:04x} offset={:#x} control={:#x} vectors={} table=BAR{}+{:#x} pba=BAR{}+{:#x}",
                device.vendor,
                device.id,
                msix.offset(),
                msix.control(),
                msix.vectors(),
                msix.table().bir(),
                msix.table().offset(),
                msix.pending().bir(),
                msix.pending().offset()
            );
            if device.vendor == 0x1af4 && device.id == 0x1110 {
                crate::debugcon::write("VIBRIX: IVSHMEM MSI-X capability found\r\n");
            }
        }
    }

    pub(super) fn finish(self) {
        crate::println!(
            "PCI interrupt inventory: MSI={} MSI-X={} rejected={} (read-only)",
            self.msi,
            self.msix,
            self.malformed
        );
        crate::debugcon::write("VIBRIX: PCI interrupt inventory complete\r\n");
    }
}
