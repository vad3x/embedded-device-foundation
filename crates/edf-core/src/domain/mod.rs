//! Device-independent domain records and policies.

pub mod diagnostics;
pub mod ecosystem;
pub mod reset;

pub use diagnostics::{SystemDiagnostics, WifiSsid};
pub use ecosystem::{
    EcosystemId, EcosystemStatus, EcosystemType, EcosystemValue, EndpointInfo, FabricInfo,
    FabricLabel, MAX_ECOSYSTEM_ENDPOINTS, MAX_ECOSYSTEM_FABRICS, MAX_ECOSYSTEM_PROPERTIES,
};
pub use reset::{classify_reset_hold, ResetScope};
