//! Canonical schema-v1 encoding into caller-owned memory; no file I/O.
use super::{
    Error, Keymap, LogLevel, MAX_CONFIG_BYTES, NetworkPolicy, PortableConfig, hostname_valid,
};

impl PortableConfig<'_> {
    /// Serialize all portable preferences in a deterministic key order. Public
    /// fields are revalidated. On error, output is unchanged; on success, only
    /// the returned prefix is written, without a trailing NUL terminator.
    pub fn write_text(&self, output: &mut [u8]) -> Result<usize, Error> {
        if !hostname_valid(self.hostname) {
            return Err(Error::Value);
        }
        let keymap = match self.keymap {
            Keymap::Us => "us",
            Keymap::Fi => "fi",
        };
        let network = match self.network {
            NetworkPolicy::Off => "off",
            NetworkPolicy::Dhcp => "dhcp",
        };
        let log_level = match self.log_level {
            LogLevel::Error => "error",
            LogLevel::Info => "info",
            LogLevel::Debug => "debug",
        };
        let parts = [
            "version=1\nhostname=",
            self.hostname,
            "\nkeymap=",
            keymap,
            "\nnetwork=",
            network,
            "\nlog_level=",
            log_level,
            "\n",
        ];
        let length = parts.iter().try_fold(0usize, |length, part| {
            length.checked_add(part.len()).ok_or(Error::TooLarge)
        })?;
        if length > MAX_CONFIG_BYTES {
            return Err(Error::TooLarge);
        }
        if output.len() < length {
            return Err(Error::OutputTooSmall);
        }
        let mut offset = 0;
        for part in parts {
            let end = offset + part.len();
            output[offset..end].copy_from_slice(part.as_bytes());
            offset = end;
        }
        Ok(length)
    }
}

pub(super) fn self_test() -> Result<(), Error> {
    let config = PortableConfig {
        hostname: "portable-1",
        keymap: Keymap::Fi,
        network: NetworkPolicy::Dhcp,
        log_level: LogLevel::Debug,
    };
    let mut output = [0xa5; 160];
    let length = config.write_text(&mut output)?;
    let text = core::str::from_utf8(&output[..length]).map_err(|_| Error::Encoding)?;
    let decoded = PortableConfig::from_text(text)?;
    if decoded != config
        || decoded.network_for_boot(false) != NetworkPolicy::Off
        || output[length..].iter().any(|byte| *byte != 0xa5)
    {
        return Err(Error::Value);
    }
    let mut short = [0x5a; 1];
    if config.write_text(&mut short) != Err(Error::OutputTooSmall) || short != [0x5a] {
        return Err(Error::Value);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_round_trip() {
        assert_eq!(self_test(), Ok(()));
    }

    #[test]
    fn default_encoding_is_canonical_and_preserves_suffix() {
        let expected = b"version=1\nhostname=vibrix\nkeymap=us\nnetwork=off\nlog_level=info\n";
        let mut output = [0xa5; MAX_CONFIG_BYTES];
        let length = PortableConfig::default().write_text(&mut output).unwrap();
        assert_eq!(length, expected.len());
        assert_eq!(&output[..length], expected);
        assert!(output[length..].iter().all(|byte| *byte == 0xa5));
        let mut exact = [0; 63];
        assert_eq!(expected.len(), exact.len());
        assert_eq!(PortableConfig::default().write_text(&mut exact), Ok(63));
        assert_eq!(&exact, expected);
    }

    #[test]
    fn every_preference_combination_round_trips_without_network_consent() {
        for keymap in [Keymap::Us, Keymap::Fi] {
            for network in [NetworkPolicy::Off, NetworkPolicy::Dhcp] {
                for log_level in [LogLevel::Error, LogLevel::Info, LogLevel::Debug] {
                    let config = PortableConfig {
                        hostname: "portable-1",
                        keymap,
                        network,
                        log_level,
                    };
                    let mut output = [0; MAX_CONFIG_BYTES];
                    let length = config.write_text(&mut output).unwrap();
                    let text = core::str::from_utf8(&output[..length]).unwrap();
                    let decoded = PortableConfig::from_text(text).unwrap();
                    assert_eq!(decoded, config);
                    assert_eq!(decoded.network_for_boot(false), NetworkPolicy::Off);
                    assert_eq!(decoded.network_for_boot(true), network);
                }
            }
        }
    }

    #[test]
    fn every_short_output_is_unchanged() {
        let config = PortableConfig::default();
        let required = config.write_text(&mut [0; MAX_CONFIG_BYTES]).unwrap();
        for length in 0..required {
            let mut output = [0xa5; MAX_CONFIG_BYTES];
            assert_eq!(
                config.write_text(&mut output[..length]),
                Err(Error::OutputTooSmall)
            );
            assert_eq!(output, [0xa5; MAX_CONFIG_BYTES]);
        }
    }

    #[test]
    fn invalid_public_hostname_fields_cannot_inject_records() {
        let long = "a".repeat(64);
        for hostname in ["", "UPPER", "-bad", "bad-", "ä", "a\nroot=x", "a=b", &long] {
            let config = PortableConfig {
                hostname,
                ..PortableConfig::default()
            };
            let mut output = [0xa5; MAX_CONFIG_BYTES];
            assert_eq!(config.write_text(&mut output), Err(Error::Value));
            assert_eq!(output, [0xa5; MAX_CONFIG_BYTES]);
        }
    }

    #[test]
    fn maximum_hostname_and_canonical_idempotence() {
        let hostname = "a".repeat(63);
        let config = PortableConfig {
            hostname: &hostname,
            ..PortableConfig::default()
        };
        let mut first = [0; MAX_CONFIG_BYTES];
        let length = config.write_text(&mut first).unwrap();
        let text = core::str::from_utf8(&first[..length]).unwrap();
        let decoded = PortableConfig::from_text(text).unwrap();
        assert_eq!(decoded, config);
        let mut second = [0; MAX_CONFIG_BYTES];
        assert_eq!(decoded.write_text(&mut second), Ok(length));
        assert_eq!(first, second);
        let commented = PortableConfig::from_text("# settings\r\n version = 1\r\n").unwrap();
        let length = commented.write_text(&mut first).unwrap();
        let defaults = PortableConfig::default().write_text(&mut second).unwrap();
        assert_eq!(&first[..length], &second[..defaults]);
    }
}
