//! Shared application services.

pub mod diagnostics;
pub mod reset;

pub use diagnostics::DiagnosticsService;
pub use reset::ResetService;
