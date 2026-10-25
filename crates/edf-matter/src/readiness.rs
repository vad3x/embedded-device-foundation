//! Consumer-configured subscription readiness barrier.

use edf_core::events::SystemStateSender;
use portable_atomic::{AtomicBool, AtomicU32, Ordering};

/// Tracks required subscription bits independently from pairing and network state.
pub struct ClusterReadinessBarrier<'a> {
    required_mask: u32,
    ready_mask: AtomicU32,
    paired: AtomicBool,
    state_sender: SystemStateSender<'a>,
}

impl<'a> ClusterReadinessBarrier<'a> {
    pub const fn new(required_mask: u32, state_sender: SystemStateSender<'a>) -> Self {
        Self {
            required_mask,
            ready_mask: AtomicU32::new(0),
            paired: AtomicBool::new(false),
            state_sender,
        }
    }

    /// Mark one or more consumer-defined subscription bits ready.
    pub fn mark_ready(&self, bits: u32) {
        let before = self.ready_mask.fetch_or(bits, Ordering::AcqRel);
        if (before & self.required_mask) != self.required_mask
            && ((before | bits) & self.required_mask) == self.required_mask
        {
            self.state_sender.operational();
        }
    }

    /// Clear subscription readiness while preserving pairing state.
    pub fn reset(&self) {
        self.ready_mask.store(0, Ordering::Release);
    }

    /// A zero required mask is vacuously ready; construction publishes no state.
    pub fn is_ready(&self) -> bool {
        (self.ready_mask.load(Ordering::Acquire) & self.required_mask) == self.required_mask
    }

    pub fn set_paired(&self, paired: bool) {
        self.paired.store(paired, Ordering::Release);
    }

    pub fn is_paired(&self) -> bool {
        self.paired.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use crate::readiness::ClusterReadinessBarrier;
    use edf_core::events::{SystemState, SystemStateSender, SystemStateWatch};

    #[test]
    fn required_bits_gate_operational_publication() {
        let watch = SystemStateWatch::new();
        let sender = SystemStateSender::new(&watch);
        let barrier = ClusterReadinessBarrier::new(0b101, sender);
        barrier.mark_ready(0b001);
        assert!(!barrier.is_ready());
        assert_eq!(sender.current_state(), None);
        barrier.mark_ready(0b100);
        assert!(barrier.is_ready());
        assert_eq!(sender.current_state(), Some(SystemState::Operational));
    }

    #[test]
    fn reset_clears_readiness_but_retains_pairing() {
        let watch = SystemStateWatch::new();
        let barrier = ClusterReadinessBarrier::new(0b11, SystemStateSender::new(&watch));
        barrier.set_paired(true);
        barrier.mark_ready(0b11);
        barrier.reset();
        assert!(!barrier.is_ready());
        assert!(barrier.is_paired());
    }

    #[test]
    fn zero_required_mask_is_ready_without_constructor_publication() {
        let watch = SystemStateWatch::new();
        let sender = SystemStateSender::new(&watch);
        let barrier = ClusterReadinessBarrier::new(0, sender);
        assert!(barrier.is_ready());
        assert_eq!(sender.current_state(), None);
        barrier.mark_ready(1);
        assert!(barrier.is_ready());
    }
}
