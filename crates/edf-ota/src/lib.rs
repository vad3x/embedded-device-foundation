//! Verified A/B Over-The-Air (OTA) update engine and flash adapter.
//!
//! Provides bootloader state manipulation, streaming flash writing, and Ed25519
//! signature verification for Over-The-Air (OTA) firmware updates.

#![cfg_attr(not(test), no_std)]

extern crate alloc;

// Keep the audited bootloader state machine byte-identical to its source.
#[expect(clippy::unnecessary_map_or, clippy::manual_is_multiple_of)]
#[rustfmt::skip]
pub mod bootloader;
pub mod driver;
// Keep the audited streaming verifier byte-identical to its source.
#[expect(clippy::manual_repeat_n)]
#[rustfmt::skip]
pub mod writer;

pub use driver::{FlashOtaDriver, COOPERATIVE_YIELD_DELAY, MAX_OTA_PAYLOAD_SIZE};

pub use bootloader::{
    ActiveSlot, BootloaderError, EspOtaSelectEntry, MemOtaFlash, OtaBootloader, OtaFlashStorage,
    OtaState, Slot, OTA_DATA_ENTRY_SIZE, OTA_DATA_SECTOR_SIZE,
};

pub use writer::{
    validate_esp_image_header, verify_image_signature, DefaultTargetFlash, MemTargetFlash,
    OtaError, OtaTargetFlash, OtaWriter, SectorBuffer, ESP32_C6_CHIP_ID, ESP_IMAGE_MAGIC,
    MAX_APP_SLOT_SIZE, OTA_0_OFFSET, OTA_1_OFFSET, OTA_HEADER_LEN, OTA_HEADER_MAGIC, OTA_SLOT_SIZE,
    SECTOR_SIZE,
};

#[cfg(target_arch = "riscv32")]
pub use writer::{EspFlashError, EspTargetFlash};
