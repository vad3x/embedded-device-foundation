//! Flash OTA Driver Port Implementation
//!
//! Implements `OtaFlashDriver` from `edf_core::ports::ota` by streaming firmware
//! bytes through `OtaWriter`, verifying Ed25519 signatures, and committing new bootloader
//! entries with `OtaBootloader`.

#[cfg(not(target_arch = "riscv32"))]
use crate::bootloader::MemOtaFlash;
#[cfg(target_arch = "riscv32")]
use crate::bootloader::{BootloaderError, EspOtaFlash};
use crate::bootloader::{DefaultFlash, OtaBootloader, OtaFlashStorage};
#[cfg(target_arch = "riscv32")]
use crate::writer::EspTargetFlash;
#[cfg(not(target_arch = "riscv32"))]
use crate::writer::MemTargetFlash;
use crate::writer::{
    DefaultTargetFlash, OtaError, OtaTargetFlash, OtaWriter, MAX_APP_SLOT_SIZE, OTA_HEADER_LEN,
};

use edf_core::ports::ota::{OtaDriverError, OtaFlashDriver, OtaStream};
use embassy_time::{Duration, Timer};
use log::{info, warn};

// ============================================================================
// Public Main Struct & Implementation
// ============================================================================

/// Flash OTA driver implementation for streaming, cryptographically verifying,
/// and committing firmware updates into inactive application partitions.
pub struct FlashOtaDriver<F = DefaultFlash, TF = DefaultTargetFlash> {
    public_key: [u8; 32],
    check_esp_magic: bool,
    bootloader: OtaBootloader<F>,
    target_flash: TF,
    reboot_required: bool,
}

#[cfg(not(target_arch = "riscv32"))]
impl FlashOtaDriver<MemOtaFlash, MemTargetFlash> {
    /// Construct a new `FlashOtaDriver` with default in-memory flash for host testing.
    pub fn new(public_key: [u8; 32]) -> Self {
        Self {
            public_key,
            check_esp_magic: true,
            bootloader: OtaBootloader::new(MemOtaFlash::blank())
                .expect("Failed to initialize MemOtaFlash"),
            target_flash: MemTargetFlash::new(),
            reboot_required: false,
        }
    }
}

#[cfg(target_arch = "riscv32")]
impl FlashOtaDriver<EspOtaFlash, EspTargetFlash> {
    /// Construct a new `FlashOtaDriver` with the target hardware ESP32-C6 flash driver.
    pub fn new(public_key: [u8; 32]) -> Self {
        let bootloader =
            OtaBootloader::load().expect("Failed to load OTA bootloader from SPI flash");
        Self {
            public_key,
            check_esp_magic: true,
            bootloader,
            target_flash: EspTargetFlash,
            reboot_required: false,
        }
    }

    /// Attempt to construct a new `FlashOtaDriver`, returning an error if bootloader loading fails.
    pub fn try_new(public_key: [u8; 32]) -> Result<Self, BootloaderError> {
        let bootloader = OtaBootloader::load()?;
        Ok(Self {
            public_key,
            check_esp_magic: true,
            bootloader,
            target_flash: EspTargetFlash,
            reboot_required: false,
        })
    }

    /// Reload the bootloader state from hardware flash.
    pub fn reload_bootloader(&mut self) -> Result<(), BootloaderError> {
        self.bootloader = OtaBootloader::load()?;
        Ok(())
    }
}

impl<F: OtaFlashStorage, TF: OtaTargetFlash> FlashOtaDriver<F, TF> {
    /// Construct a new `FlashOtaDriver` with explicit bootloader and target flash instances.
    pub fn with_flash(
        bootloader: OtaBootloader<F>,
        target_flash: TF,
        public_key: [u8; 32],
    ) -> Self {
        Self {
            public_key,
            check_esp_magic: true,
            bootloader,
            target_flash,
            reboot_required: false,
        }
    }

    /// Enable or disable ESP32 image format magic validation (default: enabled).
    pub fn with_esp_magic_check(mut self, enabled: bool) -> Self {
        self.check_esp_magic = enabled;
        self
    }

    /// Access reference to the internal bootloader coordinator.
    pub fn bootloader(&self) -> &OtaBootloader<F> {
        &self.bootloader
    }

    /// Access mutable reference to the internal bootloader coordinator.
    pub fn bootloader_mut(&mut self) -> &mut OtaBootloader<F> {
        &mut self.bootloader
    }

