//! Active-low button driver for dual-stage reset holds:
//! - Held 5s to 10s and released: Ecosystem Reset (unpairs active smart home ecosystem, preserves Wi-Fi).
//! - Held >= 10s: Full Factory Reset (erases both Wi-Fi credentials and ecosystem fabrics).

use edf_core::domain::classify_reset_hold;
use edf_core::ports::{ResetScope, ResetTriggerDriver};
use embassy_time::{Duration, Instant, Timer};
#[cfg(target_arch = "riscv32")]
use esp_hal::gpio::{Input, InputConfig, Pull};
use log::info;

use crate::config;
use crate::led::HoldFeedback;

// ============================================================================
// Public Structs & Implementation
// ============================================================================

/// GPIO-based BOOT button driver for system credential resets.
pub struct GpioBootButtonDriver {
    config: GpioBootButtonConfig,
}

impl GpioBootButtonDriver {
    /// Construct a new reset button driver with the provided configuration.
    pub fn new(config: GpioBootButtonConfig) -> Self {
        info!(
            "GpioBootButtonDriver: ecosystem_hold={}ms, factory_hold={}ms, feedback={}",
            config.ecosystem_hold_ms,
            config.factory_hold_ms,
            config.indicator.is_some()
        );
        Self { config }
    }
}

/// Configuration options for the GPIO BOOT button reset driver.
pub struct GpioBootButtonConfig {
    /// Active-low input pin with internal pull-up.
    #[cfg(target_arch = "riscv32")]
    pub pin: Input<'static>,
    #[cfg(not(target_arch = "riscv32"))]
    pub pin: MockButtonPin,

    /// Optional LED indicator for live visual hold feedback.
    pub indicator: Option<&'static dyn HoldFeedback>,

    /// Duration the button must be held to arm ecosystem reset (ms).
    pub ecosystem_hold_ms: u64,

    /// Duration the button must be held to trigger factory reset (ms).
    pub factory_hold_ms: u64,

    /// Duration to wait after a button edge to filter contact bounce (ms).
    pub debounce_ms: u64,

    /// Interval between button state checks while held (ms).
    pub poll_interval_ms: u64,
}

impl GpioBootButtonConfig {
    /// Create default configuration for target hardware on RISC-V.
    #[cfg(target_arch = "riscv32")]
    pub fn new(pin: impl esp_hal::gpio::InputPin + 'static) -> Self {
        Self {
            pin: Input::new(pin, InputConfig::default().with_pull(Pull::Up)),
            indicator: None,
            ecosystem_hold_ms: config::ECOSYSTEM_RESET_HOLD_MS,
            factory_hold_ms: config::FACTORY_RESET_HOLD_MS,
            debounce_ms: config::BUTTON_DEBOUNCE_MS,
            poll_interval_ms: config::BUTTON_POLL_INTERVAL_MS,
        }
    }

    /// Create default configuration for host testing with simulated pin.
    #[cfg(not(target_arch = "riscv32"))]
    pub fn new() -> Self {
        Self {
            pin: MockButtonPin::new(),
            indicator: None,
            ecosystem_hold_ms: config::ECOSYSTEM_RESET_HOLD_MS,
            factory_hold_ms: config::FACTORY_RESET_HOLD_MS,
            debounce_ms: config::BUTTON_DEBOUNCE_MS,
            poll_interval_ms: config::BUTTON_POLL_INTERVAL_MS,
        }
    }

    /// Attach a status LED indicator for live hold feedback.
    pub fn with_indicator<I: HoldFeedback + 'static>(mut self, indicator: &'static I) -> Self {
        self.indicator = Some(indicator);
        self
    }

    /// Configure custom hold thresholds (useful for accelerated testing).
    pub fn with_hold_durations(mut self, ecosystem_ms: u64, factory_ms: u64) -> Self {
        self.ecosystem_hold_ms = ecosystem_ms;
        self.factory_hold_ms = factory_ms;
        self
    }

    /// Select input debounce and held-button polling intervals (ms).
    pub fn with_input_timings(mut self, debounce_ms: u64, poll_interval_ms: u64) -> Self {
        self.debounce_ms = debounce_ms;
        self.poll_interval_ms = poll_interval_ms;
        self
    }
}

