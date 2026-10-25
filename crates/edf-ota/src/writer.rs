//! Flash Sector Writer & Ed25519 Signature Verifier
//!
//! Streams incoming firmware bytes, parses the 72-byte wire header, buffers incoming
//! chunks into 4 KiB sector-aligned blocks, writes sectors to the target partition
//! (`ota_0` or `ota_1`), and verifies an Ed25519 signature over the binary image
//! before reporting completion.

use crate::bootloader::Slot;
use core::fmt;
use ed25519_dalek::{Signature, StreamVerifier, VerifyingKey};

// ============================================================================
// Public Constants
// ============================================================================

/// Wire header magic bytes: ASCII `b"ICOT"`.
pub const OTA_HEADER_MAGIC: &[u8; 4] = b"ICOT";

/// Byte length of the OTA stream header:
/// 4 bytes magic (`ICOT`) + 4 bytes length (`u32` LE) + 64 bytes Ed25519 signature.
pub const OTA_HEADER_LEN: usize = 72;

/// Maximum allowable application binary size (3,473,408 bytes = 0x350000 = 3.3125 MiB).
pub const MAX_APP_SLOT_SIZE: usize = 0x350000;

/// Flash sector size for ESP32 NOR flash (4 KiB = 4096 bytes).
pub const SECTOR_SIZE: usize = 4096;

/// Start offset of `ota_0` app partition (Slot A, from `partitions.csv`).
pub const OTA_0_OFFSET: u32 = 0x020000;

/// Start offset of `ota_1` app partition (Slot B, from `partitions.csv`).
pub const OTA_1_OFFSET: u32 = 0x370000;

/// Partition slot capacity (3,473,408 bytes).
pub const OTA_SLOT_SIZE: u32 = 0x350000;

/// ESP-IDF firmware binary image header magic byte (`0xE9`).
pub const ESP_IMAGE_MAGIC: u8 = 0xE9;

/// ESP-IDF chip ID for ESP32-C6 (`0x000D` little-endian at offset 12..14).
pub const ESP32_C6_CHIP_ID: u16 = 0x000D;

// ============================================================================
// Public Main Struct & Implementation
// ============================================================================

/// Streaming OTA flash writer and cryptographic verifier.
///
/// Buffers incoming network byte chunks into 4 KiB sector-aligned blocks, writes them
/// to the inactive OTA partition, and verifies the Ed25519 signature before committing.
pub struct OtaWriter<F = DefaultTargetFlash> {
    flash: F,
    target_slot: Slot,
    public_key: [u8; 32],
    header_buf: [u8; OTA_HEADER_LEN],
    header_len: usize,
    image_len: usize,
    received_image_bytes: usize,
    flashed_bytes: usize,
    sector_buf_len: usize,
    sector_buf: SectorBuffer,
    stream_verifier: Option<StreamVerifier>,
    check_esp_magic: bool,
    esp_magic_checked: bool,
    magic_prefix: [u8; 14],
    magic_prefix_len: usize,
    finished: bool,
}

impl<F: OtaTargetFlash> OtaWriter<F> {
    /// Construct a new streaming writer targeting `target_slot` and verified with `public_key`.
    pub fn new(flash: F, target_slot: Slot, public_key: [u8; 32]) -> Self {
        Self {
            flash,
            target_slot,
            public_key,
            header_buf: [0u8; OTA_HEADER_LEN],
            header_len: 0,
            image_len: 0,
            received_image_bytes: 0,
            flashed_bytes: 0,
            sector_buf_len: 0,
            sector_buf: SectorBuffer([0xffu8; SECTOR_SIZE]),
            stream_verifier: None,
            check_esp_magic: true,
            esp_magic_checked: false,
            magic_prefix: [0u8; 14],
            magic_prefix_len: 0,
            finished: false,
        }
    }

    /// Enable or disable ESP32 image format magic validation (default: enabled).
    pub fn with_esp_magic_check(mut self, enabled: bool) -> Self {
        self.check_esp_magic = enabled;
        self
    }

