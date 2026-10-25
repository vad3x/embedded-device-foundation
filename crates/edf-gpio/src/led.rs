//! Active-low lifecycle LED indicator driver.
//!
//! Signal-based LED driver: synchronous methods signal commands, while the async `run` loop
//! drives GPIO timing and observes `SystemStateReceiver` for lifecycle patterns.

use core::cell::RefCell;
use core::marker::PhantomData;

use edf_core::events::{SystemState, SystemStateReceiver};
use embassy_futures::join::join;
use embassy_futures::select::{select, Either};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Timer};
#[cfg(target_arch = "riscv32")]
use esp_hal::gpio::{Level, Output, OutputConfig};
use log::info;

use crate::config;

// ============================================================================
// Public Structs & Implementation
// ============================================================================

/// Compile-time LED pattern timings for a device profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LedTimings {
    pub booting_on_ms: u64,
    pub booting_off_ms: u64,
    pub starting_on_ms: u64,
    pub starting_off_ms: u64,
    pub commissioning_on_ms: u64,
    pub commissioning_off_ms: u64,
    pub wifi_connecting_on_ms: u64,
    pub wifi_connecting_off_ms: u64,
    pub ecosystem_connecting_on_ms: u64,
    pub ecosystem_connecting_off_ms: u64,
    pub error_on_ms: u64,
    pub error_off_ms: u64,
    pub error_burst_count: u8,
    pub error_pause_ms: u64,
    pub ecosystem_reset_on_ms: u64,
    pub ecosystem_reset_off_ms: u64,
    pub factory_reset_on_ms: u64,
    pub factory_reset_off_ms: u64,
    pub updating_strobe_ms: u64,
    pub activity_pulse_ms: u64,
}

impl LedTimings {
    /// Foundational default timings, sourced from [`crate::config`].
    pub const DEFAULT: Self = Self {
        booting_on_ms: config::BOOTING_ON_MS,
        booting_off_ms: config::BOOTING_OFF_MS,
        starting_on_ms: config::STARTING_ON_MS,
        starting_off_ms: config::STARTING_OFF_MS,
        commissioning_on_ms: config::COMMISSIONING_ON_MS,
        commissioning_off_ms: config::COMMISSIONING_OFF_MS,
        wifi_connecting_on_ms: config::WIFI_CONNECTING_ON_MS,
        wifi_connecting_off_ms: config::WIFI_CONNECTING_OFF_MS,
        ecosystem_connecting_on_ms: config::ECOSYSTEM_CONNECTING_ON_MS,
        ecosystem_connecting_off_ms: config::ECOSYSTEM_CONNECTING_OFF_MS,
        error_on_ms: config::ERROR_ON_MS,
        error_off_ms: config::ERROR_OFF_MS,
        error_burst_count: config::ERROR_BURST_COUNT,
        error_pause_ms: config::ERROR_PAUSE_MS,
        ecosystem_reset_on_ms: config::ECOSYSTEM_RESET_ON_MS,
        ecosystem_reset_off_ms: config::ECOSYSTEM_RESET_OFF_MS,
        factory_reset_on_ms: config::FACTORY_RESET_ON_MS,
        factory_reset_off_ms: config::FACTORY_RESET_OFF_MS,
        updating_strobe_ms: config::UPDATING_STROBE_MS,
        activity_pulse_ms: config::ACTIVITY_PULSE_MS,
    };
}

