//! Shared diagnostics application service.
//!
//! Produces point-in-time system diagnostics snapshots combining hardware telemetry,
//! system lifecycle state, caller-provided domain state, and active ecosystem status.

use crate::domain::diagnostics::SystemDiagnostics;
use crate::domain::ecosystem::{EcosystemId, EcosystemType};
use crate::events::{SystemState, SystemStateWatch};
use crate::ports::diagnostics::HardwareInfoDriver;
use crate::ports::ecosystem::EcosystemInfoDriver;

// ============================================================================
// DiagnosticsService
// ============================================================================

/// Service responsible for aggregating system telemetry into point-in-time snapshots.
pub struct DiagnosticsService<
    'a,
    H: HardwareInfoDriver,
    E: EcosystemInfoDriver = (),
    T: EcosystemType = EcosystemId,
> {
    hardware: &'a H,
    ecosystem: &'a E,
    available_ecosystems: &'static [T],
    state_watch: &'a SystemStateWatch,
}

impl<'a, H: HardwareInfoDriver> DiagnosticsService<'a, H, (), EcosystemId> {
    /// Create a new DiagnosticsService instance without a dedicated smart home ecosystem driver.
    pub fn new(hardware: &'a H, state_watch: &'a SystemStateWatch) -> Self {
        Self {
            hardware,
            ecosystem: &(),
            available_ecosystems: &[],
            state_watch,
        }
    }
}

impl<'a, H: HardwareInfoDriver, E: EcosystemInfoDriver, T: EcosystemType>
    DiagnosticsService<'a, H, E, T>
{
    /// Create a new DiagnosticsService instance with a dedicated smart home ecosystem driver
    /// and a slice of available ecosystems provided by the composition root.
    pub fn with_ecosystem(
        hardware: &'a H,
        ecosystem: &'a E,
        available_ecosystems: &'static [T],
        state_watch: &'a SystemStateWatch,
    ) -> Self {
        Self {
            hardware,
            ecosystem,
            available_ecosystems,
            state_watch,
        }
    }

    /// Aggregate current domain state, ecosystem status, and hardware telemetry into a snapshot.
    pub fn snapshot<D>(&self, domain: D) -> SystemDiagnostics<D, T> {
        let state = self.state_watch.try_get().unwrap_or(SystemState::Booting);

        SystemDiagnostics {
            device_type: self.hardware.device_type(),
            state,
            domain,
            uptime_sec: self.hardware.uptime_secs(),
            wifi_ssid: self.hardware.wifi_ssid(),
            wifi_rssi_dbm: self.hardware.wifi_rssi(),
            ip: self.hardware.ip_address(),
            mac: self.hardware.mac_address(),
            active_slot: self.hardware.active_slot(),
            slot_valid: self.hardware.is_slot_valid(),
            free_heap_bytes: self.hardware.free_heap_bytes(),
            total_heap_bytes: self.hardware.total_heap_bytes(),
            available_ecosystems: self.available_ecosystems,
            active_ecosystem: self.ecosystem.status(),
            version: self.hardware.firmware_version(),
            git_commit: self.hardware.git_commit(),
            build_date: self.hardware.build_date(),
        }
    }

    /// Reference to the underlying hardware info driver.
    pub fn hardware(&self) -> &H {
        self.hardware
    }

    /// Reference to the underlying ecosystem info driver.
    pub fn ecosystem(&self) -> &E {
        self.ecosystem
    }

    /// Reference to the system state watch signal.
    pub fn state_watch(&self) -> &SystemStateWatch {
        self.state_watch
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::diagnostics::WifiSsid;
    use crate::domain::ecosystem::{EcosystemId, EcosystemStatus, EcosystemType};
    use crate::events::{SystemState, SystemStateSender, SystemStateWatch};
    use crate::ports::{EcosystemInfoDriver, HardwareInfoDriver};
    use crate::service::DiagnosticsService;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct DevicePayload {
        marker: u16,
    }

    struct MockHardwareInfoDriver {
        device_type: &'static str,
        uptime_secs: u64,
    }
    impl MockHardwareInfoDriver {
        fn new() -> Self {
            Self {
                device_type: "fixture",
                uptime_secs: 12345,
            }
        }
    }
    impl HardwareInfoDriver for MockHardwareInfoDriver {
        fn device_type(&self) -> &'static str {
            self.device_type
        }
        fn uptime_secs(&self) -> u64 {
            self.uptime_secs
        }
        fn free_heap_bytes(&self) -> usize {
            45000
        }
        fn total_heap_bytes(&self) -> usize {
            80000
        }
        fn wifi_ssid(&self) -> Option<WifiSsid> {
            WifiSsid::new("HomeNet")
        }
        fn wifi_rssi(&self) -> Option<i8> {
            Some(-62)
        }
        fn ip_address(&self) -> Option<[u8; 4]> {
            Some([192, 168, 1, 100])
        }
        fn mac_address(&self) -> [u8; 6] {
            [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc]
        }
        fn active_slot(&self) -> &'static str {
            "ota_0"
        }
        fn is_slot_valid(&self) -> bool {
            true
        }
        fn firmware_version(&self) -> &'static str {
            "0.1.0"
        }
        fn git_commit(&self) -> &'static str {
            "abcdef0"
        }
        fn build_date(&self) -> &'static str {
            "2026-10-17"
        }
    }

    #[test]
    fn diagnostics_accepts_distinct_device_payload_types() {
        let hardware = MockHardwareInfoDriver::new();
        let watch = SystemStateWatch::new();
        let service = DiagnosticsService::new(&hardware, &watch);
        let snapshot = service.snapshot(DevicePayload { marker: 17 });
        assert_eq!(snapshot.domain.marker, 17);
        assert_eq!(snapshot.state, SystemState::Booting);
        assert_eq!(snapshot.device_type, "fixture");
        assert_eq!(snapshot.uptime_sec, hardware.uptime_secs);
        assert!(service.snapshot(true).domain);
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum TestEcosystem {
        Matter,
    }
    impl EcosystemType for TestEcosystem {
        fn id(&self) -> EcosystemId {
            EcosystemId::new("Matter")
        }
    }
    const TEST_ECOSYSTEMS: &[TestEcosystem] = &[TestEcosystem::Matter];
    struct MockEcosystemInfoDriver(EcosystemStatus);
    impl EcosystemInfoDriver for MockEcosystemInfoDriver {
        fn status(&self) -> Option<EcosystemStatus> {
            Some(self.0)
        }
    }

    #[test]
    fn diagnostics_preserves_explicit_ecosystem_and_live_state() {
        let hardware = MockHardwareInfoDriver::new();
        let expected = EcosystemStatus::new(TestEcosystem::Matter.id(), true);
        let ecosystem = MockEcosystemInfoDriver(expected);
        let watch = SystemStateWatch::new();
        SystemStateSender::new(&watch).operational();
        let service =
            DiagnosticsService::with_ecosystem(&hardware, &ecosystem, TEST_ECOSYSTEMS, &watch);
        let snapshot = service.snapshot(42u16);
        assert_eq!(snapshot.domain, 42);
        assert_eq!(snapshot.state, SystemState::Operational);
        assert_eq!(snapshot.available_ecosystems, TEST_ECOSYSTEMS);
        assert_eq!(snapshot.active_ecosystem, Some(expected));
    }
}
