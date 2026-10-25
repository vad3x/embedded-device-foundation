//! Deterministic, fixed-capacity Matter commissioning identity.

use rs_matter::BasicCommData;

const HEX_UPPER: &[u8; 16] = b"0123456789ABCDEF";
const HEX_LOWER: &[u8; 16] = b"0123456789abcdef";
const MIN_PASSCODE: u32 = 10_000_001;
const MAX_PASSCODE: u32 = 99_999_998;
const FORBIDDEN_PASSCODES: [u32; 10] = [
    11_111_111, 22_222_222, 33_333_333, 44_444_444, 55_555_555, 66_666_666, 77_777_777, 88_888_888,
    12_345_678, 87_654_321,
];

/// Inputs for deterministic identity derivation.
pub struct IdentityParams<'a> {
    pub mac: &'a [u8; 6],
    pub serial_prefix: &'a str,
    pub hostname_prefix: &'a str,
}

/// Invalid caller-supplied identity prefix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentityError {
    EmptyPrefix,
    InvalidPrefix,
    PrefixTooLong,
}

/// Fixed-capacity identity suitable for no-heap firmware composition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceIdentity {
    pub passcode: u32,
    pub discriminator: u16,
    pub serial_no: [u8; 32],
    pub serial_len: usize,
    pub hostname: [u8; 32],
    pub hostname_len: usize,
}

impl DeviceIdentity {
    pub fn serial_str(&self) -> &str {
        // Prefix validation and hex formatting only write ASCII bytes.
        core::str::from_utf8(&self.serial_no[..self.serial_len]).unwrap_or("")
    }

    pub fn hostname_str(&self) -> &str {
        core::str::from_utf8(&self.hostname[..self.hostname_len]).unwrap_or("")
    }

    pub fn comm_data(&self) -> BasicCommData {
        BasicCommData {
            password: self.passcode.to_le_bytes().into(),
            discriminator: self.discriminator,
        }
    }
}

/// Derive commissioning values and validated serial/hostname buffers from a MAC.
pub fn derive_device_identity(
    params: &IdentityParams<'_>,
) -> Result<DeviceIdentity, IdentityError> {
    validate_prefix(params.serial_prefix, 19)?;
    validate_prefix(params.hostname_prefix, 27)?;
    let serial_prefix = params.serial_prefix.as_bytes();
    let hostname_prefix = params.hostname_prefix.as_bytes();
    let mut serial_no = [0; 32];
    serial_no[..serial_prefix.len()].copy_from_slice(serial_prefix);
    serial_no[serial_prefix.len()] = b'-';
    for (i, byte) in params.mac.iter().copied().enumerate() {
        serial_no[serial_prefix.len() + 1 + 2 * i] = HEX_UPPER[(byte >> 4) as usize];
        serial_no[serial_prefix.len() + 2 + 2 * i] = HEX_UPPER[(byte & 15) as usize];
    }

    let mut hostname = [0; 32];
    hostname[..hostname_prefix.len()].copy_from_slice(hostname_prefix);
    hostname[hostname_prefix.len()] = b'-';
    for (i, byte) in params.mac[4..].iter().copied().enumerate() {
        hostname[hostname_prefix.len() + 1 + 2 * i] = HEX_LOWER[(byte >> 4) as usize];
        hostname[hostname_prefix.len() + 2 + 2 * i] = HEX_LOWER[(byte & 15) as usize];
    }
    Ok(DeviceIdentity {
        passcode: passcode_from_mac(params.mac),
        discriminator: discriminator_from_mac(params.mac),
        serial_no,
        serial_len: serial_prefix.len() + 13,
        hostname,
        hostname_len: hostname_prefix.len() + 5,
    })
}

fn validate_prefix(prefix: &str, max_len: usize) -> Result<(), IdentityError> {
    if prefix.is_empty() {
        return Err(IdentityError::EmptyPrefix);
    }
    if prefix.len() > max_len {
        return Err(IdentityError::PrefixTooLong);
    }
    if !prefix
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(IdentityError::InvalidPrefix);
    }
    Ok(())
}

