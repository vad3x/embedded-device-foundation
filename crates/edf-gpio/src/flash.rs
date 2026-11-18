//! ESP32 SPI Flash Driver implementing `embedded-storage-async` traits.
//!
//! Wraps ESP32 ROM SPI flash functions (`esp_rom_spiflash_{read,write,erase_sector}`)
//! for target hardware execution on `riscv32`, and provides an in-memory safe mock
//! when running host unit tests.

use core::ops::Range;

use embedded_storage::nor_flash::{ErrorType, NorFlashError, NorFlashErrorKind};
use embedded_storage_async::nor_flash::{MultiwriteNorFlash, NorFlash, ReadNorFlash};

// ============================================================================
// Public Constants
// ============================================================================

/// ESP32 SPI flash sector size (4 KiB = 4096 bytes).
pub const SECTOR_SIZE: usize = 4096;

/// ESP32 SPI flash write/read alignment (4 bytes = u32 word).
pub const WORD_SIZE: usize = 4;

// ============================================================================
// Public Structs & Implementation
// ============================================================================

/// Async NOR flash driver for internal SPI flash storage partitions.
///
/// Implements `ReadNorFlash`, `NorFlash`, and `MultiwriteNorFlash` over absolute flash offsets.
pub struct EspFlash {
    /// Flash range covered by this driver instance (e.g. `wifi_kvs` or `matter_kvs`).
    range: Range<u32>,
    storage: backend::FlashStorage,
}

impl EspFlash {
    /// Create a new flash driver instance covering the specified absolute flash range.
    pub fn new(range: Range<u32>) -> Self {
        let storage = backend::FlashStorage::new(&range);
        Self { range, storage }
    }

    /// Flash address range covered by this driver.
    pub fn range(&self) -> &Range<u32> {
        &self.range
    }
}

/// Errors from SPI flash operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EspFlashError {
    /// ROM function returned a non-zero error code.
    RomError(i32),
    /// Alignment requirement violated (4-byte for writes, 4096-byte for erase).
    NotAligned,
    /// Offset or length outside partition boundaries.
    OutOfBounds,
}

impl NorFlashError for EspFlashError {
    fn kind(&self) -> NorFlashErrorKind {
        match self {
            EspFlashError::RomError(_) => NorFlashErrorKind::Other,
            EspFlashError::NotAligned => NorFlashErrorKind::NotAligned,
            EspFlashError::OutOfBounds => NorFlashErrorKind::OutOfBounds,
        }
    }
}

impl ErrorType for EspFlash {
    type Error = EspFlashError;
}

impl ReadNorFlash for EspFlash {
    const READ_SIZE: usize = 1;

    async fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        self.storage.read(&self.range, offset, bytes)
    }

    fn capacity(&self) -> usize {
        (self.range.end.saturating_sub(self.range.start)) as usize
    }
}

impl NorFlash for EspFlash {
    const WRITE_SIZE: usize = WORD_SIZE;
    const ERASE_SIZE: usize = SECTOR_SIZE;

    async fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        self.storage.erase(&self.range, from, to)
    }

    async fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        self.storage.write(&self.range, offset, bytes)
    }
}

impl MultiwriteNorFlash for EspFlash {}

#[cfg(target_os = "none")]
mod backend {
    use core::ops::Range;

    use log::{debug, warn};

    use super::{EspFlashError, SECTOR_SIZE, WORD_SIZE};

    pub struct FlashStorage;

    impl FlashStorage {
        pub fn new(_range: &Range<u32>) -> Self {
            Self
        }

        pub fn read(&self, range: &Range<u32>, offset: u32, bytes: &mut [u8]) -> Result<(), EspFlashError> {
            if bytes.is_empty() {
                return Ok(());
            }
            if offset < range.start
                || (offset as usize).saturating_add(bytes.len()) > range.end as usize
            {
                return Err(EspFlashError::OutOfBounds);
            }

            let aligned_start = offset & !(WORD_SIZE as u32 - 1);
            let head_skip = (offset - aligned_start) as usize;
            let total_len = head_skip + bytes.len();
            let aligned_len = (total_len + WORD_SIZE - 1) & !(WORD_SIZE - 1);

            let mut buf = [0u32; 68];
            if aligned_len > buf.len() * WORD_SIZE {
                return self.read_chunked(range, offset, bytes);
            }

            let ret = unsafe {
                esp_hal::rom::spiflash::esp_rom_spiflash_read(
                    aligned_start,
                    buf.as_mut_ptr(),
                    aligned_len as u32,
                )
            };
            if ret != esp_hal::rom::spiflash::ESP_ROM_SPIFLASH_RESULT_OK {
                return Err(EspFlashError::RomError(ret));
            }

            let raw =
                unsafe { core::slice::from_raw_parts(buf.as_ptr() as *const u8, aligned_len) };
            bytes.copy_from_slice(&raw[head_skip..head_skip + bytes.len()]);
            Ok(())
        }