    /// Feed an arbitrary incoming chunk of bytes from the network into the writer.
    ///
    /// Once the 72-byte wire header is assembled, incoming bytes are streamed into
    /// the cryptographic verifier and buffered into 4 KiB sectors written directly to flash.
    pub fn write_chunk(&mut self, mut chunk: &[u8]) -> Result<(), OtaError> {
        if self.finished {
            return Err(OtaError::AlreadyFinished);
        }

        // 1. Process 72-byte wire header if not yet complete
        if self.header_len < OTA_HEADER_LEN {
            let needed = OTA_HEADER_LEN - self.header_len;
            let to_copy = chunk.len().min(needed);
            self.header_buf[self.header_len..self.header_len + to_copy]
                .copy_from_slice(&chunk[..to_copy]);
            self.header_len += to_copy;
            chunk = &chunk[to_copy..];

            if self.header_len == OTA_HEADER_LEN {
                self.parse_and_init_header()?;
            }
        }

        if chunk.is_empty() {
            return Ok(());
        }

        // 2. Validate bounds
        if self.received_image_bytes + chunk.len() > self.image_len {
            return Err(OtaError::ImageTooLarge);
        }

        // 3. Validate ESP32 image header prefix if enabled
        if self.check_esp_magic && !self.esp_magic_checked {
            self.accumulate_and_check_esp_prefix(chunk)?;
        }

        // 4. Update signature stream verifier
        if let Some(ref mut verifier) = self.stream_verifier {
            verifier.update(chunk);
        }
        self.received_image_bytes += chunk.len();

        // 5. Buffer bytes into 4 KiB sectors and flash complete sectors
        let mut remaining = chunk;
        while !remaining.is_empty() {
            let space = SECTOR_SIZE - self.sector_buf_len;
            let to_buffer = remaining.len().min(space);
            self.sector_buf.0[self.sector_buf_len..self.sector_buf_len + to_buffer]
                .copy_from_slice(&remaining[..to_buffer]);
            self.sector_buf_len += to_buffer;
            remaining = &remaining[to_buffer..];

            if self.sector_buf_len == SECTOR_SIZE {
                self.flush_current_sector()?;
            }
        }

        Ok(())
    }

    /// Finalize the flash write process, flush any trailing partial sector, and
    /// cryptographically verify the Ed25519 signature over the entire image.
    pub fn finish(&mut self) -> Result<(), OtaError> {
        if self.finished {
            return Err(OtaError::AlreadyFinished);
        }

        if self.header_len < OTA_HEADER_LEN {
            return Err(OtaError::TruncatedHeader);
        }

        if self.received_image_bytes < self.image_len {
            return Err(OtaError::TruncatedImage);
        }

        if self.check_esp_magic && !self.esp_magic_checked {
            return Err(OtaError::InvalidImageFormat);
        }

        // Flush final partial sector (padded with 0xFF)
        if self.sector_buf_len > 0 {
            self.flush_current_sector()?;
        }

        // Finalize Ed25519 signature verification
        let verifier = self.stream_verifier.take().ok_or(OtaError::InvalidState)?;
        verifier
            .finalize_and_verify()
            .map_err(|_| OtaError::InvalidSignature)?;

        self.finished = true;
        Ok(())
    }

    /// Returns `true` if the writer has successfully finished and verified.
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// Returns the target partition slot.
    pub fn target_slot(&self) -> Slot {
        self.target_slot
    }

    /// Returns the expected image length from the wire header.
    pub fn image_len(&self) -> usize {
        self.image_len
    }

    /// Returns the total image bytes received so far.
    pub fn received_image_bytes(&self) -> usize {
        self.received_image_bytes
    }

    /// Returns the total bytes flushed to flash so far.
    pub fn flashed_bytes(&self) -> usize {
        self.flashed_bytes
    }

    // ------------------------------------------------------------------------
    // Private Helpers
    // ------------------------------------------------------------------------