    /// Access reference to the target flash storage.
    pub fn target_flash(&self) -> &TF {
        &self.target_flash
    }

    /// Access mutable reference to the target flash storage.
    pub fn target_flash_mut(&mut self) -> &mut TF {
        &mut self.target_flash
    }
}

impl<F: OtaFlashStorage, TF: OtaTargetFlash> OtaFlashDriver for FlashOtaDriver<F, TF> {
    async fn handle_flash_stream<S: OtaStream>(
        &mut self,
        stream: &mut S,
        content_length: usize,
    ) -> Result<(), OtaDriverError> {
        if self.reboot_required {
            return Err(OtaDriverError::RebootRequired);
        }

        if !(OTA_HEADER_LEN..=MAX_OTA_PAYLOAD_SIZE).contains(&content_length) {
            warn!(
                "FlashOtaDriver: invalid content length {} (expected {}..={})",
                content_length, OTA_HEADER_LEN, MAX_OTA_PAYLOAD_SIZE
            );
            return Err(OtaDriverError::InvalidHeader);
        }

        let target_slot = self.bootloader.get_target_slot();
        let next_seq = self.bootloader.get_next_seq();
        info!(
            "FlashOtaDriver: streaming update targeting slot={:?}, seq={}, length={}",
            target_slot, next_seq, content_length
        );

        let mut writer = OtaWriter::new(&mut self.target_flash, target_slot, self.public_key)
            .with_esp_magic_check(self.check_esp_magic);

        let mut chunk_buf = [0u8; CHUNK_BUF_SIZE];
        let mut total_received = 0;

        while total_received < content_length {
            Timer::after(COOPERATIVE_YIELD_DELAY).await;

            let to_read = core::cmp::min(chunk_buf.len(), content_length - total_received);
            let n = match stream.read(&mut chunk_buf[..to_read]).await {
                Ok(0) => {
                    warn!(
                        "FlashOtaDriver: stream terminated early ({}/{} bytes received)",
                        total_received, content_length
                    );
                    return Err(OtaDriverError::TruncatedStream);
                }
                Ok(n) => n,
                Err(_) => {
                    warn!(
                        "FlashOtaDriver: stream read error ({}/{} bytes received)",
                        total_received, content_length
                    );
                    return Err(OtaDriverError::TruncatedStream);
                }
            };

            writer.write_chunk(&chunk_buf[..n]).map_err(map_ota_error)?;
            total_received += n;
        }

        writer.finish().map_err(map_ota_error)?;

        // A failed metadata write may have partially committed. Keep this driver
        // closed until reboot even when the commit call reports an error.
        self.reboot_required = true;
        self.bootloader
            .mark_pending_verify(target_slot, next_seq)
            .map_err(|_| OtaDriverError::BootloaderCommitFailed)?;

        info!(
            "FlashOtaDriver: firmware update verified and committed to slot={:?}, seq={}",
            target_slot, next_seq
        );

        Ok(())
    }
}

// ============================================================================
// Public Constants
// ============================================================================

/// Cooperative yield duration between chunks during streaming OTA writes.
pub const COOPERATIVE_YIELD_DELAY: Duration = Duration::from_millis(1);

/// Maximum OTA payload size: 72-byte wire header + 3.4 MiB application slot.
pub const MAX_OTA_PAYLOAD_SIZE: usize = OTA_HEADER_LEN + MAX_APP_SLOT_SIZE;

// ============================================================================
// Private Constants & Helpers
// ============================================================================

/// Streaming chunk buffer size (2 KiB).
const CHUNK_BUF_SIZE: usize = 2048;

/// Map internal `OtaError` to domain `OtaDriverError`.
fn map_ota_error(err: OtaError) -> OtaDriverError {
    match err {
        OtaError::InvalidHeader
        | OtaError::InvalidSize
        | OtaError::ImageTooLarge
        | OtaError::InvalidImageFormat
        | OtaError::AlreadyFinished
        | OtaError::InvalidState => OtaDriverError::InvalidHeader,

        OtaError::TruncatedHeader | OtaError::TruncatedImage => OtaDriverError::TruncatedStream,

        OtaError::InvalidPublicKey | OtaError::InvalidSignature => {
            OtaDriverError::VerificationFailed
        }

        OtaError::FlashErase | OtaError::FlashWrite => OtaDriverError::FlashWriteFailed,
    }
}