impl Default for LedTimings {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Compile-time profile supplying LED pattern timings to [`GpioLedDriver`].
pub trait LedProfile {
    const TIMINGS: LedTimings;
}

/// The foundational profile using [`LedTimings::DEFAULT`].
pub struct DefaultLedProfile;

impl LedProfile for DefaultLedProfile {
    const TIMINGS: LedTimings = LedTimings::DEFAULT;
}

/// Declare an LED profile with optional per-field overrides.
///
/// Fields omitted from the invocation inherit [`LedTimings::DEFAULT`].
///
/// ```ignore
/// edf_gpio::led_profile! {
///     pub struct IntercomLedProfile;
///     booting_on_ms: 200,
///     booting_off_ms: 1000,
/// }
/// ```
#[macro_export]
macro_rules! led_profile {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident;
        $($field:ident : $value:expr),* $(,)?
    ) => {
        $(#[$meta])*
        $vis struct $name;

        #[allow(clippy::needless_update)]
        impl $crate::led::LedProfile for $name {
            const TIMINGS: $crate::led::LedTimings = $crate::led::LedTimings {
                $($field: $value,)*
                ..$crate::led::LedTimings::DEFAULT
            };
        }
    };
}

/// Object-safe hold-feedback operations used by the reset button driver.
pub trait HoldFeedback {
    fn hold_feedback_press(&self);
    fn hold_feedback_stage1(&self);
    fn hold_feedback_stage2(&self);
    fn restore_state(&self);
}

/// GPIO-based active-low status LED indicator driver.
///
/// In active-low wiring, the LED is ON when the output is LOW, and OFF when HIGH.
/// Synchronous trigger methods queue commands to the background `run` loop.
pub struct GpioLedDriver<P: LedProfile = DefaultLedProfile> {
    #[cfg(target_arch = "riscv32")]
    pin: RefCell<Output<'static>>,
    #[cfg(not(target_arch = "riscv32"))]
    pin: RefCell<MockLedPin>,
    signal: Signal<CriticalSectionRawMutex, IndicatorCommand>,
    current_state: RefCell<Option<SystemState>>,
    _profile: PhantomData<P>,
}

impl GpioLedDriver<DefaultLedProfile> {
    /// Create a new LED driver with the target hardware output pin on RISC-V.
    #[cfg(target_arch = "riscv32")]
    pub fn new(pin: impl esp_hal::gpio::OutputPin + 'static) -> Self {
        Self::with_profile(pin)
    }

    /// Create a new LED driver with simulated pin for host testing.
    #[cfg(not(target_arch = "riscv32"))]
    pub fn new() -> Self {
        Self::with_profile()
    }
}

impl<P: LedProfile> GpioLedDriver<P> {
    /// Create a new LED driver using the profile's [`LedTimings`].
    #[cfg(target_arch = "riscv32")]
    pub fn with_profile(pin: impl esp_hal::gpio::OutputPin + 'static) -> Self {
        info!("GpioLedDriver created (active-low)");
        Self {
            pin: RefCell::new(Output::new(pin, Level::High, OutputConfig::default())), // Start OFF (High)
            signal: Signal::new(),
            current_state: RefCell::new(None),
            _profile: PhantomData,
        }
    }

    /// Create a new LED driver using the profile's [`LedTimings`] for host testing.
    #[cfg(not(target_arch = "riscv32"))]
    pub fn with_profile() -> Self {
        info!("GpioLedDriver created (host mock active-low)");
        Self {
            pin: RefCell::new(MockLedPin::new()), // Start OFF (High)
            signal: Signal::new(),
            current_state: RefCell::new(None),
            _profile: PhantomData,
        }
    }

    /// Driver event loop — observes lifecycle states and drives hardware pin timing.
    /// Runs concurrently with other firmware tasks in the composition root.
    pub async fn run(&self, mut state_rx: SystemStateReceiver<'_>) {
        join(self.command_loop(), self.observe_state(&mut state_rx)).await;
    }

    /// Indicate that the reset/BOOT button is currently being pressed (stage 0: 0s-5s).
    pub fn hold_feedback_press(&self) {
        self.signal.signal(IndicatorCommand::Off);
    }

    /// Request a brief activity wink when the system is operational.
    pub fn activity_pulse(&self) {
        self.signal.signal(IndicatorCommand::ActivityPulse);
    }

    /// Indicate that stage 1 (Ecosystem reset, 5s) has been reached.
    pub fn hold_feedback_stage1(&self) {
        self.blink(
            P::TIMINGS.ecosystem_reset_on_ms,
            P::TIMINGS.ecosystem_reset_off_ms,
            None,
        );
    }