        fn read_chunked(
            &self,
            range: &Range<u32>,
            offset: u32,
            bytes: &mut [u8],
        ) -> Result<(), EspFlashError> {
            const CHUNK: usize = 240;
            let mut pos = 0;
            while pos < bytes.len() {
                let chunk_len = (bytes.len() - pos).min(CHUNK);
                self.read(range, offset + pos as u32, &mut bytes[pos..pos + chunk_len])?;
                pos += chunk_len;
            }
            Ok(())
        }

        pub fn write(&self, range: &Range<u32>, offset: u32, bytes: &[u8]) -> Result<(), EspFlashError> {
            if bytes.is_empty() {
                return Ok(());
            }
            if !(offset as usize).is_multiple_of(WORD_SIZE) || !bytes.len().is_multiple_of(WORD_SIZE)
            {
                return Err(EspFlashError::NotAligned);
            }
            if offset < range.start
                || (offset as usize).saturating_add(bytes.len()) > range.end as usize
            {
                return Err(EspFlashError::OutOfBounds);
            }

            let ret = unsafe { esp_hal::rom::spiflash::esp_rom_spiflash_unlock() };
            if ret != esp_hal::rom::spiflash::ESP_ROM_SPIFLASH_RESULT_OK {
                warn!("Flash unlock failed: {}", ret);
                return Err(EspFlashError::RomError(ret));
            }

            let ret = unsafe {
                esp_hal::rom::spiflash::esp_rom_spiflash_write(
                    offset,
                    bytes.as_ptr() as *const u32,
                    bytes.len() as u32,
                )
            };
            if ret != esp_hal::rom::spiflash::ESP_ROM_SPIFLASH_RESULT_OK {
                warn!("Flash write failed at 0x{:x}: {}", offset, ret);
                return Err(EspFlashError::RomError(ret));
            }
            Ok(())
        }

        pub fn erase(&self, range: &Range<u32>, from: u32, to: u32) -> Result<(), EspFlashError> {
            if !from.is_multiple_of(SECTOR_SIZE as u32) || !to.is_multiple_of(SECTOR_SIZE as u32) {
                return Err(EspFlashError::NotAligned);
            }
            if from < range.start || from > to || to > range.end {
                return Err(EspFlashError::OutOfBounds);
            }

            let ret = unsafe { esp_hal::rom::spiflash::esp_rom_spiflash_unlock() };
            if ret != esp_hal::rom::spiflash::ESP_ROM_SPIFLASH_RESULT_OK {
                warn!("Flash unlock failed: {}", ret);
                return Err(EspFlashError::RomError(ret));
            }

            let mut addr = from;
            while addr < to {
                let sector = addr / SECTOR_SIZE as u32;
                debug!("Erasing flash sector {} (0x{:x})", sector, addr);
                let ret = unsafe { esp_hal::rom::spiflash::esp_rom_spiflash_erase_sector(sector) };
                if ret != esp_hal::rom::spiflash::ESP_ROM_SPIFLASH_RESULT_OK {
                    warn!("Flash erase sector {} failed: {}", sector, ret);
                    return Err(EspFlashError::RomError(ret));
                }
                addr += SECTOR_SIZE as u32;
            }
            Ok(())
        }
    }
}

#[cfg(not(target_os = "none"))]
mod backend {
    use core::cell::RefCell;
    use core::ops::Range;

    use super::{EspFlashError, SECTOR_SIZE, WORD_SIZE};

    pub struct FlashStorage {
        mock_data: RefCell<alloc::vec::Vec<u8>>,
    }

    impl FlashStorage {
        pub fn new(range: &Range<u32>) -> Self {
            let size = (range.end.saturating_sub(range.start)) as usize;
            Self {
                mock_data: RefCell::new(alloc::vec![0xff; size]),
            }
        }

        pub fn read(&self, range: &Range<u32>, offset: u32, bytes: &mut [u8]) -> Result<(), EspFlashError> {
            if bytes.is_empty() {
                return Ok(());
            }
            if offset < range.start
                || (offset as usize).saturating_add(bytes.len()) > range.end as usize
            {
                return Err(EspFlashError::OutOfBounds);
            }
            let start = (offset - range.start) as usize;
            let end = start + bytes.len();
            bytes.copy_from_slice(&self.mock_data.borrow()[start..end]);
            Ok(())
        }

        pub fn write(&self, range: &Range<u32>, offset: u32, bytes: &[u8]) -> Result<(), EspFlashError> {
            if bytes.is_empty() {
                return Ok(());
            }
            if !(offset as usize).is_multiple_of(WORD_SIZE) || !bytes.len().is_multiple_of(WORD_SIZE)
            {
                return Err(EspFlashError::NotAligned);
            }
            if offset < range.start
                || (offset as usize).saturating_add(bytes.len()) > range.end as usize
            {
                return Err(EspFlashError::OutOfBounds);
            }
            let start = (offset - range.start) as usize;
            let mut mock = self.mock_data.borrow_mut();
            for (i, &b) in bytes.iter().enumerate() {
                mock[start + i] &= b; // Hardware NOR flash can only clear bits
            }
            Ok(())
        }

