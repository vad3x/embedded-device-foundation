use std::process::Command;

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_edf-pairing-tool"))
        .args(args)
        .output()
        .unwrap()
}

fn reference_json(serial: &str, qr: &str) -> String {
    format!(
        "{{\n  \"mac\": \"98:a3:16:a6:78:44\",\n  \"serial_number\": \"{serial}\",\n  \"vendor_id\": 65521,\n  \"product_id\": 32769,\n  \"discriminator\": 3096,\n  \"passcode\": 21710079,\n  \"manual_code\": \"3012-791-3250\",\n  \"manual_code_long\": \"7012-791-3256-55213-27699\",\n  \"qr_code\": \"{qr}\"\n}}\n"
    )
}

#[test]
fn help_succeeds_without_hardware() {
    let output = run(&["--help"]);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("--prefix"));
}

#[test]
fn malformed_mac_fails_without_hardware() {
    let output = run(&[
        "--prefix",
        "HV",
        "--name",
        "hvac-bridge",
        "--mac",
        "98:a3:16:a6:78:ZZ",
        "--json",
    ]);
    assert!(!output.status.success());
}

#[test]
fn missing_prefix_fails_without_hardware() {
    let output = run(&["--mac", "98:a3:16:a6:78:44", "--json"]);
    assert!(!output.status.success());
}

#[test]
fn hvac_binary_json_matches_reference() {
    let output = run(&[
        "--prefix",
        "HV",
        "--name",
        "hvac-bridge",
        "--mac",
        "98:a3:16:a6:78:44",
        "--json",
    ]);
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        reference_json(
            "HV-98A316A67844",
            "MT:-24J042C00X1VC4QK3P0OUYH0QN5G1V.X12LJRQ19UZR1OADO1O0"
        )
    );
}

#[test]
fn intercom_binary_json_matches_reference() {
    let output = run(&[
        "--prefix",
        "IC",
        "--name",
        "intercom-bridge",
        "--mac",
        "98:a3:16:a6:78:44",
        "--json",
    ]);
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        reference_json(
            "IC-98A316A67844",
            "MT:-24J042C00X1VC4QK3P0OUYH0R92G1V.X12LJRQ19UZR1OADO1O0"
        )
    );
}

#[test]
fn invalid_prefix_fails_without_hardware() {
    let output = run(&[
        "--prefix",
        "HV!",
        "--name",
        "hvac-bridge",
        "--mac",
        "98:a3:16:a6:78:44",
        "--json",
    ]);
    assert!(!output.status.success());
}
