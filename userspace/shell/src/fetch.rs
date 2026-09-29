//! Original allocation-free system summary. Hardware strings are untrusted
//! display data; only printable ASCII reaches the terminal. Unavailable
//! accounting is explicitly unavailable, never a fabricated measurement.

#[derive(Clone, Copy)]
pub struct Cpu {
    pub vendor: [u8; 12],
    pub brand: [u8; 48],
}

impl Cpu {
    pub fn discover() -> Self {
        use core::arch::x86_64::__cpuid_count;

        let basic = __cpuid_count(0, 0);
        let mut cpu = Self {
            vendor: [0; 12],
            brand: [0; 48],
        };
        for (slot, word) in cpu
            .vendor
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip([basic.ebx, basic.edx, basic.ecx])
        {
            slot.copy_from_slice(&word.to_le_bytes());
        }
        if __cpuid_count(0x8000_0000, 0).eax >= 0x8000_0004 {
            for (offset, leaf) in (0x8000_0002..=0x8000_0004).enumerate() {
                let words = __cpuid_count(leaf, 0);
                for (slot, word) in cpu.brand[offset * 16..offset * 16 + 16]
                    .as_chunks_mut::<4>()
                    .0
                    .iter_mut()
                    .zip([words.eax, words.ebx, words.ecx, words.edx])
                {
                    slot.copy_from_slice(&word.to_le_bytes());
                }
            }
        }
        cpu
    }
}

pub fn privilege_level() -> u16 {
    let selector: u16;
    // SAFETY: reading CS is unprivileged, does not access memory, and does not
    // modify flags or segment state. Only its architectural RPL bits escape.
    unsafe {
        core::arch::asm!(
            "mov {0:x}, cs",
            out(reg) selector,
            options(nomem, nostack, preserves_flags)
        )
    };
    selector & 3
}

pub fn decimal(mut value: u64, emit: &mut impl FnMut(&[u8])) {
    let mut digits = [0u8; 20];
    let mut cursor = digits.len();
    loop {
        cursor -= 1;
        digits[cursor] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    emit(&digits[cursor..]);
}

fn label(bytes: &[u8], emit: &mut impl FnMut(&[u8])) {
    let end = bytes
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(bytes.len());
    let bytes = bytes[..end].trim_ascii();
    if bytes.is_empty() {
        emit(b"unavailable");
        return;
    }
    for &byte in bytes {
        emit(&[if (0x20..=0x7e).contains(&byte) {
            byte
        } else {
            b'?'
        }]);
    }
}

pub fn render(cpu: &Cpu, pid: Option<u64>, ring: u16, version: &str, mut emit: impl FnMut(&[u8])) {
    emit(b"\n   /\x5c       Vibrix / vfetch\n  /  \x5c      OS: Vibrix\n / /\x5c \x5c     Arch: x86_64\n \x5c \x5c/ /     Shell: vibrix-sh ");
    emit(version.as_bytes());
    emit(b"\n  \x5c  /      CPU: ");
    label(&cpu.brand, &mut emit);
    emit(b"\n   \x5c/       Vendor: ");
    label(&cpu.vendor, &mut emit);
    emit(b"\n            Privilege: ring ");
    decimal(u64::from(ring), &mut emit);
    emit(b"\n            PID: ");
    match pid {
        Some(pid) => decimal(pid, &mut emit),
        None => emit(b"unavailable"),
    }
    emit(b"\n            Boot: UEFI\n            Root: bootstrap RAM (volatile)\n            Memory usage: unavailable\n\n");
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::{string::String, vec::Vec};

    #[test]
    fn formats_integer_boundaries_without_truncation() {
        for (value, expected) in [(0, "0"), (42, "42"), (u64::MAX, "18446744073709551615")] {
            let mut output = Vec::new();
            decimal(value, &mut |bytes| output.extend_from_slice(bytes));
            assert_eq!(output, expected.as_bytes());
        }
    }

    #[test]
    fn hardware_labels_are_bounded_trimmed_and_escape_free() {
        let mut output = Vec::new();
        label(b"  CPU\x1b[2J\xff  \0ignored", &mut |bytes| {
            output.extend_from_slice(bytes)
        });
        assert_eq!(output, b"CPU?[2J?");
        output.clear();
        label(b"\0junk", &mut |bytes| output.extend_from_slice(bytes));
        assert_eq!(output, b"unavailable");
    }

    #[test]
    fn summary_uses_supplied_measurements_and_discloses_missing_data() {
        let cpu = Cpu {
            vendor: *b"TestVendor12",
            brand: [b'X'; 48],
        };
        let mut output = Vec::new();
        render(&cpu, Some(73), 3, "test", |bytes| {
            output.extend_from_slice(bytes)
        });
        let text = String::from_utf8(output).unwrap();
        assert!(text.contains("CPU: XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"));
        assert!(text.contains("Vendor: TestVendor12"));
        assert!(text.contains("Privilege: ring 3"));
        assert!(text.contains("PID: 73"));
        assert!(text.contains("Memory usage: unavailable"));
        assert!(text.contains("bootstrap RAM (volatile)"));
        assert!(!text.contains("Packages:"));
        let mut missing = Vec::new();
        render(&cpu, None, 0, "test", |bytes| {
            missing.extend_from_slice(bytes)
        });
        assert!(
            String::from_utf8(missing)
                .unwrap()
                .contains("PID: unavailable")
        );
    }
}
