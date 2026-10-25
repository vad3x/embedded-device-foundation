//! Controller fabric policy and network lifecycle monitoring.

use core::pin::pin;

use edf_core::domain::reset::ResetScope;
use edf_core::events::{SystemState, SystemStateSender};
use edf_core::ports::reset::SystemResetDriver;
use embassy_futures::select::select;
use embassy_time::{Duration, Timer};
use rs_matter::dm::clusters::gen_diag::NetifDiag;
use rs_matter::dm::networks::NetChangeNotif;
use rs_matter::error::Error;

use crate::readiness::ClusterReadinessBarrier;

/// Apple Home Hub vendor ID excluded by the legacy controller policy.
pub const APPLE_HOME_HUB_VENDOR_ID: u16 = 0x1384;

/// Optional vendor exclusion for deciding whether a user-facing controller remains.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FabricPolicy {
    excluded_vendor: Option<u16>,
}

impl Default for FabricPolicy {
    fn default() -> Self {
        Self::new(Some(APPLE_HOME_HUB_VENDOR_ID))
    }
}

impl FabricPolicy {
    /// `None` disables vendor exclusion.
    pub const fn new(excluded_vendor: Option<u16>) -> Self {
        Self { excluded_vendor }
    }

    pub fn has_active_controller_fabric(&self, vendors: impl IntoIterator<Item = u16>) -> bool {
        vendors
            .into_iter()
            .any(|vendor| Some(vendor) != self.excluded_vendor)
    }
}

/// Check the live Matter fabric table under its state lock.
pub fn has_active_controller_fabric(matter: &rs_matter::Matter<'_>, policy: FabricPolicy) -> bool {
    matter.with_state(|state| {
        policy.has_active_controller_fabric(state.fabrics.iter().map(|fabric| fabric.vendor_id()))
    })
}

/// Effects of one connection/fabric observation, independent of hardware.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConnectionEffects {
    pub lifecycle: Option<SystemState>,
    pub reset_readiness: bool,
    pub erase_ecosystem_and_reboot: bool,
}

/// Pure transition memory for the monitor's repeated observations.
pub struct ConnectionState {
    was_operational: bool,
    was_commissioned: bool,
}

impl ConnectionState {
    pub const fn new(initially_commissioned: bool) -> Self {
        Self {
            was_operational: false,
            was_commissioned: initially_commissioned,
        }
    }

    pub const fn orphan_only_boot(has_any_fabrics: bool, has_active_fabric: bool) -> bool {
        has_any_fabrics && !has_active_fabric
    }

    /// Preserve the consumers' network and controller transition priority.
    pub fn step(
        &mut self,
        operational: bool,
        commissioned: bool,
        ready: bool,
    ) -> ConnectionEffects {
        let (lifecycle, reset_readiness) = if operational && !self.was_operational {
            (
                commissioned.then_some(if ready {
                    SystemState::Operational
                } else {
                    SystemState::EcosystemConnecting
                }),
                false,
            )
        } else if !operational && self.was_operational {
            (
                Some(if commissioned {
                    SystemState::WifiConnecting
                } else {
                    SystemState::Commissioning
                }),
                true,
            )
        } else if !self.was_commissioned && commissioned && operational {
            (
                Some(if ready {
                    SystemState::Operational
                } else {
                    SystemState::EcosystemConnecting
                }),
                false,
            )
        } else {
            (None, false)
        };
        let erase_ecosystem_and_reboot = self.was_commissioned && !commissioned;
        self.was_operational = operational;
        self.was_commissioned = commissioned;
        ConnectionEffects {
            lifecycle,
            reset_readiness,
            erase_ecosystem_and_reboot,
        }
    }
}

/// Async runner called by a consumer-owned Matter `UserTask` wrapper.
pub struct ConnectionMonitor<'a, 'b, R: SystemResetDriver> {
    pub state_sender: SystemStateSender<'a>,
    pub readiness: &'a ClusterReadinessBarrier<'a>,
    pub reset: R,
    pub matter: &'b rs_matter::Matter<'b>,
    pub fabric_policy: FabricPolicy,
}

