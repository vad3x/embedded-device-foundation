#![cfg_attr(not(test), no_std)]

pub mod config;
pub mod domain;
pub mod events;
pub mod ports;
pub mod service;

pub use domain::{
    classify_reset_hold, EcosystemId, EcosystemStatus, EcosystemType, EcosystemValue, EndpointInfo,
    FabricInfo, FabricLabel, ResetScope, SystemDiagnostics, WifiSsid, MAX_ECOSYSTEM_ENDPOINTS,
    MAX_ECOSYSTEM_FABRICS, MAX_ECOSYSTEM_PROPERTIES,
};
pub use events::{SystemState, SystemStateReceiver, SystemStateSender, SystemStateWatch};
pub use ports::{
    EcosystemInfoDriver, HardwareInfoDriver, OtaDriverError, OtaFlashDriver, OtaStream,
    ResetTriggerDriver, SystemResetDriver,
};
pub use service::{DiagnosticsService, ResetService};
