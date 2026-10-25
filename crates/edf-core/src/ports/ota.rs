//! Shared OTA firmware update ports.
//!
//! Over-The-Air (OTA) firmware update ports and error types.

/// Abstract bidirectional byte stream for OTA transport.
#[allow(async_fn_in_trait)]
pub trait OtaStream {
    /// Read incoming bytes into the provided buffer. Returns bytes read, or Err on failure.
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, ()>;

    /// Write output bytes to the stream.
    async fn write_all(&mut self, buf: &[u8]) -> Result<(), ()>;

    /// Flush any pending buffered writes.
    async fn flush(&mut self) -> Result<(), ()>;
}

/// Abstract flash driver for streaming and committing OTA firmware updates.
#[allow(async_fn_in_trait)]
pub trait OtaFlashDriver {
    /// Stream, cryptographically verify, and flash a firmware image.
    async fn handle_flash_stream<S: OtaStream>(
        &mut self,
        stream: &mut S,
        content_length: usize,
    ) -> Result<(), OtaDriverError>;
}

/// Errors returned by OTA flash operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OtaDriverError {
    /// Firmware header or magic bytes were invalid.
    InvalidHeader,
    /// Cryptographic signature verification failed.
    VerificationFailed,
    /// Flash sector erase or write failed.
    FlashWriteFailed,
    /// Stream terminated prematurely before all expected bytes were received.
    TruncatedStream,
    /// Bootloader otadata state transition failed.
    BootloaderCommitFailed,
    /// A previous update reached boot metadata commit; reboot before another upload.
    RebootRequired,
}

impl<T: OtaFlashDriver> OtaFlashDriver for &mut T {
    async fn handle_flash_stream<S: OtaStream>(
        &mut self,
        stream: &mut S,
        content_length: usize,
    ) -> Result<(), OtaDriverError> {
        (**self).handle_flash_stream(stream, content_length).await
    }
}