impl<R: SystemResetDriver> ConnectionMonitor<'_, '_, R> {
    /// Observe rs-matter netif changes and a three-second fabric polling tick.
    pub async fn run<N: NetifDiag + NetChangeNotif>(&mut self, netif: N) -> Result<(), Error> {
        let initially_commissioned = has_active_controller_fabric(self.matter, self.fabric_policy);
        if ConnectionState::orphan_only_boot(self.matter.has_fabrics(), initially_commissioned) {
            self.reset.erase_credentials(ResetScope::Ecosystem);
            self.reset.reboot();
        }
        let mut state = ConnectionState::new(initially_commissioned);
        loop {
            let mut operational = false;
            netif.netifs(&mut |interface| {
                operational |= interface.operational;
                Ok(())
            })?;
            let commissioned = has_active_controller_fabric(self.matter, self.fabric_policy);
            self.readiness.set_paired(commissioned);
            let effects = state.step(operational, commissioned, self.readiness.is_ready());
            if effects.reset_readiness {
                self.readiness.reset();
            }
            if let Some(lifecycle) = effects.lifecycle {
                self.state_sender.send(lifecycle);
            }
            if effects.erase_ecosystem_and_reboot {
                self.reset.erase_credentials(ResetScope::Ecosystem);
                self.reset.reboot();
            }
            select(
                pin!(netif.wait_changed()),
                pin!(Timer::after(Duration::from_secs(3))),
            )
            .await;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::connection::{ConnectionState, FabricPolicy};
    use edf_core::events::SystemState;

    #[test]
    fn fabric_policy_defaults_to_apple_hub_exclusion() {
        let policy = FabricPolicy::default();
        assert!(!policy.has_active_controller_fabric([0x1384]));
        assert!(policy.has_active_controller_fabric([0x1384, 0x1349]));
        assert!(FabricPolicy::new(None).has_active_controller_fabric([0x1384]));
        assert!(!FabricPolicy::new(Some(42)).has_active_controller_fabric([42]));
    }

    #[test]
    fn network_and_fabric_transitions_match_consumer_policy() {
        let mut state = ConnectionState::new(false);
        let up = state.step(true, false, false);
        assert_eq!(up.lifecycle, None);
        let paired = state.step(true, true, false);
        assert_eq!(paired.lifecycle, Some(SystemState::EcosystemConnecting));
        let down = state.step(false, true, false);
        assert_eq!(down.lifecycle, Some(SystemState::WifiConnecting));
        assert!(down.reset_readiness);
        let recovered = state.step(true, true, true);
        assert_eq!(recovered.lifecycle, Some(SystemState::Operational));
        let removed = state.step(true, false, false);
        assert!(removed.erase_ecosystem_and_reboot);
    }

    #[test]
    fn uncommissioned_network_loss_returns_to_commissioning() {
        let mut state = ConnectionState::new(false);
        state.step(true, false, false);
        assert_eq!(
            state.step(false, false, false).lifecycle,
            Some(SystemState::Commissioning)
        );
    }

    #[test]
    fn commissioned_boot_waits_for_readiness_when_network_comes_up() {
        let mut state = ConnectionState::new(true);
        assert_eq!(state.step(false, true, false).lifecycle, None);
        assert_eq!(
            state.step(true, true, false).lifecycle,
            Some(SystemState::EcosystemConnecting)
        );
        assert_eq!(state.step(true, true, true).lifecycle, None);
    }

    #[test]
    fn orphan_only_boot_requests_ecosystem_reset() {
        assert!(ConnectionState::orphan_only_boot(true, false));
        assert!(!ConnectionState::orphan_only_boot(false, false));
        assert!(!ConnectionState::orphan_only_boot(true, true));
    }
}
