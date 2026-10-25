//! Shared hardware diagnostics telemetry port.
//!
//! Implementors query the underlying platform runtime, network stack,
//! flash bootloader, and build metadata for diagnostics snapshot aggregation.

use crate::domain::diagnostics::WifiSsid;

/// Hardware telemetry provider port.
pub trait HardwareInfoDriver {
    /// Return the explicit device type identifier.
    fn device_type(&self) -> &'static str;

    /// Return system uptime in seconds.
    fn uptime_secs(&self) -> u64;

    /// Return available free heap in bytes.
    fn free_heap_bytes(&self) -> usize;

    /// Return total heap size in bytes.
    fn total_heap_bytes(&self) -> usize;

    /// Return Wi-Fi SSID if connected, or None.
    fn wifi_ssid(&self) -> Option<WifiSsid>;

    /// Return Wi-Fi RSSI in dBm if connected, or None.
    fn wifi_rssi(&self) -> Option<i8>;

    /// Return IPv4 address if assigned, or None.
    fn ip_address(&self) -> Option<[u8; 4]>;

    /// Return station MAC address as a 6-byte array.
    fn mac_address(&self) -> [u8; 6];

    /// Return the active boot partition slot identifier (e.g. "ota_0" or "ota_1").
    fn active_slot(&self) -> &'static str;

    /// Return whether the active slot is marked valid in otadata.
    fn is_slot_valid(&self) -> bool;

    /// Return the semantic firmware version string.
    fn firmware_version(&self) -> &'static str;

    /// Return the Git commit hash of the build.
    fn git_commit(&self) -> &'static str;

    /// Return the build timestamp string.
    fn build_date(&self) -> &'static str;
}

impl<T: HardwareInfoDriver> HardwareInfoDriver for &T {
    fn device_type(&self) -> &'static str {
        (*self).device_type()
    }

    fn uptime_secs(&self) -> u64 {
        (*self).uptime_secs()
    }

    fn free_heap_bytes(&self) -> usize {
        (*self).free_heap_bytes()
    }

    fn total_heap_bytes(&self) -> usize {
        (*self).total_heap_bytes()
    }

    fn wifi_ssid(&self) -> Option<WifiSsid> {
        (*self).wifi_ssid()
    }

    fn wifi_rssi(&self) -> Option<i8> {
        (*self).wifi_rssi()
    }

    fn ip_address(&self) -> Option<[u8; 4]> {
        (*self).ip_address()
    }

    fn mac_address(&self) -> [u8; 6] {
        (*self).mac_address()
    }

    fn active_slot(&self) -> &'static str {
        (*self).active_slot()
    }

    fn is_slot_valid(&self) -> bool {
        (*self).is_slot_valid()
    }

    fn firmware_version(&self) -> &'static str {
        (*self).firmware_version()
    }

    fn git_commit(&self) -> &'static str {
        (*self).git_commit()
    }

    fn build_date(&self) -> &'static str {
        (*self).build_date()
    }
}
