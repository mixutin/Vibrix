//! Portable preference policy v1. No storage access or hardware selection.

#[path = "config_encoding.rs"]
mod encoding;

pub const MAX_CONFIG_BYTES: usize = 1024;
pub const MAX_CONFIG_LINES: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    TooLarge,
    OutputTooSmall,
    Encoding,
    Syntax,
    Version,
    Duplicate,
    UnknownKey,
    HardwareKey,
    Value,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Keymap {
    Us,
    Fi,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkPolicy {
    Off,
    Dhcp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogLevel {
    Error,
    Info,
    Debug,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PortableConfig<'a> {
    pub hostname: &'a str,
    pub keymap: Keymap,
    pub network: NetworkPolicy,
    pub log_level: LogLevel,
}

impl Default for PortableConfig<'_> {
    fn default() -> Self {
        Self {
            hostname: "vibrix",
            keymap: Keymap::Us,
            network: NetworkPolicy::Off,
            log_level: LogLevel::Info,
        }
    }
}

fn hostname_valid(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 63
        && bytes[0].is_ascii_alphanumeric()
        && bytes[bytes.len() - 1].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

impl<'a> PortableConfig<'a> {
    /// Parses a bounded, borrowed preference document transactionally. No
    /// field is applied until the entire document validates. There are no
    /// shell expansions, includes, paths, escape sequences or credentials.
    pub fn from_text(input: &'a str) -> Result<Self, Error> {
        if input.len() > MAX_CONFIG_BYTES || input.lines().count() > MAX_CONFIG_LINES {
            return Err(Error::TooLarge);
        }
        if !input.is_ascii()
            || input
                .bytes()
                .any(|byte| byte.is_ascii_control() && !matches!(byte, b'\n' | b'\r' | b'\t'))
        {
            return Err(Error::Encoding);
        }
        let mut config = Self::default();
        let mut seen = 0u8;
        for line in input.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (key, value) = line.split_once('=').ok_or(Error::Syntax)?;
            let key = key.trim();
            let value = value.trim();
            let bit = match key {
                "version" => 1,
                "hostname" => 2,
                "keymap" => 4,
                "network" => 8,
                "log_level" => 16,
                "root" | "root_device" | "pci_address" | "usb_address" | "cpu_count"
                | "driver_binding" => return Err(Error::HardwareKey),
                _ => return Err(Error::UnknownKey),
            };
            if seen & bit != 0 {
                return Err(Error::Duplicate);
            }
            seen |= bit;
            match key {
                "version" if value == "1" => {}
                "version" => return Err(Error::Version),
                "hostname" if hostname_valid(value) => config.hostname = value,
                "keymap" => {
                    config.keymap = match value {
                        "us" => Keymap::Us,
                        "fi" => Keymap::Fi,
                        _ => return Err(Error::Value),
                    };
                }
                "network" => {
                    config.network = match value {
                        "off" => NetworkPolicy::Off,
                        "dhcp" => NetworkPolicy::Dhcp,
                        _ => return Err(Error::Value),
                    };
                }
                "log_level" => {
                    config.log_level = match value {
                        "error" => LogLevel::Error,
                        "info" => LogLevel::Info,
                        "debug" => LogLevel::Debug,
                        _ => return Err(Error::Value),
                    };
                }
                _ => return Err(Error::Value),
            }
        }
        if seen & 1 == 0 {
            return Err(Error::Version);
        }
        Ok(config)
    }

    /// Approval is transient input from this boot's user/policy decision,
    /// never a persisted machine fingerprint or authentication assertion.
    pub fn network_for_boot(self, approved_this_boot: bool) -> NetworkPolicy {
        if approved_this_boot {
            self.network
        } else {
            NetworkPolicy::Off
        }
    }
}

pub fn self_test() -> Result<(), Error> {
    let config = PortableConfig::from_text(
        "version=1\nhostname=portable\nkeymap=fi\nnetwork=dhcp\nlog_level=error\n",
    )?;
    if config.hostname != "portable"
        || config.keymap != Keymap::Fi
        || config.network_for_boot(false) != NetworkPolicy::Off
        || config.network_for_boot(true) != NetworkPolicy::Dhcp
        || PortableConfig::from_text("version=1\nroot=/dev/nvme0n1") != Err(Error::HardwareKey)
    {
        return Err(Error::Value);
    }
    encoding::self_test()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_policy_and_defaults() {
        assert_eq!(self_test(), Ok(()));
        assert_eq!(
            PortableConfig::from_text("version=1"),
            Ok(PortableConfig::default())
        );
    }

    #[test]
    fn version_and_duplicate_keys_fail_closed() {
        for text in ["", "hostname=test", "version=0", "version=2"] {
            assert_eq!(PortableConfig::from_text(text), Err(Error::Version));
        }
        assert_eq!(
            PortableConfig::from_text("version=1\nversion=1"),
            Err(Error::Duplicate)
        );
        assert_eq!(
            PortableConfig::from_text("version=1\nnetwork=off\nnetwork=dhcp"),
            Err(Error::Duplicate)
        );
    }

    #[test]
    fn forbidden_hardware_keys_and_unknown_extensions() {
        for key in [
            "root",
            "root_device",
            "pci_address",
            "usb_address",
            "cpu_count",
            "driver_binding",
        ] {
            let text = std::format!("version=1\n{key}=something");
            assert_eq!(PortableConfig::from_text(&text), Err(Error::HardwareKey));
        }
        assert_eq!(
            PortableConfig::from_text("version=1\npassword=secret"),
            Err(Error::UnknownKey)
        );
    }

    #[test]
    fn document_bounds_and_encoding() {
        assert_eq!(
            PortableConfig::from_text(&" ".repeat(1025)),
            Err(Error::TooLarge)
        );
        assert_eq!(
            PortableConfig::from_text(&"\n".repeat(33)),
            Err(Error::TooLarge)
        );
        assert_eq!(
            PortableConfig::from_text("version=1\0"),
            Err(Error::Encoding)
        );
        assert_eq!(
            PortableConfig::from_text("version=1\nhostname=ä"),
            Err(Error::Encoding)
        );
    }

    #[test]
    fn no_shell_paths_or_hostname_injection() {
        for name in [
            "",
            "-host",
            "host-",
            "../host",
            "$(id)",
            "host.local",
            "HOST",
            "a=b",
        ] {
            let text = std::format!("version=1\nhostname={name}");
            assert_eq!(PortableConfig::from_text(&text), Err(Error::Value));
        }
        assert!(hostname_valid("portable-123"));
        assert!(!hostname_valid(&"a".repeat(64)));
        assert!(PortableConfig::from_text("# comment\r\n version = 1\r\n").is_ok());
    }
}