    fn parse_and_init_header(&mut self) -> Result<(), OtaError> {
        if &self.header_buf[0..4] != OTA_HEADER_MAGIC {
            return Err(OtaError::InvalidHeader);
        }

        let raw_len = u32::from_le_bytes(
            self.header_buf[4..8]
                .try_into()
                .map_err(|_| OtaError::InvalidSize)?,
        );

        if raw_len == 0 {
            return Err(OtaError::InvalidSize);
        }

        if (raw_len as usize) > MAX_APP_SLOT_SIZE {
            return Err(OtaError::ImageTooLarge);
        }

        self.image_len = raw_len as usize;

        let sig_bytes: [u8; 64] = self.header_buf[8..72]
            .try_into()
            .map_err(|_| OtaError::InvalidSignature)?;
        let signature = Signature::from_bytes(&sig_bytes);

        let verifying_key = VerifyingKey::from_bytes(&self.public_key)
            .map_err(|_| OtaError::InvalidPublicKey)?;

        let verifier = verifying_key
            .verify_stream(&signature)
            .map_err(|_| OtaError::InvalidSignature)?;

        self.stream_verifier = Some(verifier);
        Ok(())
    }

    fn accumulate_and_check_esp_prefix(&mut self, chunk: &[u8]) -> Result<(), OtaError> {
        let needed = 14 - self.magic_prefix_len;
        let to_copy = chunk.len().min(needed);
        self.magic_prefix[self.magic_prefix_len..self.magic_prefix_len + to_copy]
            .copy_from_slice(&chunk[..to_copy]);
        self.magic_prefix_len += to_copy;

        if self.magic_prefix_len >= 1 && self.magic_prefix[0] != ESP_IMAGE_MAGIC {
            return Err(OtaError::InvalidImageFormat);
        }

        if self.magic_prefix_len >= 14 || self.magic_prefix_len == self.image_len {
            validate_esp_image_header(&self.magic_prefix[..self.magic_prefix_len])?;
            self.esp_magic_checked = true;
        }

        Ok(())
    }

    fn flush_current_sector(&mut self) -> Result<(), OtaError> {
        if self.sector_buf_len == 0 {
            return Ok(());
        }

        // Pad partial sector with erased flash value 0xFF
        if self.sector_buf_len < SECTOR_SIZE {
            self.sector_buf.0[self.sector_buf_len..SECTOR_SIZE].fill(0xff);
        }

        let flash_offset = self.target_slot.flash_offset() + self.flashed_bytes as u32;

        self.flash
            .erase_sector(flash_offset)
            .map_err(|_| OtaError::FlashErase)?;

        self.flash
            .write_sector(flash_offset, &self.sector_buf.0)
            .map_err(|_| OtaError::FlashWrite)?;

        self.flashed_bytes += SECTOR_SIZE;
        self.sector_buf_len = 0;
        self.sector_buf.0.fill(0xff);

        Ok(())
    }
}

// ============================================================================
// Public Functions
// ============================================================================

/// Verify an Ed25519 signature over a complete binary payload in memory.
pub fn verify_image_signature(
    public_key: &[u8; 32],
    image: &[u8],
    signature: &[u8; 64],
) -> Result<(), OtaError> {
    let verifying_key =
        VerifyingKey::from_bytes(public_key).map_err(|_| OtaError::InvalidPublicKey)?;
    let sig = Signature::from_bytes(signature);
    verifying_key
        .verify_strict(image, &sig)
        .map_err(|_| OtaError::InvalidSignature)
}

/// Validate ESP-IDF firmware binary header format.
///
/// Verifies that byte 0 is `0xE9` (`ESP_IMAGE_MAGIC`), and if at least 14 bytes
/// are present, that bytes 12..14 match ESP32-C6 Chip ID `0x000D`.
pub fn validate_esp_image_header(header: &[u8]) -> Result<(), OtaError> {
    if header.is_empty() {
        return Err(OtaError::InvalidImageFormat);
    }
    if header[0] != ESP_IMAGE_MAGIC {
        return Err(OtaError::InvalidImageFormat);
    }
    if header.len() >= 14 {
        let chip_id = u16::from_le_bytes([header[12], header[13]]);
        if chip_id != ESP32_C6_CHIP_ID {
            return Err(OtaError::InvalidImageFormat);
        }
    }
    Ok(())
}

