//! System reset domain logic and hold duration classification.

use embassy_time::Duration;

// ============================================================================
// Public Types & Functions
// ============================================================================

/// Scope of persistent credentials erasure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResetScope {
    /// Erases only active ecosystem credentials (e.g. smart home fabrics and sessions).
    /// Preserves Wi-Fi credentials so the device stays on the local network.
    Ecosystem,
    /// Full factory reset: erases both Wi-Fi credentials and ecosystem credentials.
    Factory,
}

/// Pure evaluation function classifying button press hold duration into a `ResetScope`.
pub fn classify_reset_hold(
    elapsed: Duration,
    released: bool,
    ecosystem_hold: Duration,
    factory_hold: Duration,
) -> Option<ResetScope> {
    if elapsed >= factory_hold {
        Some(ResetScope::Factory)
    } else if released && elapsed >= ecosystem_hold {
        Some(ResetScope::Ecosystem)
    } else {
        None
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    const ECOSYSTEM_HOLD: Duration = Duration::from_millis(5000);
    const FACTORY_HOLD: Duration = Duration::from_millis(10000);

    #[test]
    fn test_released_before_ecosystem_threshold_is_ignored() {
        // Arrange
        let elapsed = Duration::from_millis(4900);
        let released = true;

        // Act
        let result = classify_reset_hold(elapsed, released, ECOSYSTEM_HOLD, FACTORY_HOLD);

        // Assert
        assert_eq!(result, None);
    }

    #[test]
    fn test_held_at_ecosystem_threshold_without_release_waits() {
        // Arrange - user is holding past 5s, might be holding for 10s factory reset
        let elapsed = Duration::from_millis(6000);
        let released = false;

        // Act
        let result = classify_reset_hold(elapsed, released, ECOSYSTEM_HOLD, FACTORY_HOLD);

        // Assert
        assert_eq!(result, None);
    }

    #[test]
    fn test_released_between_ecosystem_and_factory_triggers_ecosystem_reset() {
        // Arrange
        let elapsed = Duration::from_millis(7500);
        let released = true;

        // Act
        let result = classify_reset_hold(elapsed, released, ECOSYSTEM_HOLD, FACTORY_HOLD);

        // Assert
        assert_eq!(result, Some(ResetScope::Ecosystem));
    }

    #[test]
    fn test_released_just_under_factory_threshold_triggers_ecosystem_reset() {
        // Arrange
        let elapsed = Duration::from_millis(9990);
        let released = true;

        // Act
        let result = classify_reset_hold(elapsed, released, ECOSYSTEM_HOLD, FACTORY_HOLD);

        // Assert
        assert_eq!(result, Some(ResetScope::Ecosystem));
    }

    #[test]
    fn test_held_at_or_above_factory_threshold_triggers_factory_reset_immediately() {
        // Arrange - button reaches 10s without waiting for release
        let elapsed = Duration::from_millis(10000);
        let released = false;

        // Act
        let result = classify_reset_hold(elapsed, released, ECOSYSTEM_HOLD, FACTORY_HOLD);

        // Assert
        assert_eq!(result, Some(ResetScope::Factory));
    }

    #[test]
    fn test_held_well_beyond_factory_threshold_triggers_factory_reset() {
        // Arrange
        let elapsed = Duration::from_millis(15000);
        let released = false;

        // Act
        let result = classify_reset_hold(elapsed, released, ECOSYSTEM_HOLD, FACTORY_HOLD);

        // Assert
        assert_eq!(result, Some(ResetScope::Factory));
    }
}