fn mac_hash(mac: &[u8; 6]) -> u64 {
    mac.iter().fold(0xcbf29ce484222325_u64, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

fn passcode_from_mac(mac: &[u8; 6]) -> u32 {
    let span = u64::from(MAX_PASSCODE - MIN_PASSCODE + 1);
    let mut code = MIN_PASSCODE + (mac_hash(mac) % span) as u32;
    while FORBIDDEN_PASSCODES.contains(&code) {
        code = if code == MAX_PASSCODE {
            MIN_PASSCODE
        } else {
            code + 1
        };
    }
    code
}

fn discriminator_from_mac(mac: &[u8; 6]) -> u16 {
    let mut discriminator = ((mac_hash(mac) >> 32) as u16) & 0x0fff;
    if discriminator == 0 || discriminator == 3840 {
        discriminator = (discriminator + 1) & 0x0fff;
        if discriminator == 0 {
            discriminator = 1;
        }
    }
    discriminator
}

#[cfg(test)]
mod tests {
    use crate::identity::{derive_device_identity, IdentityParams};

    const MAC: [u8; 6] = [0x98, 0xa3, 0x16, 0xa6, 0x78, 0x44];

    #[test]
    fn known_hvac_identity_keeps_pairing_vectors() {
        let identity = derive_device_identity(&IdentityParams {
            mac: &MAC,
            serial_prefix: "HV",
            hostname_prefix: "hvac-bridge",
        })
        .unwrap();
        assert_eq!(
            (identity.passcode, identity.discriminator),
            (21_710_079, 3096)
        );
        assert_eq!(identity.serial_str(), "HV-98A316A67844");
        assert_eq!(identity.hostname_str(), "hvac-bridge-7844");
        assert_eq!(
            identity.comm_data().password.access(),
            &21_710_079_u32.to_le_bytes()
        );
    }

    #[test]
    fn known_intercom_identity_keeps_pairing_vectors() {
        let identity = derive_device_identity(&IdentityParams {
            mac: &MAC,
            serial_prefix: "IC",
            hostname_prefix: "intercom-bridge",
        })
        .unwrap();
        assert_eq!(identity.serial_str(), "IC-98A316A67844");
        assert_eq!(identity.hostname_str(), "intercom-bridge-7844");
    }

    #[test]
    fn prefixes_are_validated_before_fixed_buffer_formatting() {
        for prefix in ["", "a b", "é", "a.b"] {
            assert!(derive_device_identity(&IdentityParams {
                mac: &MAC,
                serial_prefix: prefix,
                hostname_prefix: "device"
            })
            .is_err());
            assert!(derive_device_identity(&IdentityParams {
                mac: &MAC,
                serial_prefix: "DV",
                hostname_prefix: prefix
            })
            .is_err());
        }
        assert!(derive_device_identity(&IdentityParams {
            mac: &MAC,
            serial_prefix: "ABCDEFGHIJKLMNOPQRST",
            hostname_prefix: "device"
        })
        .is_err());
        assert!(derive_device_identity(&IdentityParams {
            mac: &MAC,
            serial_prefix: "DV",
            hostname_prefix: "ABCDEFGHIJKLMNOPQRSTUVWXYZ12"
        })
        .is_err());
    }

    #[test]
    fn maximum_prefix_lengths_fit_exactly() {
        let identity = derive_device_identity(&IdentityParams {
            mac: &MAC,
            serial_prefix: "ABCDEFGHIJKLMNOPQRS",
            hostname_prefix: "ABCDEFGHIJKLMNOPQRSTUVWXYZ1",
        })
        .unwrap();
        assert_eq!(identity.serial_len, 32);
        assert_eq!(identity.hostname_len, 32);
    }
}
