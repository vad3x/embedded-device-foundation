//! Shared ecosystem info port.
//!
//! Outbound port traits for smart home ecosystem integrations.

use crate::domain::ecosystem::EcosystemStatus;

/// Ecosystem status and telemetry provider.
///
/// Implemented by smart home protocol adapters (e.g. Matter, Home Assistant).
/// Decouples the core domain and diagnostic telemetry from protocol-specific details.
pub trait EcosystemInfoDriver {
    /// Return the current status snapshot of the active ecosystem, or `None` if standalone.
    fn status(&self) -> Option<EcosystemStatus>;
}

impl EcosystemInfoDriver for () {
    fn status(&self) -> Option<EcosystemStatus> {
        None
    }
}

impl<T: EcosystemInfoDriver> EcosystemInfoDriver for &T {
    fn status(&self) -> Option<EcosystemStatus> {
        (**self).status()
    }
}
