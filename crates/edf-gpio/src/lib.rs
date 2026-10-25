//! Reusable ESP32-C6 GPIO and flash adapters for device lifecycle services.

#![cfg_attr(not(test), no_std)]
#![allow(async_fn_in_trait)]

extern crate alloc;

pub mod boot_button;
pub mod config;
pub mod flash;
pub mod led;

pub use boot_button::{GpioBootButtonConfig, GpioBootButtonDriver};
pub use flash::{EspFlash, EspFlashError};
pub use led::{DefaultLedProfile, GpioLedDriver, HoldFeedback, LedProfile, LedTimings};
