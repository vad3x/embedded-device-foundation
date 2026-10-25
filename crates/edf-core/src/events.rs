//! Shared system lifecycle state observation.

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::watch::{Receiver as WatchReceiver, Watch};

// ============================================================================
// System Lifecycle State
// ============================================================================

/// System lifecycle states
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemState {
    /// Hardware peripherals initializing on boot
    Booting,
    /// Core domain services started, initializing network and ecosystem stack
    Starting,
    /// Device is unpaired/uncommissioned; advertising & awaiting initial pairing
    Commissioning,
    /// Associating with Wi-Fi and acquiring IP lease
    WifiConnecting,
    /// Wi-Fi & LAN operational (HTTP/OTA reachable); awaiting ecosystem controller subscription
    EcosystemConnecting,
    /// All endpoints and ecosystem subscriptions active and operational
    Operational,
    /// Firmware OTA update in progress (flashing inactive partition)
    Updating,
    /// Ecosystem reset triggered (unpairing active ecosystem credentials, keeping Wi-Fi, rebooting)
    EcosystemReset,
    /// Factory reset triggered (erasing persistent credentials, rebooting)
    FactoryReset,
    /// Fatal hardware or protocol fault
    Error,
}

/// Maximum concurrent receivers for system lifecycle state.
pub const STATE_WATCH_RECEIVERS: usize = 3;

/// Continuous lifecycle state watch signal (zero-allocation, zero queue overflow).
pub type SystemStateWatch = Watch<CriticalSectionRawMutex, SystemState, STATE_WATCH_RECEIVERS>;

/// Watch receiver for observing system lifecycle transitions.
pub type SystemStateReceiver<'a> =
    WatchReceiver<'a, CriticalSectionRawMutex, SystemState, STATE_WATCH_RECEIVERS>;

/// Lightweight, Copy handle for sending system lifecycle state updates.
#[derive(Clone, Copy)]
pub struct SystemStateSender<'a>(&'a SystemStateWatch);

impl<'a> SystemStateSender<'a> {
    /// Create a new sender handle wrapping a `SystemStateWatch`.
    pub const fn new(watch: &'a SystemStateWatch) -> Self {
        Self(watch)
    }

    /// Query the current system lifecycle state, if available.
    pub fn current_state(&self) -> Option<SystemState> {
        self.0.try_get()
    }

    /// Send a new system lifecycle state.
    pub fn send(&self, state: SystemState) {
        self.0.sender().send(state);
    }

    /// Convenience helper to transition to `SystemState::Booting`.
    pub fn booting(&self) {
        self.send(SystemState::Booting);
    }

    /// Convenience helper to transition to `SystemState::Starting`.
    pub fn starting(&self) {
        self.send(SystemState::Starting);
    }

    /// Convenience helper to transition to `SystemState::Commissioning`.
    pub fn commissioning(&self) {
        self.send(SystemState::Commissioning);
    }

    /// Convenience helper to transition to `SystemState::WifiConnecting`.
    pub fn wifi_connecting(&self) {
        self.send(SystemState::WifiConnecting);
    }

    /// Convenience helper to transition to `SystemState::EcosystemConnecting`.
    pub fn ecosystem_connecting(&self) {
        self.send(SystemState::EcosystemConnecting);
    }

    /// Convenience helper to transition to `SystemState::Operational`.
    pub fn operational(&self) {
        self.send(SystemState::Operational);
    }

    /// Convenience helper to transition to `SystemState::Updating`.
    pub fn updating(&self) {
        self.send(SystemState::Updating);
    }

    /// Convenience helper to transition to `SystemState::EcosystemReset`.
    pub fn ecosystem_reset(&self) {
        self.send(SystemState::EcosystemReset);
    }

    /// Convenience helper to transition to `SystemState::FactoryReset`.
    pub fn factory_reset(&self) {
        self.send(SystemState::FactoryReset);
    }

    /// Convenience helper to transition to `SystemState::Error`.
    pub fn error(&self) {
        self.send(SystemState::Error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_state_sender_transitions_and_queries_current_state() {
        let watch = SystemStateWatch::new();
        let sender = SystemStateSender::new(&watch);
        assert_eq!(sender.current_state(), None);
        sender.booting();
        assert_eq!(sender.current_state(), Some(SystemState::Booting));
        sender.starting();
        assert_eq!(sender.current_state(), Some(SystemState::Starting));
        sender.commissioning();
        assert_eq!(sender.current_state(), Some(SystemState::Commissioning));
        sender.wifi_connecting();
        assert_eq!(sender.current_state(), Some(SystemState::WifiConnecting));
        sender.ecosystem_connecting();
        assert_eq!(
            sender.current_state(),
            Some(SystemState::EcosystemConnecting)
        );
        sender.operational();
        assert_eq!(sender.current_state(), Some(SystemState::Operational));
        sender.updating();
        assert_eq!(sender.current_state(), Some(SystemState::Updating));
        sender.ecosystem_reset();
        assert_eq!(sender.current_state(), Some(SystemState::EcosystemReset));
        sender.factory_reset();
        assert_eq!(sender.current_state(), Some(SystemState::FactoryReset));
        sender.error();
        assert_eq!(sender.current_state(), Some(SystemState::Error));
    }

    #[test]
    fn receiver_observes_transition() {
        let watch = SystemStateWatch::new();
        let mut receiver = watch.receiver().expect("receiver available");
        SystemStateSender::new(&watch).operational();
        assert_eq!(receiver.try_get(), Some(SystemState::Operational));
    }
}
