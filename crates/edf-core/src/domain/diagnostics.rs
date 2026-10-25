//! System diagnostics domain model.

use crate::domain::ecosystem::{EcosystemId, EcosystemStatus, EcosystemType};
use crate::events::SystemState;

// ============================================================================
// Public Types
// ============================================================================

/// Real-time health, network, flash, ecosystem, and firmware telemetry snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemDiagnostics<D, T: EcosystemType = EcosystemId> {
    pub device_type: &'static str,
    pub state: SystemState,
    pub domain: D,
    pub uptime_sec: u64,
    pub wifi_ssid: Option<WifiSsid>,
    pub wifi_rssi_dbm: Option<i8>,
    pub ip: Option<[u8; 4]>,
    pub mac: [u8; 6],
    pub active_slot: &'static str,
    pub slot_valid: bool,
    pub free_heap_bytes: usize,
    pub total_heap_bytes: usize,
    pub available_ecosystems: &'static [T],
    pub active_ecosystem: Option<EcosystemStatus>,
    pub version: &'static str,
    pub git_commit: &'static str,
    pub build_date: &'static str,
}

/// Up to 32-byte Wi-Fi SSID string (IEEE 802.11 maximum SSID length).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WifiSsid {
    bytes: [u8; 32],
    len: u8,
}

impl WifiSsid {
    /// Create a new `WifiSsid` from a string slice if within 32 bytes.
    pub fn new(s: &str) -> Option<Self> {
        Self::from_bytes(s.as_bytes())
    }

    /// Create a new `WifiSsid` from a byte slice if within 32 bytes.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() > 32 {
            return None;
        }
        let mut buf = [0u8; 32];
        buf[..bytes.len()].copy_from_slice(bytes);
        Some(Self {
            bytes: buf,
            len: bytes.len() as u8,
        })
    }

    /// Return the SSID as a string slice (or empty string if not valid UTF-8).
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len as usize]).unwrap_or("")
    }

    /// Return the raw SSID bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::WifiSsid;

    #[test]
    fn wifi_ssid_preserves_32_byte_capacity() {
        assert!(WifiSsid::from_bytes(&[b'a'; 32]).is_some());
        assert!(WifiSsid::from_bytes(&[b'a'; 33]).is_none());
    }
}
