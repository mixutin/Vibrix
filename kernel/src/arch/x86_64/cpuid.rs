//! x86-64 CPUID discovery without external runtime dependencies.
//!
//! CPUID is available in 64-bit mode. Probe the maximum basic and extended
//! leaf before requesting optional leaves: unsupported queries can alias the
//! last supported basic leaf rather than producing an error.

use core::arch::x86_64::__cpuid_count;

const EXTENDED_BASE: u32 = 0x8000_0000;
const EXTENDED_FEATURES: u32 = 0x8000_0001;

#[derive(Clone, Copy)]
struct Registers {
    eax: u32,
    ebx: u32,
    ecx: u32,
    edx: u32,
}

/// Capabilities discovered from architecturally specified CPUID bits.
/// These bits are informative; enable paging/APIC features only after the
/// corresponding kernel initialization and safety checks exist.
#[allow(dead_code)]
pub struct CpuFeatures {
    pub apic: bool,
    pub x2apic: bool,
    pub nx: bool,
    pub smep: bool,
    pub smap: bool,
}

/// One CPU's CPUID snapshot. Re-run on each CPU during future SMP startup;
/// this is not a persistent hardware identifier for Vibrix's portable USB.
#[allow(dead_code)]
pub struct CpuInfo {
    pub vendor: [u8; 12],
    pub max_basic_leaf: u32,
    pub max_extended_leaf: u32,
    pub features: CpuFeatures,
}

pub fn discover() -> CpuInfo {
    discover_with(|leaf, subleaf| {
        let registers = __cpuid_count(leaf, subleaf);
        Registers {
            eax: registers.eax,
            ebx: registers.ebx,
            ecx: registers.ecx,
            edx: registers.edx,
        }
    })
}

fn discover_with(mut cpuid: impl FnMut(u32, u32) -> Registers) -> CpuInfo {
    let root = cpuid(0, 0);
    let max_basic_leaf = root.eax;
    let extended_root = cpuid(EXTENDED_BASE, 0);
    let max_extended_leaf = extended_root.eax;

    let basic_features = if max_basic_leaf >= 1 {
        Some(cpuid(1, 0))
    } else {
        None
    };
    let structured_features = if max_basic_leaf >= 7 {
        Some(cpuid(7, 0))
    } else {
        None
    };
    let extended_features = if max_extended_leaf >= EXTENDED_FEATURES {
        Some(cpuid(EXTENDED_FEATURES, 0))
    } else {
        None
    };

    let mut vendor = [0u8; 12];
    vendor[0..4].copy_from_slice(&root.ebx.to_le_bytes());
    vendor[4..8].copy_from_slice(&root.edx.to_le_bytes());
    vendor[8..12].copy_from_slice(&root.ecx.to_le_bytes());

    CpuInfo {
        vendor,
        max_basic_leaf,
        max_extended_leaf,
        features: CpuFeatures {
            apic: basic_features.is_some_and(|r| r.edx & (1 << 9) != 0),
            x2apic: basic_features.is_some_and(|r| r.ecx & (1 << 21) != 0),
            nx: extended_features.is_some_and(|r| r.edx & (1 << 20) != 0),
            smep: structured_features.is_some_and(|r| r.ebx & (1 << 7) != 0),
            smap: structured_features.is_some_and(|r| r.ebx & (1 << 20) != 0),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn intel_root(max_basic_leaf: u32) -> Registers {
        Registers {
            eax: max_basic_leaf,
            ebx: u32::from_le_bytes(*b"Genu"),
            edx: u32::from_le_bytes(*b"ineI"),
            ecx: u32::from_le_bytes(*b"ntel"),
        }
    }

    #[test]
    fn reads_vendor_and_gated_feature_bits() {
        let mut queried = Vec::new();
        let cpu = discover_with(|leaf, subleaf| {
            queried.push((leaf, subleaf));
            match leaf {
                0 => intel_root(7),
                EXTENDED_BASE => Registers {
                    eax: EXTENDED_FEATURES,
                    ebx: 0,
                    ecx: 0,
                    edx: 0,
                },
                1 => Registers {
                    eax: 0,
                    ebx: 0,
                    ecx: 1 << 21,
                    edx: 1 << 9,
                },
                7 => Registers {
                    eax: 0,
                    ebx: (1 << 7) | (1 << 20),
                    ecx: 0,
                    edx: 0,
                },
                EXTENDED_FEATURES => Registers {
                    eax: 0,
                    ebx: 0,
                    ecx: 0,
                    edx: 1 << 20,
                },
                _ => panic!("unexpected CPUID leaf"),
            }
        });
        assert_eq!(&cpu.vendor, b"GenuineIntel");
        assert_eq!(cpu.max_basic_leaf, 7);
        assert_eq!(cpu.max_extended_leaf, EXTENDED_FEATURES);
        assert!(cpu.features.apic);
        assert!(cpu.features.x2apic);
        assert!(cpu.features.nx);
        assert!(cpu.features.smep);
        assert!(cpu.features.smap);
        assert_eq!(
            queried,
            [
                (0, 0),
                (EXTENDED_BASE, 0),
                (1, 0),
                (7, 0),
                (EXTENDED_FEATURES, 0),
            ]
        );
    }

    #[test]
    fn does_not_query_unsupported_leaves_or_invent_features() {
        let mut queried = Vec::new();
        let cpu = discover_with(|leaf, subleaf| {
            queried.push((leaf, subleaf));
            match leaf {
                0 => Registers {
                    eax: 0,
                    ebx: u32::from_le_bytes(*b"Auth"),
                    edx: u32::from_le_bytes(*b"enti"),
                    ecx: u32::from_le_bytes(*b"cAMD"),
                },
                EXTENDED_BASE => Registers {
                    eax: EXTENDED_BASE,
                    ebx: 0,
                    ecx: 0,
                    edx: 0,
                },
                _ => panic!("unsupported leaf queried"),
            }
        });
        assert_eq!(&cpu.vendor, b"AuthenticAMD");
        assert!(!cpu.features.apic);
        assert!(!cpu.features.x2apic);
        assert!(!cpu.features.nx);
        assert!(!cpu.features.smep);
        assert!(!cpu.features.smap);
        assert_eq!(queried, [(0, 0), (EXTENDED_BASE, 0)]);
    }

    #[test]
    fn host_cpu_has_basic_discovery() {
        let cpu = discover();
        assert!(cpu.max_basic_leaf >= 1);
        assert!(cpu.vendor.iter().all(u8::is_ascii_graphic));
    }
}
