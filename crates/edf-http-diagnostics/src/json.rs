use core::fmt::{self, Write};
use edf_core::domain::diagnostics::SystemDiagnostics;
use edf_core::domain::ecosystem::{EcosystemType, EcosystemValue};
use edf_core::events::SystemState;

/// Device-owned diagnostic fields written as complete comma-prefixed JSON fields.
pub trait DiagnosticPayload {
    fn write_fields<W: Write>(&self, out: &mut W) -> fmt::Result;
}

impl DiagnosticPayload for () {
    fn write_fields<W: Write>(&self, _: &mut W) -> fmt::Result {
        Ok(())
    }
}

/// Write one JSON string, including surrounding quotation marks.
pub fn write_json_string<W: Write>(out: &mut W, value: &str) -> fmt::Result {
    out.write_char('"')?;
    for ch in value.chars() {
        match ch {
            '"' => out.write_str("\\\"")?,
            '\\' => out.write_str("\\\\")?,
            '\n' => out.write_str("\\n")?,
            '\r' => out.write_str("\\r")?,
            '\t' => out.write_str("\\t")?,
            '\u{0008}' => out.write_str("\\b")?,
            '\u{000c}' => out.write_str("\\f")?,
            c if c <= '\u{001f}' => write!(out, "\\u{:04x}", c as u32)?,
            c => out.write_char(c)?,
        }
    }
    out.write_char('"')
}

fn state_name(state: SystemState) -> &'static str {
    match state {
        SystemState::Booting => "Booting",
        SystemState::Starting => "Starting",
        SystemState::Commissioning => "Commissioning",
        SystemState::WifiConnecting => "WifiConnecting",
        SystemState::EcosystemConnecting => "EcosystemConnecting",
        SystemState::Operational => "Operational",
        SystemState::Updating => "Updating",
        SystemState::EcosystemReset => "EcosystemReset",
        SystemState::FactoryReset => "FactoryReset",
        SystemState::Error => "Error",
    }
}

/// Serialize the shared diagnostics schema and caller-owned domain fields.
pub fn write_status_json<D: DiagnosticPayload, T: EcosystemType, W: Write>(
    diag: &SystemDiagnostics<D, T>,
    out: &mut W,
) -> fmt::Result {
    out.write_str("{\"device_type\":")?;
    write_json_string(out, diag.device_type)?;
    out.write_str(",\"state\":")?;
    write_json_string(out, state_name(diag.state))?;
    write!(out, ",\"uptime_sec\":{},\"wifi_ssid\":", diag.uptime_sec)?;
    match &diag.wifi_ssid {
        Some(ssid) => write_json_string(out, ssid.as_str())?,
        None => out.write_str("null")?,
    }
    out.write_str(",\"wifi_rssi_dbm\":")?;
    match diag.wifi_rssi_dbm {
        Some(rssi) => write!(out, "{rssi}")?,
        None => out.write_str("null")?,
    }
    out.write_str(",\"ip\":")?;
    match diag.ip {
        Some([a, b, c, d]) => write!(out, "\"{a}.{b}.{c}.{d}\"")?,
        None => out.write_str("null")?,
    }
    let [a, b, c, d, e, f] = diag.mac;
    write!(
        out,
        ",\"mac\":\"{a:02X}:{b:02X}:{c:02X}:{d:02X}:{e:02X}:{f:02X}\",\"active_slot\":"
    )?;
    write_json_string(out, diag.active_slot)?;
    write!(out, ",\"slot_valid\":{},\"free_heap_bytes\":{},\"total_heap_bytes\":{},\"available_ecosystems\":[", diag.slot_valid, diag.free_heap_bytes, diag.total_heap_bytes)?;
    for (index, ecosystem) in diag.available_ecosystems.iter().enumerate() {
        if index != 0 {
            out.write_char(',')?;
        }
        write_json_string(out, ecosystem.id().as_str())?;
    }
    out.write_str("],\"active_ecosystem\":")?;
    if let Some(active) = &diag.active_ecosystem {
        out.write_str("{\"type\":")?;
        write_json_string(out, active.ecosystem_type.as_str())?;
        write!(out, ",\"paired\":{},\"properties\":{{", active.paired)?;
        for (index, (key, value)) in active.iter_properties().enumerate() {
            if index != 0 {
                out.write_char(',')?;
            }
            write_json_string(out, key)?;
            out.write_char(':')?;
            match value {
                EcosystemValue::Bool(b) => write!(out, "{b}")?,
                EcosystemValue::Int(n) => write!(out, "{n}")?,
                EcosystemValue::Text(s) => write_json_string(out, s)?,
            }
        }
        out.write_str("},\"fabrics\":[")?;
        for (index, fabric) in active.iter_fabrics().enumerate() {
            if index != 0 {
                out.write_char(',')?;
            }
            write!(out, "{{\"fab_idx\":{},\"vendor_id\":{},\"fabric_id\":\"0x{:016X}\",\"node_id\":\"0x{:016X}\",\"label\":", fabric.fab_idx, fabric.vendor_id, fabric.fabric_id, fabric.node_id)?;
            write_json_string(out, fabric.label.as_str())?;
            out.write_char('}')?;
        }
        out.write_str("],\"endpoints\":[")?;
        for (index, endpoint) in active.iter_endpoints().enumerate() {
            if index != 0 {
                out.write_char(',')?;
            }
            write!(out, "{{\"id\":{},\"name\":", endpoint.id)?;
            write_json_string(out, endpoint.name)?;
            out.write_str(",\"cluster\":")?;
            write_json_string(out, endpoint.cluster)?;
            write!(out, ",\"ready\":{}}}", endpoint.ready)?;
        }
        out.write_str("]}")?;
    } else {
        out.write_str("null")?;
    }
    diag.domain.write_fields(out)?;
    out.write_str(",\"version\":")?;
    write_json_string(out, diag.version)?;
    out.write_str(",\"git_commit\":")?;
    write_json_string(out, diag.git_commit)?;
    out.write_str(",\"build_date\":")?;
    write_json_string(out, diag.build_date)?;
    out.write_char('}')
}