        pub fn erase(&self, range: &Range<u32>, from: u32, to: u32) -> Result<(), EspFlashError> {
            if !from.is_multiple_of(SECTOR_SIZE as u32) || !to.is_multiple_of(SECTOR_SIZE as u32) {
                return Err(EspFlashError::NotAligned);
            }
            if from < range.start || from > to || to > range.end {
                return Err(EspFlashError::OutOfBounds);
            }
            let start = (from - range.start) as usize;
            let end = (to - range.start) as usize;
            self.mock_data.borrow_mut()[start..end].fill(0xff);
            Ok(())
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
    fn host_mock_erase_and_write_flow() {
        // Arrange
        let range = 0x6c0000..0x6c2000; // 8 KiB (2 sectors)
        let mut flash = EspFlash::new(range.clone());
        assert_eq!(flash.capacity(), 8192);

        // Act 1: Erase first sector
        let erase_res = futures::executor::block_on(flash.erase(0x6c0000, 0x6c1000));
        assert_eq!(erase_res, Ok(()));

        // Act 2: Write 4-byte word
        let data = [0x12, 0x34, 0x56, 0x78];
        let write_res = futures::executor::block_on(flash.write(0x6c0000, &data));
        assert_eq!(write_res, Ok(()));

        // Act 3: Read back
        let mut read_buf = [0u8; 4];
        let read_res = futures::executor::block_on(flash.read(0x6c0000, &mut read_buf));

        // Assert
        assert_eq!(read_res, Ok(()));
        assert_eq!(read_buf, data);
    }

    #[test]
    fn rejects_unaligned_erase_and_write() {
        // Arrange
        let mut flash = EspFlash::new(0x6c0000..0x6c2000);

        // Act: Erase not 4096-aligned
        let unaligned_erase = futures::executor::block_on(flash.erase(0x6c0000, 0x6c0100));
        // Act: Write not 4-byte aligned
        let unaligned_write = futures::executor::block_on(flash.write(0x6c0001, &[1, 2, 3, 4]));

        // Assert
        assert_eq!(unaligned_erase, Err(EspFlashError::NotAligned));
        assert_eq!(unaligned_write, Err(EspFlashError::NotAligned));
    }

    #[test]
    fn rejects_out_of_bounds_access() {
        // Arrange
        let mut flash = EspFlash::new(0x6c0000..0x6c2000);

        // Act & Assert
        let oob_read = futures::executor::block_on(flash.read(0x6bf000, &mut [0u8; 4]));
        assert_eq!(oob_read, Err(EspFlashError::OutOfBounds));

        let oob_write = futures::executor::block_on(flash.write(0x6c2000, &[0u8; 4]));
        assert_eq!(oob_write, Err(EspFlashError::OutOfBounds));

        let oob_erase = futures::executor::block_on(flash.erase(0x6c0000, 0x6c3000));
        assert_eq!(oob_erase, Err(EspFlashError::OutOfBounds));

        let oob_erase_under = futures::executor::block_on(flash.erase(0x6bf000, 0x6c1000));
        assert_eq!(oob_erase_under, Err(EspFlashError::OutOfBounds));

        let oob_erase_inverted = futures::executor::block_on(flash.erase(0x6c1000, 0x6c0000));
        assert_eq!(oob_erase_inverted, Err(EspFlashError::OutOfBounds));
    }

    #[test]
    fn host_unaligned_and_large_reads() {
        // Arrange
        let range = 0x6c0000..0x6c2000;
        let mut flash = EspFlash::new(range);

        // Erase and write 512 bytes across sectors
        futures::executor::block_on(flash.erase(0x6c0000, 0x6c1000)).unwrap();
        let pattern: alloc::vec::Vec<u8> = (0..512).map(|i| (i & 0xff) as u8).collect();
        futures::executor::block_on(flash.write(0x6c0000, &pattern)).unwrap();

        // Act: Unaligned read of 300 bytes at offset 0x6c0003
        let mut unaligned_buf = [0u8; 300];
        futures::executor::block_on(flash.read(0x6c0003, &mut unaligned_buf)).unwrap();

        // Assert
        assert_eq!(unaligned_buf, &pattern[3..303]);
    }

    #[test]
    fn unaligned_chunk_fits_in_buffer_without_recursion() {
        const CHUNK: usize = 240;
        const BUF_CAPACITY: usize = 68 * WORD_SIZE; // 272 bytes

        for offset in 0..16u32 {
            for len in 0..=CHUNK {
                let aligned_start = offset & !(WORD_SIZE as u32 - 1);
                let head_skip = (offset - aligned_start) as usize;
                let total_len = head_skip + len;
                let aligned_len = (total_len + WORD_SIZE - 1) & !(WORD_SIZE - 1);

                assert!(
                    aligned_len <= BUF_CAPACITY,
                    "aligned_len {} exceeded buffer capacity {} for offset {}, len {}",
                    aligned_len,
                    BUF_CAPACITY,
                    offset,
                    len
                );
            }
        }
    }
}