    /// Indicate that stage 2 (Factory reset, 10s) has been reached.
    pub fn hold_feedback_stage2(&self) {
        self.blink(
            P::TIMINGS.factory_reset_on_ms,
            P::TIMINGS.factory_reset_off_ms,
            None,
        );
    }

    /// Signal a finite or continuous blink pattern.
    ///
    /// `count: None` blinks until interrupted. `Some(n)` blinks `n` times
    /// and returns to steady ON unless interrupted.
    pub fn blink(&self, on_ms: u64, off_ms: u64, count: Option<u8>) {
        self.signal.signal(IndicatorCommand::Blink {
            on_ms,
            off_ms,
            count,
        });
    }

    /// Re-apply the current system lifecycle state (e.g. after button is released early).
    pub fn restore_state(&self) {
        if let Some(state) = *self.current_state.borrow() {
            self.indicate_state(state);
        }
    }

    /// Query whether the LED is currently powered ON (pin is LOW).
    pub fn is_on(&self) -> bool {
        #[cfg(target_arch = "riscv32")]
        {
            self.pin.borrow().is_set_low()
        }
        #[cfg(not(target_arch = "riscv32"))]
        {
            self.pin.borrow().is_set_low()
        }
    }

    /// Query the last observed system lifecycle state.
    pub fn observed_state(&self) -> Option<SystemState> {
        *self.current_state.borrow()
    }

    // ========================================================================
    // Internal Helper Methods
    // ========================================================================

    /// Command loop — processes indicator commands and drives hardware pin timing.
    async fn command_loop(&self) {
        let mut cmd = self.signal.wait().await;

        loop {
            // Execute the command; if interrupted by a new signal, handle it immediately
            match self.next(cmd).await {
                Some(next) => cmd = next,
                None => cmd = self.signal.wait().await,
            }
        }
    }

    /// State observer loop — updates indicator pattern based on system lifecycle transitions.
    async fn observe_state(&self, state_rx: &mut SystemStateReceiver<'_>) {
        if let Some(state) = state_rx.try_get() {
            *self.current_state.borrow_mut() = Some(state);
            self.indicate_state(state);
        }

        loop {
            let state = state_rx.changed().await;
            *self.current_state.borrow_mut() = Some(state);
            self.indicate_state(state);
        }
    }

    fn solid(&self) {
        self.signal.signal(IndicatorCommand::On);
    }

    fn strobe(&self) {
        self.signal.signal(IndicatorCommand::Strobe);
    }

    fn burst(&self, on_ms: u64, off_ms: u64, burst_count: u8, pause_ms: u64) {
        self.signal.signal(IndicatorCommand::Burst {
            on_ms,
            off_ms,
            burst_count,
            pause_ms,
        });
    }

    /// Execute a command. Returns the next pending command if interrupted by a higher-priority signal.
    async fn next(&self, cmd: IndicatorCommand) -> Option<IndicatorCommand> {
        match cmd {
            IndicatorCommand::On => {
                self.led_on();
                None
            }
            IndicatorCommand::Off => {
                self.led_off();
                None
            }
            IndicatorCommand::ActivityPulse => {
                // Flash a brief wink when operational.
                if *self.current_state.borrow() == Some(SystemState::Operational) {
                    self.led_off();
                    let interrupted = self.interruptible_wait(P::TIMINGS.activity_pulse_ms).await;
                    if interrupted.is_none() {
                        self.led_on();
                    }
                    interrupted
                } else {
                    None
                }
            }
            IndicatorCommand::Strobe => {
                // Continuous rapid strobe until interrupted (e.g. during OTA update)
                loop {
                    if let Some(next) = self
                        .blink_pattern(
                            1,
                            P::TIMINGS.updating_strobe_ms,
                            P::TIMINGS.updating_strobe_ms,
                        )
                        .await
                    {
                        return Some(next);
                    }
                }
            }
            IndicatorCommand::Blink {
                on_ms,
                off_ms,
                count: None,
            } => {
                // Continuous blink until interrupted by a new state or command
                loop {
                    if let Some(next) = self.blink_pattern(1, on_ms, off_ms).await {
                        return Some(next);
                    }
                }
            }
            IndicatorCommand::Blink {
                on_ms,
                off_ms,
                count: Some(n),
            } => {
                // Finite blink — returns to steady ON when finished
                let interrupted = self.blink_pattern(n, on_ms, off_ms).await;
                if interrupted.is_none() {
                    self.led_on();
                }
                interrupted
            }
            IndicatorCommand::Burst {
                on_ms,
                off_ms,
                burst_count,
                pause_ms,
            } => {
                // Repeating burst: N quick blinks + long pause
                loop {
                    if let Some(next) = self.blink_pattern(burst_count, on_ms, off_ms).await {
                        return Some(next);
                    }
                    if let Some(next) = self.interruptible_wait(pause_ms).await {
                        return Some(next);
                    }
                }
            }
        }
    }