/// Format into a caller-owned fixed buffer; returns an error on overflow.
pub fn format_status_json<D: DiagnosticPayload, T: EcosystemType>(
    diag: &SystemDiagnostics<D, T>,
    out: &mut [u8],
) -> Result<usize, fmt::Error> {
    let mut writer = BufferWriter {
        buffer: out,
        written: 0,
    };
    write_status_json(diag, &mut writer)?;
    Ok(writer.written)
}

struct BufferWriter<'a> {
    buffer: &'a mut [u8],
    written: usize,
}
impl Write for BufferWriter<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let end = self.written.checked_add(s.len()).ok_or(fmt::Error)?;
        let target = self.buffer.get_mut(self.written..end).ok_or(fmt::Error)?;
        target.copy_from_slice(s.as_bytes());
        self.written = end;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use crate::{format_status_json, write_json_string, DiagnosticPayload};
    use core::fmt::Write;
    use edf_core::domain::diagnostics::{SystemDiagnostics, WifiSsid};
    use edf_core::domain::ecosystem::{
        EcosystemId, EcosystemStatus, EcosystemValue, EndpointInfo, FabricInfo, FabricLabel,
    };
    use edf_core::events::SystemState;
    use serde_json::Value;
    use std::string::String;

    struct Hvac;
    impl DiagnosticPayload for Hvac {
        fn write_fields<W: Write>(&self, out: &mut W) -> core::fmt::Result {
            out.write_str(",\"hvac\":{\"connected\":true}")
        }
    }

    struct Door;
    impl DiagnosticPayload for Door {
        fn write_fields<W: Write>(&self, out: &mut W) -> core::fmt::Result {
            out.write_str(",\"door_locked\":true")
        }
    }

    const ECOSYSTEMS: &[EcosystemId] = &[EcosystemId::new("ma\"tter")];

    fn snapshot<D>(domain: D) -> SystemDiagnostics<D> {
        SystemDiagnostics {
            device_type: "air\"conditioner",
            state: SystemState::Operational,
            domain,
            uptime_sec: 42,
            wifi_ssid: WifiSsid::new("Hi\\\n\0雪"),
            wifi_rssi_dbm: Some(-60),
            ip: Some([192, 168, 1, 7]),
            mac: [0, 1, 2, 0xAA, 0xBB, 0xCC],
            active_slot: "ota_1",
            slot_valid: true,
            free_heap_bytes: 100,
            total_heap_bytes: 200,
            available_ecosystems: ECOSYSTEMS,
            active_ecosystem: Some(
                EcosystemStatus::new(EcosystemId::new("mat\\ter"), true)
                    .with_property("key\"\n", EcosystemValue::Text("va\0lue雪"))
                    .with_fabric(FabricInfo {
                        fab_idx: 1,
                        vendor_id: 2,
                        fabric_id: 0xA,
                        node_id: 0xB,
                        label: FabricLabel::from_str("La\"bel"),
                    })
                    .with_endpoint(EndpointInfo {
                        id: 1,
                        name: "Name\n",
                        cluster: "Cl\\uster",
                        ready: true,
                    }),
            ),
            version: "v\"1",
            git_commit: "ab\\cd",
            build_date: "2026\n10",
        }
    }

    #[test]
    fn common_schema_escapes_every_string_and_preserves_device_fields() {
        {
            let diag = snapshot(Hvac);
            let mut buf = [0; 2048];
            let len = format_status_json(&diag, &mut buf).unwrap();
            let value: Value = serde_json::from_slice(&buf[..len]).unwrap();
            assert_eq!(value["hvac"]["connected"], true);
            assert_eq!(value["wifi_ssid"], "Hi\\\n\0雪");
            assert_eq!(value["device_type"], "air\"conditioner");
            assert_eq!(
                value["active_ecosystem"]["properties"]["key\"\n"],
                "va\0lue雪"
            );
            assert_eq!(value["active_ecosystem"]["endpoints"][0]["name"], "Name\n");
            assert_eq!(
                value["active_ecosystem"]["endpoints"][0]["cluster"],
                "Cl\\uster"
            );
            assert_eq!(
                value["active_ecosystem"]["fabrics"][0]["fabric_id"],
                "0x000000000000000A"
            );
            assert_eq!(
                value["active_ecosystem"]["fabrics"][0]["node_id"],
                "0x000000000000000B"
            );
            assert_eq!(value["mac"], "00:01:02:AA:BB:CC");
        }
        let mut buf = [0; 2048];
        let len = format_status_json(&snapshot(Door), &mut buf).unwrap();
        let value: Value = serde_json::from_slice(&buf[..len]).unwrap();
        assert_eq!(value["door_locked"], true);
    }

    #[test]
    fn nulls_and_capacity_overflow_are_reported() {
        let mut diag = snapshot(());
        diag.wifi_ssid = None;
        diag.ip = None;
        diag.active_ecosystem = None;
        let mut buf = [0; 2048];
        let len = format_status_json(&diag, &mut buf).unwrap();
        let value: Value = serde_json::from_slice(&buf[..len]).unwrap();
        assert!(value["wifi_ssid"].is_null());
        assert!(value["ip"].is_null());
        assert!(value["active_ecosystem"].is_null());
        assert!(format_status_json(&diag, &mut [0; 8]).is_err());
    }

    #[test]
    fn string_writer_escapes_all_control_bytes_and_keeps_utf8() {
        let mut output = String::new();
        write_json_string(&mut output, "\"\\\n\r\t\0\u{001f}雪").unwrap();
        let decoded: String = serde_json::from_str(&output).unwrap();
        assert_eq!(decoded, "\"\\\n\r\t\0\u{001f}雪");
    }

    #[test]
    fn maximum_ecosystem_property_fabric_and_endpoint_capacity_serializes() {
        let mut diag = snapshot(());
        let mut status = EcosystemStatus::new(EcosystemId::new("matter"), true);
        for key in ["one", "two", "three", "four", "five", "six"] {
            status = status.with_property(key, EcosystemValue::Int(1));
        }
        for index in 0..5 {
            status = status.with_fabric(FabricInfo {
                fab_idx: index + 1,
                vendor_id: 1,
                fabric_id: 1,
                node_id: 2,
                label: FabricLabel::empty(),
            });
        }
        for index in 0..4 {
            status = status.with_endpoint(EndpointInfo {
                id: index + 1,
                name: "endpoint",
                cluster: "cluster",
                ready: true,
            });
        }
        diag.active_ecosystem = Some(status);
        let mut buf = [0; 4096];
        let len = format_status_json(&diag, &mut buf).unwrap();
        let value: Value = serde_json::from_slice(&buf[..len]).unwrap();
        assert_eq!(
            value["active_ecosystem"]["properties"]
                .as_object()
                .unwrap()
                .len(),
            6
        );
        assert_eq!(
            value["active_ecosystem"]["fabrics"]
                .as_array()
                .unwrap()
                .len(),
            5
        );
        assert_eq!(
            value["active_ecosystem"]["endpoints"]
                .as_array()
                .unwrap()
                .len(),
            4
        );
    }
}
