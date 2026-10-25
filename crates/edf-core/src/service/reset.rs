//! Shared system reset service.
//!
//! Coordinates reset hold times with `SystemStateSender` and `SystemResetDriver`.

use embassy_time::{Duration, Timer};
use log::info;

use crate::config::FACTORY_RESET_REBOOT_DELAY_MS;
use crate::events::SystemStateSender;
use crate::ports::reset::{ResetScope, ResetTriggerDriver, SystemResetDriver};

// ============================================================================
// ResetService
// ============================================================================

/// System reset lifecycle service.
pub struct ResetService<T: ResetTriggerDriver, R: SystemResetDriver> {
    pub driver: T,
    pub reset: R,
    reboot_delay: Duration,
}

impl<T: ResetTriggerDriver, R: SystemResetDriver> ResetService<T, R> {
    /// Create a new ResetService from trigger and reset drivers.
    pub fn new(driver: T, reset: R) -> Self {
        Self {
            driver,
            reset,
            reboot_delay: Duration::from_millis(FACTORY_RESET_REBOOT_DELAY_MS),
        }
    }

    /// Set the delay after erasing credentials and before rebooting.
    pub fn with_reboot_delay(mut self, reboot_delay: Duration) -> Self {
        self.reboot_delay = reboot_delay;
        self
    }

    /// Run the reset monitoring loop. Awaits trigger, publishes state, erases credentials,
    /// waits the reboot delay, and triggers hardware reboot.
    pub async fn run(&mut self, state_sender: SystemStateSender<'_>) {
        info!("monitoring for reset trigger");

        let scope = self.driver.wait_for_reset().await;

        match scope {
            ResetScope::Ecosystem => {
                info!("=== ECOSYSTEM RESET TRIGGERED ===");
                state_sender.ecosystem_reset();
            }
            ResetScope::Factory => {
                info!("=== FACTORY RESET TRIGGERED ===");
                state_sender.factory_reset();
            }
        }

        self.reset.erase_credentials(scope);

        Timer::after(self.reboot_delay).await;

        info!("Rebooting...");
        self.reset.reboot();
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::ResetScope;
    use crate::events::{SystemState, SystemStateSender, SystemStateWatch};
    use crate::ports::{ResetTriggerDriver, SystemResetDriver};
    use crate::service::ResetService;
    use core::cell::Cell;
    use core::sync::atomic::{AtomicBool, Ordering};
    use embassy_time::Duration;

    struct Trigger(ResetScope);
    impl ResetTriggerDriver for Trigger {
        async fn wait_for_reset(&mut self) -> ResetScope {
            self.0
        }
    }

    struct Reset {
        erased: Cell<Option<ResetScope>>,
        reboot_called: AtomicBool,
    }
    impl SystemResetDriver for Reset {
        fn erase_credentials(&self, scope: ResetScope) {
            self.erased.set(Some(scope));
        }
        fn reboot(&self) -> ! {
            self.reboot_called.store(true, Ordering::SeqCst);
            panic!("mock reboot")
        }
    }

    fn run(scope: ResetScope) -> (Option<ResetScope>, Option<SystemState>, bool) {
        let reset = Reset {
            erased: Cell::new(None),
            reboot_called: AtomicBool::new(false),
        };
        let mut service =
            ResetService::new(Trigger(scope), reset).with_reboot_delay(Duration::from_millis(0));
        let watch = SystemStateWatch::new();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            futures::executor::block_on(service.run(SystemStateSender::new(&watch)));
        }));
        (
            service.reset.erased.get(),
            watch.try_get(),
            service.reset.reboot_called.load(Ordering::SeqCst),
        )
    }

    #[test]
    fn reset_service_erases_ecosystem_scope_and_reboots() {
        assert_eq!(
            run(ResetScope::Ecosystem),
            (
                Some(ResetScope::Ecosystem),
                Some(SystemState::EcosystemReset),
                true
            )
        );
    }

    #[test]
    fn reset_service_erases_factory_scope_and_reboots() {
        assert_eq!(
            run(ResetScope::Factory),
            (
                Some(ResetScope::Factory),
                Some(SystemState::FactoryReset),
                true
            )
        );
    }
}