#[cfg(not(target_arch = "riscv32"))]
impl Default for GpioBootButtonConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl ResetTriggerDriver for GpioBootButtonDriver {
    async fn wait_for_reset(&mut self) -> ResetScope {
        let ecosystem_duration = Duration::from_millis(self.config.ecosystem_hold_ms);
        let factory_duration = Duration::from_millis(self.config.factory_hold_ms);

        loop {
            // Wait for button press (falling edge: active-low)
            self.config.pin.wait_for_falling_edge().await;

            // Debounce mechanical contact bounce
            Timer::after(Duration::from_millis(self.config.debounce_ms)).await;
            if self.config.pin.is_high() {
                continue;
            }

            let press_start = Instant::now();
            info!(
                "BOOT button pressed — hold {}s for ecosystem reset, {}s for full factory reset",
                self.config.ecosystem_hold_ms / 1000,
                self.config.factory_hold_ms / 1000
            );

            if let Some(led) = self.config.indicator {
                led.hold_feedback_press();
            }

            let mut stage1_notified = false;

            let scope = loop {
                Timer::after(Duration::from_millis(self.config.poll_interval_ms)).await;

                let elapsed = Instant::now().duration_since(press_start);
                let pin_high = self.config.pin.is_high();
                let released = if pin_high {
                    // Debounce release to filter noise
                    Timer::after(Duration::from_millis(self.config.debounce_ms)).await;
                    self.config.pin.is_high()
                } else {
                    false
                };

                // Visual feedback when Stage 1 (Ecosystem reset, 5s) is armed
                if !stage1_notified && elapsed >= ecosystem_duration {
                    stage1_notified = true;
                    info!("BOOT button hold 5s reached (Ecosystem Reset armed)");
                    if let Some(led) = self.config.indicator {
                        led.hold_feedback_stage1();
                    }
                }

                if let Some(scope) =
                    classify_reset_hold(elapsed, released, ecosystem_duration, factory_duration)
                {
                    if scope == ResetScope::Factory {
                        info!("BOOT button hold 10s reached (Factory Reset triggered)");
                        if let Some(led) = self.config.indicator {
                            led.hold_feedback_stage2();
                        }
                    }
                    break Some(scope);
                }

                if released {
                    info!("BOOT button released early (ignored)");
                    if let Some(led) = self.config.indicator {
                        led.restore_state();
                    }
                    break None;
                }
            };

            if let Some(triggered_scope) = scope {
                return triggered_scope;
            }
        }
    }
}

// ============================================================================
// Host Simulated Pin
// ============================================================================

/// Simulated input pin for host unit tests.
#[cfg(not(target_arch = "riscv32"))]
#[derive(Debug, Default)]
pub struct MockButtonPin {
    pressed: core::cell::Cell<bool>,
}

#[cfg(not(target_arch = "riscv32"))]
impl MockButtonPin {
    pub fn new() -> Self {
        Self {
            pressed: core::cell::Cell::new(false),
        }
    }

    pub fn set_pressed(&self, pressed: bool) {
        self.pressed.set(pressed);
    }

    pub fn is_high(&self) -> bool {
        // Active-low: high = unpressed (pull-up)
        !self.pressed.get()
    }

    pub fn is_low(&self) -> bool {
        self.pressed.get()
    }

    pub async fn wait_for_falling_edge(&self) {
        // In host tests, this completes immediately if already low or pending
        if !self.is_low() {
            Timer::after(Duration::from_millis(1)).await;
        }
    }
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_timing_builder_selects_debounce_and_poll_intervals() {
        let config = GpioBootButtonConfig::new().with_input_timings(17, 42);

        assert_eq!([config.debounce_ms, config.poll_interval_ms], [17, 42]);
    }

    #[test]
    fn config_defaults_and_builder_options() {
        // Arrange & Act
        let config = GpioBootButtonConfig::new().with_hold_durations(3000, 6000);

        // Assert
        assert_eq!(config.ecosystem_hold_ms, 3000);
        assert_eq!(config.factory_hold_ms, 6000);
        assert!(config.indicator.is_none());
        assert!(config.pin.is_high()); // Unpressed by default
    }

    #[test]
    fn driver_initialization_with_config() {
        // Arrange
        let config = GpioBootButtonConfig::new();

        // Act
        let driver = GpioBootButtonDriver::new(config);

        // Assert
        assert_eq!(
            driver.config.ecosystem_hold_ms,
            config::ECOSYSTEM_RESET_HOLD_MS
        );
        assert_eq!(driver.config.factory_hold_ms, config::FACTORY_RESET_HOLD_MS);
    }

    #[test]
    fn with_indicator_accepts_leaked_led_driver() {
        // Arrange
        let indicator: &'static crate::led::GpioLedDriver =
            Box::leak(Box::new(crate::led::GpioLedDriver::new()));

        // Act
        let config = GpioBootButtonConfig::new().with_indicator(indicator);

        // Assert
        assert!(config.indicator.is_some());
    }
}
