//! Shared system reset ports.
//!
//! System reset domain port and scope definitions.

pub use crate::domain::reset::ResetScope;

/// Reset trigger detection port.
#[allow(async_fn_in_trait)]
pub trait ResetTriggerDriver {
    /// Block until a reset trigger condition is met, returning the requested scope.
    async fn wait_for_reset(&mut self) -> ResetScope;
}

/// Outbound port for performing system credential erasure and rebooting the hardware.
pub trait SystemResetDriver {
    /// Erases persistent credentials according to the specified scope.
    fn erase_credentials(&self, scope: ResetScope);

    /// Reboots the hardware controller.
    fn reboot(&self) -> !;
}

impl<T: ResetTriggerDriver> ResetTriggerDriver for &mut T {
    async fn wait_for_reset(&mut self) -> ResetScope {
        (**self).wait_for_reset().await
    }
}

impl ResetTriggerDriver for () {
    async fn wait_for_reset(&mut self) -> ResetScope {
        core::future::pending::<()>().await;
        ResetScope::Factory
    }
}

impl<T: SystemResetDriver> SystemResetDriver for &T {
    fn erase_credentials(&self, scope: ResetScope) {
        (*self).erase_credentials(scope);
    }

    fn reboot(&self) -> ! {
        (*self).reboot();
    }
}

impl SystemResetDriver for () {
    fn erase_credentials(&self, _scope: ResetScope) {}

    fn reboot(&self) -> ! {
        panic!("Reboot requested on null reset driver");
    }
}
