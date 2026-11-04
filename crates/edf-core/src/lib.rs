#![cfg_attr(not(test), no_std)]

pub mod config;
pub mod domain;
pub mod events;
pub mod ports;
pub mod service;

pub use domain::{
    classify_reset_hold, ClusterInfo, EcosystemId, EcosystemStatus, EcosystemType, EcosystemValue,
    EndpointInfo, FabricInfo, FabricLabel, FabricSubscription, ResetScope, SystemDiagnostics,
    WifiSsid, MAX_CLUSTERS_PER_ENDPOINT, MAX_ECOSYSTEM_ENDPOINTS, MAX_ECOSYSTEM_FABRICS,
    MAX_ECOSYSTEM_PROPERTIES, MAX_FABRIC_SUBSCRIPTIONS,
};
pub use events::{SystemState, SystemStateReceiver, SystemStateSender, SystemStateWatch};
pub use ports::{
    EcosystemInfoDriver, HardwareInfoDriver, OtaDriverError, OtaFlashDriver, OtaStream,
    ResetTriggerDriver, SystemResetDriver,
};
pub use service::{DiagnosticsService, ResetService};