    /// Blink the LED `count` times with given ON and OFF durations.
    /// Returns `Some(cmd)` if interrupted by an incoming command, or `None` if completed.
    async fn blink_pattern(&self, count: u8, on_ms: u64, off_ms: u64) -> Option<IndicatorCommand> {
        for _ in 0..count {
            self.led_on();
            match select(
                Timer::after(Duration::from_millis(on_ms)),
                self.signal.wait(),
            )
            .await
            {
                Either::First(()) => {}
                Either::Second(cmd) => return Some(cmd),
            }

            self.led_off();
            match select(
                Timer::after(Duration::from_millis(off_ms)),
                self.signal.wait(),
            )
            .await
            {
                Either::First(()) => {}
                Either::Second(cmd) => return Some(cmd),
            }
        }
        None
    }

    /// Wait for `ms` milliseconds, interruptible by a newly signalled command.
    /// Returns `Some(cmd)` if interrupted, `None` if the wait completed.
    async fn interruptible_wait(&self, ms: u64) -> Option<IndicatorCommand> {
        match select(Timer::after(Duration::from_millis(ms)), self.signal.wait()).await {
            Either::First(()) => None,
            Either::Second(cmd) => Some(cmd),
        }
    }

    fn indicate_state(&self, state: SystemState) {
        match state {
            SystemState::Booting => {
                self.blink(P::TIMINGS.booting_on_ms, P::TIMINGS.booting_off_ms, None);
            }
            SystemState::Starting => {
                self.blink(P::TIMINGS.starting_on_ms, P::TIMINGS.starting_off_ms, None);
            }
            SystemState::Commissioning => {
                self.blink(
                    P::TIMINGS.commissioning_on_ms,
                    P::TIMINGS.commissioning_off_ms,
                    None,
                );
            }
            SystemState::WifiConnecting => {
                self.blink(
                    P::TIMINGS.wifi_connecting_on_ms,
                    P::TIMINGS.wifi_connecting_off_ms,
                    None,
                );
            }
            SystemState::EcosystemConnecting => {
                self.blink(
                    P::TIMINGS.ecosystem_connecting_on_ms,
                    P::TIMINGS.ecosystem_connecting_off_ms,
                    None,
                );
            }
            SystemState::Operational => {
                self.solid();
            }
            SystemState::Updating => {
                self.strobe();
            }
            SystemState::EcosystemReset => {
                self.blink(
                    P::TIMINGS.ecosystem_reset_on_ms,
                    P::TIMINGS.ecosystem_reset_off_ms,
                    None,
                );
            }
            SystemState::FactoryReset => {
                self.blink(
                    P::TIMINGS.factory_reset_on_ms,
                    P::TIMINGS.factory_reset_off_ms,
                    None,
                );
            }
            SystemState::Error => {
                self.burst(
                    P::TIMINGS.error_on_ms,
                    P::TIMINGS.error_off_ms,
                    P::TIMINGS.error_burst_count,
                    P::TIMINGS.error_pause_ms,
                );
            }
        }
    }

    /// Turn LED on (active-low: GPIO LOW).
    fn led_on(&self) {
        self.pin.borrow_mut().set_low();
    }