// ============================================================================
// Public Supporting Types & Traits
// ============================================================================

/// Target NOR flash abstraction for writing application firmware partitions.
pub trait OtaTargetFlash {
    type Error: core::fmt::Debug;

    /// Erase a 4 KiB flash sector starting at `offset`.
    fn erase_sector(&mut self, offset: u32) -> Result<(), Self::Error>;

    /// Write a 4 KiB aligned block of data at `offset`.
    fn write_sector(&mut self, offset: u32, data: &[u8; SECTOR_SIZE]) -> Result<(), Self::Error>;
}

/// 4-byte aligned sector buffer wrapper.
#[repr(C, align(4))]
pub struct SectorBuffer(pub [u8; SECTOR_SIZE]);

/// Errors occurring during streaming OTA flash writing or signature verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OtaError {
    /// Wire header magic bytes were not `b"ICOT"`.
    InvalidHeader,
    /// Stream ended before completing the 72-byte wire header.
    TruncatedHeader,
    /// Firmware image length was 0.
    InvalidSize,
    /// Firmware binary exceeds slot capacity (0x350000) or advertised length.
    ImageTooLarge,
    /// Stream closed before receiving the full advertised image length.
    TruncatedImage,
    /// Firmware binary header does not conform to ESP-IDF format (magic `0xE9` / chip ID).
    InvalidImageFormat,
    /// Provided Ed25519 public key is invalid or malformed.
    InvalidPublicKey,
    /// Cryptographic Ed25519 signature verification failed.
    InvalidSignature,
    /// Flash sector erase operation failed.
    FlashErase,
    /// Flash sector write operation failed.
    FlashWrite,
    /// Attempted to write to or finish a writer that has already completed.
    AlreadyFinished,
    /// Writer was in an invalid internal state.
    InvalidState,
}

impl fmt::Display for OtaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OtaError::InvalidHeader => write!(f, "Invalid OTA header magic (expected 'ICOT')"),
            OtaError::TruncatedHeader => write!(f, "Stream ended before 72-byte header completed"),
            OtaError::InvalidSize => write!(f, "Image size cannot be 0"),
            OtaError::ImageTooLarge => write!(f, "Image size exceeds maximum partition slot size"),
            OtaError::TruncatedImage => write!(f, "Stream ended before full image was received"),
            OtaError::InvalidImageFormat => write!(f, "Invalid ESP32 binary format (magic/chip ID)"),
            OtaError::InvalidPublicKey => write!(f, "Invalid Ed25519 public key"),
            OtaError::InvalidSignature => write!(f, "Ed25519 signature verification failed"),
            OtaError::FlashErase => write!(f, "Flash sector erase failed"),
            OtaError::FlashWrite => write!(f, "Flash sector write failed"),
            OtaError::AlreadyFinished => write!(f, "OTA writer already finished"),
            OtaError::InvalidState => write!(f, "Invalid internal writer state"),
        }
    }
}

/// In-memory implementation of `OtaTargetFlash` for host testing.
#[derive(Debug, Clone)]
pub struct MemTargetFlash {
    /// Map of sector offset to sector bytes.
    pub sectors: alloc::collections::BTreeMap<u32, [u8; SECTOR_SIZE]>,
    /// List of sector offsets erased.
    pub erased_sectors: alloc::vec::Vec<u32>,
    /// Optional offset on which erase operations should return an error.
    pub fail_erase_offset: Option<u32>,
    /// Optional offset on which write operations should return an error.
    pub fail_write_offset: Option<u32>,
}

impl MemTargetFlash {
    /// Create a new in-memory flash mock.
    pub fn new() -> Self {
        Self {
            sectors: alloc::collections::BTreeMap::new(),
            erased_sectors: alloc::vec::Vec::new(),
            fail_erase_offset: None,
            fail_write_offset: None,
        }
    }

