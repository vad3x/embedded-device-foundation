//! Host-side Matter commissioning credentials derived from the shared device identity.

use edf_matter::identity::{derive_device_identity, IdentityError, IdentityParams};
use rs_matter::dm::clusters::basic_info::BasicInfoConfig;
use rs_matter::pairing::qr::{no_optional_data, CommFlowType, QrPayload};
use rs_matter::pairing::DiscoveryCapabilities;

pub struct PairingOptions<'a> {
    pub mac: [u8; 6],
    pub prefix: &'a str,
    pub name: &'a str,
    pub vid: u16,
    pub pid: u16,
}

#[derive(Debug)]
pub enum PairingError {
    InvalidIdentity(IdentityError),
    Qr(rs_matter::error::Error),
}

pub struct PairingData {
    pub mac: String,
    pub serial_number: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub discriminator: u16,
    pub passcode: u32,
    pub manual_code: String,
    pub manual_code_long: String,
    pub qr_code: String,
}

impl PairingData {
    pub fn json(&self) -> String {
        format!(
            "{{\n  \"mac\": \"{}\",\n  \"serial_number\": \"{}\",\n  \"vendor_id\": {},\n  \"product_id\": {},\n  \"discriminator\": {},\n  \"passcode\": {},\n  \"manual_code\": \"{}\",\n  \"manual_code_long\": \"{}\",\n  \"qr_code\": \"{}\"\n}}",
            self.mac, self.serial_number, self.vendor_id, self.product_id, self.discriminator,
            self.passcode, self.manual_code, self.manual_code_long, self.qr_code
        )
    }
}

pub fn parse_mac(input: &str) -> Result<[u8; 6], &'static str> {
    let raw: String = if input.contains(':') || input.contains('-') {
        let separator = if input.contains(':') { ':' } else { '-' };
        let parts: Vec<_> = input.split(separator).collect();
        if parts.len() != 6 || parts.iter().any(|part| part.len() != 2) {
            return Err("MAC must contain six two-digit hex octets");
        }
        parts.concat()
    } else {
        input.to_owned()
    };
    if raw.len() != 12 || !raw.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("MAC must contain exactly twelve hex digits");
    }
    let mut mac = [0; 6];
    for (index, byte) in mac.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&raw[index * 2..index * 2 + 2], 16)
            .map_err(|_| "MAC contains invalid hex")?;
    }
    Ok(mac)
}

pub fn generate(options: PairingOptions<'_>) -> Result<PairingData, PairingError> {
    let identity = derive_device_identity(&IdentityParams {
        mac: &options.mac,
        serial_prefix: options.prefix,
        hostname_prefix: options.name,
    })
    .map_err(PairingError::InvalidIdentity)?;
    let comm_data = identity.comm_data();
    let manual_code = comm_data.compute_pretty_pairing_code().to_string();
    let manual_code_long = comm_data
        .compute_pretty_pairing_code_long(options.vid, options.pid)
        .to_string();
    let basic_info = BasicInfoConfig {
        vid: options.vid,
        pid: options.pid,
        serial_no: identity.serial_str(),
        ..BasicInfoConfig::new()
    };
    let payload = QrPayload::new_from_basic_info(
        DiscoveryCapabilities::BLE,
        CommFlowType::Standard,
        comm_data,
        &basic_info,
        no_optional_data,
    );
    let mut qr_buf = [0; 128];
    let qr_code = payload
        .as_str(&mut qr_buf)
        .map_err(PairingError::Qr)?
        .0
        .to_owned();
    Ok(PairingData {
        mac: options.mac.map(|byte| format!("{byte:02x}")).join(":"),
        serial_number: identity.serial_str().to_owned(),
        vendor_id: options.vid,
        product_id: options.pid,
        discriminator: identity.discriminator,
        passcode: identity.passcode,
        manual_code,
        manual_code_long,
        qr_code,
    })
}

#[cfg(test)]
mod tests {
    use crate::{generate, PairingOptions};
    use rs_matter::pairing::qr::QrPayload;

    const MAC: [u8; 6] = [0x98, 0xa3, 0x16, 0xa6, 0x78, 0x44];

    fn options<'a>(prefix: &'a str, name: &'a str) -> PairingOptions<'a> {
        PairingOptions {
            mac: MAC,
            prefix,
            name,
            vid: 0xfff1,
            pid: 0x8001,
        }
    }

    #[test]
    fn hvac_reference_json_matches_phase_zero() {
        let data = generate(options("HV", "hvac-bridge")).unwrap();
        assert_eq!(data.json(), "{\n  \"mac\": \"98:a3:16:a6:78:44\",\n  \"serial_number\": \"HV-98A316A67844\",\n  \"vendor_id\": 65521,\n  \"product_id\": 32769,\n  \"discriminator\": 3096,\n  \"passcode\": 21710079,\n  \"manual_code\": \"3012-791-3250\",\n  \"manual_code_long\": \"7012-791-3256-55213-27699\",\n  \"qr_code\": \"MT:-24J042C00X1VC4QK3P0OUYH0QN5G1V.X12LJRQ19UZR1OADO1O0\"\n}");
    }

    #[test]
    fn intercom_reference_json_matches_phase_zero() {
        let data = generate(options("IC", "intercom-bridge")).unwrap();
        assert_eq!(data.json(), "{\n  \"mac\": \"98:a3:16:a6:78:44\",\n  \"serial_number\": \"IC-98A316A67844\",\n  \"vendor_id\": 65521,\n  \"product_id\": 32769,\n  \"discriminator\": 3096,\n  \"passcode\": 21710079,\n  \"manual_code\": \"3012-791-3250\",\n  \"manual_code_long\": \"7012-791-3256-55213-27699\",\n  \"qr_code\": \"MT:-24J042C00X1VC4QK3P0OUYH0R92G1V.X12LJRQ19UZR1OADO1O0\"\n}");
    }

    #[test]
    fn qr_round_trip_keeps_commissioning_values() {
        let data = generate(options("HV", "hvac-bridge")).unwrap();
        let mut scratch = [0; 128];
        let parsed = QrPayload::parse(&data.qr_code, &mut scratch).unwrap();
        assert_eq!(
            (
                parsed.passcode(),
                parsed.discriminator(),
                parsed.vid(),
                parsed.pid()
            ),
            (21710079, 3096, 0xfff1, 0x8001)
        );
    }

    #[test]
    fn manual_codes_have_valid_check_digits() {
        let data = generate(options("IC", "intercom-bridge")).unwrap();
        assert!(QrPayload::parse_pairing_code(&data.manual_code.replace('-', "")).is_ok());
        assert!(QrPayload::parse_pairing_code(&data.manual_code_long.replace('-', "")).is_ok());
    }

    #[test]
    fn invalid_prefix_is_rejected() {
        assert!(generate(options("HV!", "hvac-bridge")).is_err());
    }
}
