//! Outbound capability contracts implemented by device adapters.

pub mod diagnostics;
pub mod ecosystem;
pub mod ota;
pub mod reset;

pub use diagnostics::HardwareInfoDriver;
pub use ecosystem::EcosystemInfoDriver;
pub use ota::{OtaDriverError, OtaFlashDriver, OtaStream};
pub use reset::{ResetScope, ResetTriggerDriver, SystemResetDriver};