    /// Read contiguous byte range starting at `base_offset` across written sectors.
    pub fn read_bytes(&self, base_offset: u32, len: usize) -> alloc::vec::Vec<u8> {
        let mut out = alloc::vec::Vec::with_capacity(len);
        let mut curr_offset = base_offset;
        let end_offset = base_offset + len as u32;

        while curr_offset < end_offset {
            let sector_start = (curr_offset / SECTOR_SIZE as u32) * SECTOR_SIZE as u32;
            let offset_in_sec = (curr_offset - sector_start) as usize;
            let bytes_in_sec =
                (SECTOR_SIZE - offset_in_sec).min((end_offset - curr_offset) as usize);

            if let Some(sector) = self.sectors.get(&sector_start) {
                out.extend_from_slice(&sector[offset_in_sec..offset_in_sec + bytes_in_sec]);
            } else {
                out.extend(core::iter::repeat(0xff).take(bytes_in_sec));
            }
            curr_offset += bytes_in_sec as u32;
        }

        out
    }
}

impl Default for MemTargetFlash {
    fn default() -> Self {
        Self::new()
    }
}

/// Error type for in-memory mock flash operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemFlashError {
    SimulatedEraseError,
    SimulatedWriteError,
}

impl OtaTargetFlash for MemTargetFlash {
    type Error = MemFlashError;

    fn erase_sector(&mut self, offset: u32) -> Result<(), Self::Error> {
        if self.fail_erase_offset == Some(offset) {
            return Err(MemFlashError::SimulatedEraseError);
        }
        self.erased_sectors.push(offset);
        let blank = [0xffu8; SECTOR_SIZE];
        self.sectors.insert(offset, blank);
        Ok(())
    }

    fn write_sector(&mut self, offset: u32, data: &[u8; SECTOR_SIZE]) -> Result<(), Self::Error> {
        if self.fail_write_offset == Some(offset) {
            return Err(MemFlashError::SimulatedWriteError);
        }
        let mut buf = [0u8; SECTOR_SIZE];
        buf.copy_from_slice(data);
        self.sectors.insert(offset, buf);
        Ok(())
    }
}

// ============================================================================
// Target Hardware Flash Implementation (RISC-V only)
// ============================================================================

#[cfg(target_arch = "riscv32")]
pub type DefaultTargetFlash = EspTargetFlash;

#[cfg(not(target_arch = "riscv32"))]
pub type DefaultTargetFlash = MemTargetFlash;

#[cfg(target_arch = "riscv32")]
pub struct EspTargetFlash;

#[cfg(target_arch = "riscv32")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EspFlashError {
    Rom(i32),
    Unaligned,
}

#[cfg(target_arch = "riscv32")]
impl OtaTargetFlash for EspTargetFlash {
    type Error = EspFlashError;

    fn erase_sector(&mut self, offset: u32) -> Result<(), Self::Error> {
        if offset % SECTOR_SIZE as u32 != 0 {
            return Err(EspFlashError::Unaligned);
        }
        let sector_num = offset / SECTOR_SIZE as u32;

        let ret = unsafe { esp_hal::rom::spiflash::esp_rom_spiflash_unlock() };
        if ret != esp_hal::rom::spiflash::ESP_ROM_SPIFLASH_RESULT_OK {
            return Err(EspFlashError::Rom(ret));
        }

        let ret = unsafe { esp_hal::rom::spiflash::esp_rom_spiflash_erase_sector(sector_num) };
        if ret != esp_hal::rom::spiflash::ESP_ROM_SPIFLASH_RESULT_OK {
            return Err(EspFlashError::Rom(ret));
        }

        Ok(())
    }

    fn write_sector(&mut self, offset: u32, data: &[u8; SECTOR_SIZE]) -> Result<(), Self::Error> {
        if offset % 4 != 0 {
            return Err(EspFlashError::Unaligned);
        }

        let ret = unsafe { esp_hal::rom::spiflash::esp_rom_spiflash_unlock() };
        if ret != esp_hal::rom::spiflash::ESP_ROM_SPIFLASH_RESULT_OK {
            return Err(EspFlashError::Rom(ret));
        }

        let ret = unsafe {
            esp_hal::rom::spiflash::esp_rom_spiflash_write(
                offset,
                data.as_ptr() as *const u32,
                SECTOR_SIZE as u32,
            )
        };
        if ret != esp_hal::rom::spiflash::ESP_ROM_SPIFLASH_RESULT_OK {
            return Err(EspFlashError::Rom(ret));
        }

        Ok(())
    }
}