    /// Turn LED off (active-low: GPIO HIGH).
    fn led_off(&self) {
        self.pin.borrow_mut().set_high();
    }
}

impl<P: LedProfile> HoldFeedback for GpioLedDriver<P> {
    fn hold_feedback_press(&self) {
        GpioLedDriver::<P>::hold_feedback_press(self);
    }

    fn hold_feedback_stage1(&self) {
        GpioLedDriver::<P>::hold_feedback_stage1(self);
    }

    fn hold_feedback_stage2(&self) {
        GpioLedDriver::<P>::hold_feedback_stage2(self);
    }

    fn restore_state(&self) {
        GpioLedDriver::<P>::restore_state(self);
    }
}

#[cfg(not(target_arch = "riscv32"))]
impl Default for GpioLedDriver<DefaultLedProfile> {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Internal Commands & Supporting Types
// ============================================================================

/// Commands sent to the driver execution loop via `Signal`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IndicatorCommand {
    /// Steady ON (GPIO LOW).
    On,
    /// Steady OFF (GPIO HIGH).
    Off,
    /// Brief activity pulse (150ms wink when operational).
    ActivityPulse,
    /// Rapid strobe blink (10 Hz, e.g. during OTA update).
    Strobe,
    /// Continuous or finite blink pattern.
    Blink {
        on_ms: u64,
        off_ms: u64,
        count: Option<u8>,
    },
    /// Repeating burst pattern: N blinks followed by pause.
    Burst {
        on_ms: u64,
        off_ms: u64,
        burst_count: u8,
        pause_ms: u64,
    },
}

/// Simulated output pin for host unit tests.
#[cfg(not(target_arch = "riscv32"))]
#[derive(Debug, Default)]
pub struct MockLedPin {
    pub is_low: bool,
}

#[cfg(not(target_arch = "riscv32"))]
impl MockLedPin {
    pub fn new() -> Self {
        Self { is_low: false } // Starts high (OFF for active-low)
    }

    pub fn set_low(&mut self) {
        self.is_low = true;
    }

    pub fn set_high(&mut self) {
        self.is_low = false;
    }

    pub fn is_low(&self) -> bool {
        self.is_low
    }

    pub fn is_set_low(&self) -> bool {
        self.is_low
    }

    pub fn is_high(&self) -> bool {
        !self.is_low
    }
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    crate::led_profile! {
        struct TestProfile;
        booting_on_ms: 123,
        booting_off_ms: 456,
    }

    crate::led_profile! {
        struct EmptyProfile;
    }

    #[test]
    fn starts_in_off_state() {
        // Arrange & Act
        let driver = GpioLedDriver::new();

        // Assert
        assert!(!driver.is_on());
        assert_eq!(driver.observed_state(), None);
    }

    #[test]
    fn activity_pulse_signals_command() {
        // Arrange
        let driver = GpioLedDriver::new();
        // Act
        driver.activity_pulse();

        // Assert
        // Signal channel contains pending command
        assert!(driver.signal.signaled());
    }

    #[test]
    fn hold_feedback_triggers_signals() {
        // Arrange
        let driver = GpioLedDriver::new();

        // Act & Assert 1: Press
        driver.hold_feedback_press();
        assert!(driver.signal.signaled());

        // Act & Assert 2: Stage 1 (Ecosystem reset hold)
        driver.hold_feedback_stage1();
        assert!(driver.signal.signaled());

        // Act & Assert 3: Stage 2 (Factory reset hold)
        driver.hold_feedback_stage2();
        assert!(driver.signal.signaled());
    }

    #[test]
    fn custom_profile_overrides_lifecycle_timings() {
        // Arrange
        let driver = GpioLedDriver::<TestProfile>::with_profile();

        // Act
        driver.indicate_state(SystemState::Booting);

        // Assert
        assert_eq!(
            driver.signal.try_take(),
            Some(IndicatorCommand::Blink {
                on_ms: 123,
                off_ms: 456,
                count: None,
            })
        );
    }

    #[test]
    fn empty_profile_inherits_default_timings() {
        // Arrange & Act & Assert
        assert_eq!(EmptyProfile::TIMINGS, LedTimings::DEFAULT);
    }
}
