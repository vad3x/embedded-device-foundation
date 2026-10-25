use std::fs;
use std::process::{Command, ExitCode};

use edf_pairing_tool::{generate, parse_mac, PairingOptions};
use rs_matter::pairing::qr::{Qr, QrTextType};

const USAGE: &str = "Usage: cargo qr --prefix PREFIX --name NAME [--mac MAC | --port PORT] [--json] [--vid ID] [--pid ID]\n\nOptions:\n  --prefix PREFIX  Serial prefix (for example HV or IC)\n  --name NAME      Hostname prefix and display name\n  --mac, -m MAC    Six-byte device MAC address\n  --port, -p PORT  Serial port for espflash board-info\n  --json           Output Phase 0 compatible JSON\n  --vid ID         Vendor ID, decimal or 0x hex (default 0xfff1)\n  --pid ID         Product ID, decimal or 0x hex (default 0x8001)\n  -h, --help       Show this help\n";

fn find_serial_port() -> Option<String> {
    let mut ports: Vec<_> = fs::read_dir("/dev")
        .ok()?
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.starts_with("cu.usbmodem") || name.starts_with("cu.usbserial"))
        .map(|name| format!("/dev/{name}"))
        .collect();
    ports.sort();
    ports.into_iter().next()
}

fn query_mac(port: &str) -> Result<String, String> {
    let output = Command::new("espflash")
        .args(["board-info", "--port", port])
        .output()
        .or_else(|_| {
            Command::new("mise")
                .args(["exec", "--", "espflash", "board-info", "--port", port])
                .output()
        })
        .map_err(|error| format!("Unable to run espflash: {error}"))?;
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if !output.status.success() {
        return Err(format!("espflash board-info failed: {combined}"));
    }
    combined
        .lines()
        .find_map(|line| {
            line.split_once("MAC address:")
                .map(|(_, mac)| mac.trim().to_owned())
        })
        .filter(|mac| !mac.is_empty())
        .ok_or_else(|| "espflash did not report a MAC address".to_owned())
}

fn parse_id(value: &str) -> Result<u16, String> {
    if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        u16::from_str_radix(hex, 16).map_err(|_| format!("Invalid 16-bit ID: {value}"))
    } else {
        value
            .parse()
            .map_err(|_| format!("Invalid 16-bit ID: {value}"))
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let mut prefix = None;
    let mut name = None;
    let mut mac = None;
    let mut port = None;
    let mut json = false;
    let mut vid = 0xfff1;
    let mut pid = 0x8001;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return Ok(());
            }
            "--prefix" => prefix = Some(args.next().ok_or("--prefix requires a value")?),
            "--name" => name = Some(args.next().ok_or("--name requires a value")?),
            "--mac" | "-m" => mac = Some(args.next().ok_or("--mac requires a value")?),
            "--port" | "-p" => port = Some(args.next().ok_or("--port requires a value")?),
            "--json" => json = true,
            "--vid" => vid = parse_id(&args.next().ok_or("--vid requires a value")?)?,
            "--pid" => pid = parse_id(&args.next().ok_or("--pid requires a value")?)?,
            "--" => {}
            _ => return Err(format!("Unknown argument: {arg}")),
        }
    }
    let prefix = prefix.ok_or("--prefix is required")?;
    let name = name.ok_or("--name is required")?;
    let mac_text = match mac {
        Some(mac) => mac,
        None => query_mac(
            &port
                .or_else(find_serial_port)
                .ok_or("No serial port found; use --mac")?,
        )?,
    };
    let data = generate(PairingOptions {
        mac: parse_mac(&mac_text).map_err(str::to_owned)?,
        prefix: &prefix,
        name: &name,
        vid,
        pid,
    })
    .map_err(|error| format!("Invalid identity or QR payload: {error:?}"))?;
    if json {
        println!("{}", data.json());
        return Ok(());
    }
    println!("{name} — Matter Commissioning Credentials\n");
    println!("Device MAC: {}", data.mac);
    println!("Serial number: {}", data.serial_number);
    println!("Vendor / product: {:#06x} / {:#06x}", vid, pid);
    println!("Discriminator: {}", data.discriminator);
    println!("Passcode: {}", data.passcode);
    println!("Manual code: {}", data.manual_code);
    println!("Long manual code: {}", data.manual_code_long);
    println!("QR payload: {}", data.qr_code);
    let mut scratch = [0; 1024];
    let mut output = [0; 1024];
    if let Ok(qr) = Qr::compute(&data.qr_code, &mut scratch, &mut output) {
        for y in qr.lines_range(QrTextType::Unicode, 2) {
            let mut line = [0; 512];
            if let Ok((text, _)) =
                qr.line_as_str(QrTextType::Unicode, 2, false, false, y, &mut line)
            {
                println!("  {text}");
            }
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Error: {error}\n\n{USAGE}");
            ExitCode::FAILURE
        }
    }
}