impl<F: OtaTargetFlash> OtaTargetFlash for &mut F {
    type Error = F::Error;

    fn erase_sector(&mut self, offset: u32) -> Result<(), Self::Error> {
        (**self).erase_sector(offset)
    }

    fn write_sector(&mut self, offset: u32, data: &[u8; SECTOR_SIZE]) -> Result<(), Self::Error> {
        (**self).write_sector(offset, data)
    }
}

// ============================================================================
// Unit Tests (Host-Runnable)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;
    use ed25519_dalek::{Signer, SigningKey};

    fn generate_test_keys() -> ([u8; 32], SigningKey) {
        let seed = [0x42u8; 32];
        let signing_key = SigningKey::from_bytes(&seed);
        let pubkey = signing_key.verifying_key().to_bytes();
        (pubkey, signing_key)
    }

    fn sign_payload(signing_key: &SigningKey, payload: &[u8]) -> [u8; 64] {
        signing_key.sign(payload).to_bytes()
    }

    fn build_test_esp_binary(len: usize) -> Vec<u8> {
        let mut bin = vec![0xaa; len];
        bin[0] = ESP_IMAGE_MAGIC;
        if len >= 14 {
            bin[12] = (ESP32_C6_CHIP_ID & 0xff) as u8;
            bin[13] = ((ESP32_C6_CHIP_ID >> 8) & 0xff) as u8;
        }
        bin
    }

    fn build_wire_packet(image: &[u8], signing_key: &SigningKey) -> Vec<u8> {
        let sig = sign_payload(signing_key, image);
        let mut packet = Vec::with_capacity(OTA_HEADER_LEN + image.len());
        packet.extend_from_slice(OTA_HEADER_MAGIC);
        packet.extend_from_slice(&(image.len() as u32).to_le_bytes());
        packet.extend_from_slice(&sig);
        packet.extend_from_slice(image);
        packet
    }

    #[test]
    fn rejects_image_with_tampered_payload() {
        // Arrange
        let (pubkey, privkey) = generate_test_keys();
        let payload = vec![0x12; 8192];
        let sig = sign_payload(&privkey, &payload);
        let mut tampered = payload.clone();
        tampered[100] ^= 0x01; // flip 1 bit

        // Act
        let result = verify_image_signature(&pubkey, &tampered, &sig);

        // Assert
        assert!(result.is_err());
    }

    #[test]
    fn verifies_valid_signature_over_image() {
        // Arrange
        let (pubkey, privkey) = generate_test_keys();
        let payload = vec![0x55; 16384];
        let sig = sign_payload(&privkey, &payload);

        // Act
        let result = verify_image_signature(&pubkey, &payload, &sig);

        // Assert
        assert!(result.is_ok());
    }

    #[test]
    fn ota_writer_happy_path_with_sector_flushing_and_verification() {
        // Arrange
        let (pubkey, privkey) = generate_test_keys();
        let flash = MemTargetFlash::new();
        let mut writer = OtaWriter::new(flash, Slot::Ota0, pubkey);

        let image = build_test_esp_binary(9500); // spans 2 full sectors + 1 partial sector
        let packet = build_wire_packet(&image, &privkey);

        // Act: Stream packet in arbitrary irregular chunks
        let chunk_sizes = [50, 1000, 2048, 100, 4096, 500];
        let mut offset = 0;
        let mut chunk_idx = 0;
        while offset < packet.len() {
            let size = chunk_sizes[chunk_idx % chunk_sizes.len()].min(packet.len() - offset);
            writer.write_chunk(&packet[offset..offset + size]).unwrap();
            offset += size;
            chunk_idx += 1;
        }
        let finish_res = writer.finish();

        // Assert
        assert!(finish_res.is_ok());
        assert!(writer.is_finished());
        assert_eq!(writer.image_len(), 9500);
        assert_eq!(writer.received_image_bytes(), 9500);
        assert_eq!(writer.flashed_bytes(), 3 * SECTOR_SIZE);

        let flashed_data = writer.flash.read_bytes(Slot::Ota0.flash_offset(), 9500);
        assert_eq!(flashed_data, image);

        // Verify trailing bytes of the 3rd sector are padded with 0xFF
        let padding = writer
            .flash
            .read_bytes(Slot::Ota0.flash_offset() + 9500, (3 * SECTOR_SIZE) - 9500);
        assert!(padding.iter().all(|&b| b == 0xff));
    }

    #[test]
    fn ota_writer_rejects_invalid_wire_magic() {
        // Arrange
        let (pubkey, _) = generate_test_keys();
        let flash = MemTargetFlash::new();
        let mut writer = OtaWriter::new(flash, Slot::Ota0, pubkey);

        let mut invalid_header = [0u8; 72];
        invalid_header[0..4].copy_from_slice(b"BAD!");

        // Act
        let res = writer.write_chunk(&invalid_header);

        // Assert
        assert_eq!(res, Err(OtaError::InvalidHeader));
    }

    #[test]
    fn ota_writer_rejects_payload_exceeding_max_slot_size() {
        // Arrange
        let (pubkey, _) = generate_test_keys();
        let flash = MemTargetFlash::new();
        let mut writer = OtaWriter::new(flash, Slot::Ota0, pubkey);

        let mut header = [0u8; 72];
        header[0..4].copy_from_slice(OTA_HEADER_MAGIC);
        let excessive_len = (MAX_APP_SLOT_SIZE + 1) as u32;
        header[4..8].copy_from_slice(&excessive_len.to_le_bytes());

        // Act
        let res = writer.write_chunk(&header);

        // Assert
        assert_eq!(res, Err(OtaError::ImageTooLarge));
    }

    #[test]
    fn ota_writer_rejects_zero_length_image() {
        // Arrange
        let (pubkey, _) = generate_test_keys();
        let flash = MemTargetFlash::new();
        let mut writer = OtaWriter::new(flash, Slot::Ota0, pubkey);

        let mut header = [0u8; 72];
        header[0..4].copy_from_slice(OTA_HEADER_MAGIC);
        header[4..8].copy_from_slice(&0u32.to_le_bytes());

        // Act
        let res = writer.write_chunk(&header);

        // Assert
        assert_eq!(res, Err(OtaError::InvalidSize));
    }

    #[test]
    fn ota_writer_rejects_tampered_payload_stream() {
        // Arrange
        let (pubkey, privkey) = generate_test_keys();
        let flash = MemTargetFlash::new();
        let mut writer = OtaWriter::new(flash, Slot::Ota1, pubkey);

        let image = build_test_esp_binary(5000);
        let mut packet = build_wire_packet(&image, &privkey);
        // Tamper with a payload byte (offset 72 is start of image)
        packet[72 + 250] ^= 0x40;

        // Act
        writer.write_chunk(&packet).unwrap();
        let res = writer.finish();

        // Assert
        assert_eq!(res, Err(OtaError::InvalidSignature));
        assert!(!writer.is_finished());
    }

    #[test]
    fn ota_writer_rejects_truncated_image() {
        // Arrange
        let (pubkey, privkey) = generate_test_keys();
        let flash = MemTargetFlash::new();
        let mut writer = OtaWriter::new(flash, Slot::Ota0, pubkey);

        let image = build_test_esp_binary(6000);
        let packet = build_wire_packet(&image, &privkey);

        // Act: Stream only header + half of payload, then call finish
        writer.write_chunk(&packet[..OTA_HEADER_LEN + 3000]).unwrap();
        let res = writer.finish();

        // Assert
        assert_eq!(res, Err(OtaError::TruncatedImage));
    }

    #[test]
    fn ota_writer_rejects_overflow_image() {
        // Arrange
        let (pubkey, privkey) = generate_test_keys();
        let flash = MemTargetFlash::new();
        let mut writer = OtaWriter::new(flash, Slot::Ota0, pubkey);

        let image = build_test_esp_binary(100);
        let mut packet = build_wire_packet(&image, &privkey);
        packet.push(0x99); // extra unexpected byte

        // Act
        let res = writer.write_chunk(&packet);

        // Assert
        assert_eq!(res, Err(OtaError::ImageTooLarge));
    }

    #[test]
    fn ota_writer_rejects_invalid_esp_magic() {
        // Arrange
        let (pubkey, privkey) = generate_test_keys();
        let flash = MemTargetFlash::new();
        let mut writer = OtaWriter::new(flash, Slot::Ota0, pubkey);

        let mut image = build_test_esp_binary(500);
        image[0] = 0x00; // corrupt magic byte (expected 0xE9)
        let packet = build_wire_packet(&image, &privkey);

        // Act
        let res = writer.write_chunk(&packet);

        // Assert
        assert_eq!(res, Err(OtaError::InvalidImageFormat));
    }

    #[test]
    fn ota_writer_rejects_invalid_chip_id() {
        // Arrange
        let (pubkey, privkey) = generate_test_keys();
        let flash = MemTargetFlash::new();
        let mut writer = OtaWriter::new(flash, Slot::Ota0, pubkey);

        let mut image = build_test_esp_binary(500);
        image[12] = 0x05; // ESP32-C3 chip ID instead of 0x000D
        image[13] = 0x00;
        let packet = build_wire_packet(&image, &privkey);

        // Act
        let res = writer.write_chunk(&packet);

        // Assert
        assert_eq!(res, Err(OtaError::InvalidImageFormat));
    }

    #[test]
    fn ota_writer_allows_disabling_esp_magic_check() {
        // Arrange
        let (pubkey, privkey) = generate_test_keys();
        let flash = MemTargetFlash::new();
        let mut writer = OtaWriter::new(flash, Slot::Ota0, pubkey).with_esp_magic_check(false);

        let raw_payload = vec![0x12; 4096]; // Not an ESP image
        let packet = build_wire_packet(&raw_payload, &privkey);

        // Act
        writer.write_chunk(&packet).unwrap();
        let finish_res = writer.finish();

        // Assert
        assert!(finish_res.is_ok());
    }

    #[test]
    fn ota_writer_rejects_write_after_finish() {
        // Arrange
        let (pubkey, privkey) = generate_test_keys();
        let flash = MemTargetFlash::new();
        let mut writer = OtaWriter::new(flash, Slot::Ota0, pubkey);

        let image = build_test_esp_binary(100);
        let packet = build_wire_packet(&image, &privkey);
        writer.write_chunk(&packet).unwrap();
        writer.finish().unwrap();

        // Act
        let res = writer.write_chunk(&[0xaa]);

        // Assert
        assert_eq!(res, Err(OtaError::AlreadyFinished));
    }

    #[test]
    fn ota_writer_handles_flash_erase_failure() {
        // Arrange
        let (pubkey, privkey) = generate_test_keys();
        let mut flash = MemTargetFlash::new();
        flash.fail_erase_offset = Some(Slot::Ota0.flash_offset());
        let mut writer = OtaWriter::new(flash, Slot::Ota0, pubkey);

        let image = build_test_esp_binary(5000);
        let packet = build_wire_packet(&image, &privkey);

        // Act: Write until first sector flushes (4096 bytes)
        let res = writer.write_chunk(&packet);

        // Assert
        assert_eq!(res, Err(OtaError::FlashErase));
    }

    #[test]
    fn ota_writer_handles_flash_write_failure() {
        // Arrange
        let (pubkey, privkey) = generate_test_keys();
        let mut flash = MemTargetFlash::new();
        flash.fail_write_offset = Some(Slot::Ota0.flash_offset());
        let mut writer = OtaWriter::new(flash, Slot::Ota0, pubkey);

        let image = build_test_esp_binary(5000);
        let packet = build_wire_packet(&image, &privkey);

        // Act: Write until first sector flushes (4096 bytes)
        let res = writer.write_chunk(&packet);

        // Assert
        assert_eq!(res, Err(OtaError::FlashWrite));
    }
}
