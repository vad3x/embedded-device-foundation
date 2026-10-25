use ed25519_dalek::{Signer, SigningKey};
use edf_core::ports::ota::{OtaDriverError, OtaFlashDriver, OtaStream};
use edf_ota::{
    FlashOtaDriver, MemOtaFlash, MemTargetFlash, OtaBootloader, OtaState, Slot, ESP32_C6_CHIP_ID,
    ESP_IMAGE_MAGIC, MAX_OTA_PAYLOAD_SIZE, OTA_HEADER_LEN, OTA_HEADER_MAGIC,
};
use std::cell::Cell;
use std::rc::Rc;

// ============================================================================
// Integration Tests
// ============================================================================

#[test]
fn successful_flash_stream_workflow() {
    // Arrange
    let (pubkey, privkey) = generate_test_keys();
    let image = build_test_esp_binary(8192);
    let wire_payload = build_signed_wire_payload(&image, &privkey);
    let content_len = wire_payload.len();
    let mut stream = MockStream::new(wire_payload);

    let bootloader = OtaBootloader::new(MemOtaFlash::blank()).unwrap();
    let target_flash = MemTargetFlash::new();
    let mut driver = FlashOtaDriver::with_flash(bootloader, target_flash, pubkey);

    // Act
    let outcome = futures::executor::block_on(driver.handle_flash_stream(&mut stream, content_len));

    // Assert
    assert_eq!(outcome, Ok(()));

    // Bootloader should transition to target slot (Slot::Ota1) with sequence 2 and OtaState::New
    let active = driver.bootloader().get_active();
    assert_eq!(active.slot, Slot::Ota1);
    assert_eq!(active.seq, 2);
    assert_eq!(active.state, OtaState::New);

    // Flashed bytes in Slot::Ota1 must match binary image
    let flashed = driver
        .target_flash()
        .read_bytes(Slot::Ota1.flash_offset(), 8192);
    assert_eq!(flashed, image);
}

#[test]
fn successful_flash_stream_with_default_constructor() {
    // Arrange
    let (pubkey, privkey) = generate_test_keys();
    let image = build_test_esp_binary(4096);
    let wire_payload = build_signed_wire_payload(&image, &privkey);
    let content_len = wire_payload.len();
    let mut stream = MockStream::new(wire_payload);
    let mut driver = FlashOtaDriver::new(pubkey);

    // Act
    let outcome = futures::executor::block_on(driver.handle_flash_stream(&mut stream, content_len));

    // Assert
    assert_eq!(outcome, Ok(()));
}

#[test]
fn handles_chunked_streaming() {
    // Arrange: small 64-byte chunks simulate slow/fragmented TCP network packets
    let (pubkey, privkey) = generate_test_keys();
    let image = build_test_esp_binary(5000);
    let wire_payload = build_signed_wire_payload(&image, &privkey);
    let content_len = wire_payload.len();
    let mut stream = MockStream::new(wire_payload).with_chunk_size(64);

    let bootloader = OtaBootloader::new(MemOtaFlash::blank()).unwrap();
    let target_flash = MemTargetFlash::new();
    let mut driver = FlashOtaDriver::with_flash(bootloader, target_flash, pubkey);

    // Act
    let outcome = futures::executor::block_on(driver.handle_flash_stream(&mut stream, content_len));

    // Assert
    assert_eq!(outcome, Ok(()));
    let flashed = driver
        .target_flash()
        .read_bytes(Slot::Ota1.flash_offset(), 5000);
    assert_eq!(flashed, image);
}

#[test]
fn rejects_invalid_wire_magic() {
    // Arrange
    let (pubkey, privkey) = generate_test_keys();
    let image = build_test_esp_binary(4096);
    let mut wire_payload = build_signed_wire_payload(&image, &privkey);
    wire_payload[0..4].copy_from_slice(b"BAD!");
    let content_len = wire_payload.len();
    let mut stream = MockStream::new(wire_payload);
    let mut driver = FlashOtaDriver::new(pubkey);

    // Act
    let outcome = futures::executor::block_on(driver.handle_flash_stream(&mut stream, content_len));

    // Assert
    assert_eq!(outcome, Err(OtaDriverError::InvalidHeader));
}

#[test]
fn rejects_content_length_smaller_than_header() {
    // Arrange
    let (pubkey, _) = generate_test_keys();
    let mut stream = MockStream::new(vec![0u8; 70]);
    let mut driver = FlashOtaDriver::new(pubkey);

    // Act
    let outcome = futures::executor::block_on(driver.handle_flash_stream(&mut stream, 70));

    // Assert
    assert_eq!(outcome, Err(OtaDriverError::InvalidHeader));
}

#[test]
fn rejects_content_length_exceeding_max_slot() {
    // Arrange
    let (pubkey, _) = generate_test_keys();
    let too_large = MAX_OTA_PAYLOAD_SIZE + 1;
    let mut stream = MockStream::new(vec![]);
    let mut driver = FlashOtaDriver::new(pubkey);

    // Act
    let outcome = futures::executor::block_on(driver.handle_flash_stream(&mut stream, too_large));

    // Assert
    assert_eq!(outcome, Err(OtaDriverError::InvalidHeader));
}

#[test]
fn rejects_tampered_signature_and_protects_rollback() {
    // Arrange
    let (pubkey, privkey) = generate_test_keys();
    let image = build_test_esp_binary(4096);
    let mut wire_payload = build_signed_wire_payload(&image, &privkey);
    // Tamper with signature at byte 8
    wire_payload[8] ^= 0xff;
    let content_len = wire_payload.len();
    let mut stream = MockStream::new(wire_payload);

    let bootloader = OtaBootloader::new(MemOtaFlash::blank()).unwrap();
    let target_flash = MemTargetFlash::new();
    let mut driver = FlashOtaDriver::with_flash(bootloader, target_flash, pubkey);

    // Act
    let outcome = futures::executor::block_on(driver.handle_flash_stream(&mut stream, content_len));

    // Assert
    assert_eq!(outcome, Err(OtaDriverError::VerificationFailed));

    // Rollback protection: bootloader state must remain untouched at Slot::Ota0, seq 0
    let active = driver.bootloader().get_active();
    assert_eq!(active.slot, Slot::Ota0);
    assert_eq!(active.seq, 0);
}

#[test]
fn verification_failure_before_commit_allows_retry_in_inactive_slot() {
    let (pubkey, private_key) = generate_test_keys();
    let image = build_test_esp_binary(4096);
    let valid_payload = build_signed_wire_payload(&image, &private_key);
    let mut invalid_payload = valid_payload.clone();
    invalid_payload[8] ^= 0xff;
    let bootloader = OtaBootloader::new(MemOtaFlash::blank()).unwrap();
    let mut driver = FlashOtaDriver::with_flash(bootloader, MemTargetFlash::new(), pubkey);

    let mut first = MockStream::new(invalid_payload);
    assert_eq!(
        futures::executor::block_on(driver.handle_flash_stream(&mut first, valid_payload.len())),
        Err(OtaDriverError::VerificationFailed)
    );
    let mut retry = MockStream::new(valid_payload.clone());
    assert_eq!(
        futures::executor::block_on(driver.handle_flash_stream(&mut retry, valid_payload.len())),
        Ok(())
    );
    assert_eq!(driver.bootloader().get_active().slot, Slot::Ota1);
    assert_eq!(
        driver
            .target_flash()
            .read_bytes(Slot::Ota1.flash_offset(), image.len()),
        image
    );
}

#[test]
fn truncated_stream_before_commit_allows_retry_in_inactive_slot() {
    let (pubkey, private_key) = generate_test_keys();
    let image = build_test_esp_binary(4096);
    let payload = build_signed_wire_payload(&image, &private_key);
    let bootloader = OtaBootloader::new(MemOtaFlash::blank()).unwrap();
    let mut driver = FlashOtaDriver::with_flash(bootloader, MemTargetFlash::new(), pubkey);

    let mut truncated = MockStream::new(payload[..1000].to_vec());
    assert_eq!(
        futures::executor::block_on(driver.handle_flash_stream(&mut truncated, payload.len())),
        Err(OtaDriverError::TruncatedStream)
    );
    let mut retry = MockStream::new(payload.clone());
    assert_eq!(
        futures::executor::block_on(driver.handle_flash_stream(&mut retry, payload.len())),
        Ok(())
    );
    assert_eq!(driver.bootloader().get_active().slot, Slot::Ota1);
}

struct PartialCommitFlash {
    storage: MemOtaFlash,
    writes: Rc<Cell<usize>>,
}

impl edf_ota::OtaFlashStorage for PartialCommitFlash {
    type Error = ();

    fn read_otadata(
        &mut self,
        sector0: &mut [u8; edf_ota::OTA_DATA_ENTRY_SIZE],
        sector1: &mut [u8; edf_ota::OTA_DATA_ENTRY_SIZE],
    ) -> Result<(), Self::Error> {
        sector0.copy_from_slice(&self.storage.sector0);
        sector1.copy_from_slice(&self.storage.sector1);
        Ok(())
    }

    fn write_otadata_sector(
        &mut self,
        sector_index: usize,
        data: &[u8; edf_ota::OTA_DATA_ENTRY_SIZE],
    ) -> Result<(), Self::Error> {
        if sector_index == 0 {
            self.storage.sector0.copy_from_slice(data);
        } else {
            self.storage.sector1.copy_from_slice(data);
        }
        self.writes.set(self.writes.get() + 1);
        Err(())
    }
}

#[test]
fn partial_boot_metadata_commit_failure_requires_reboot_before_retry() {
    let (pubkey, private_key) = generate_test_keys();
    let image = build_test_esp_binary(4096);
    let payload = build_signed_wire_payload(&image, &private_key);
    let writes = Rc::new(Cell::new(0));
    let bootloader = OtaBootloader::new(PartialCommitFlash {
        storage: MemOtaFlash::blank(),
        writes: Rc::clone(&writes),
    })
    .unwrap();
    let mut driver = FlashOtaDriver::with_flash(bootloader, MemTargetFlash::new(), pubkey);

    let mut first = MockStream::new(payload.clone());
    assert_eq!(
        futures::executor::block_on(driver.handle_flash_stream(&mut first, payload.len())),
        Err(OtaDriverError::BootloaderCommitFailed)
    );
    assert_eq!(writes.get(), 1);
    let erased_before_retry = driver.target_flash().erased_sectors.clone();
    let mut retry = MockStream::new(payload.clone());
    assert_eq!(
        futures::executor::block_on(driver.handle_flash_stream(&mut retry, payload.len())),
        Err(OtaDriverError::RebootRequired)
    );
    assert_eq!(retry.read_pos, 0);
    assert_eq!(driver.target_flash().erased_sectors, erased_before_retry);
    assert_eq!(writes.get(), 1);
}

#[test]
fn rejects_invalid_esp_image_magic() {
    // Arrange
    let (pubkey, privkey) = generate_test_keys();
    let mut image = build_test_esp_binary(4096);
    image[0] = 0x00; // Invalid magic (not 0xE9)
    let wire_payload = build_signed_wire_payload(&image, &privkey);
    let content_len = wire_payload.len();
    let mut stream = MockStream::new(wire_payload);
    let mut driver = FlashOtaDriver::new(pubkey);

    // Act
    let outcome = futures::executor::block_on(driver.handle_flash_stream(&mut stream, content_len));

    // Assert
    assert_eq!(outcome, Err(OtaDriverError::InvalidHeader));
}

#[test]
fn rejects_truncated_stream_and_preserves_bootloader() {
    // Arrange
    let (pubkey, privkey) = generate_test_keys();
    let image = build_test_esp_binary(4096);
    let wire_payload = build_signed_wire_payload(&image, &privkey);
    let content_len = wire_payload.len();
    // Feed only 1000 bytes out of content_len
    let truncated_data = wire_payload[..1000].to_vec();
    let mut stream = MockStream::new(truncated_data);

    let bootloader = OtaBootloader::new(MemOtaFlash::blank()).unwrap();
    let target_flash = MemTargetFlash::new();
    let mut driver = FlashOtaDriver::with_flash(bootloader, target_flash, pubkey);

    // Act
    let outcome = futures::executor::block_on(driver.handle_flash_stream(&mut stream, content_len));

    // Assert
    assert_eq!(outcome, Err(OtaDriverError::TruncatedStream));

    // Rollback check: Bootloader state must not be committed to Slot::Ota1
    let active = driver.bootloader().get_active();
    assert_eq!(active.slot, Slot::Ota0);
    assert_eq!(active.seq, 0);
}

#[test]
fn rejects_flash_erase_failure() {
    // Arrange
    let (pubkey, privkey) = generate_test_keys();
    let image = build_test_esp_binary(4096);
    let wire_payload = build_signed_wire_payload(&image, &privkey);
    let content_len = wire_payload.len();
    let mut stream = MockStream::new(wire_payload);

    let bootloader = OtaBootloader::new(MemOtaFlash::blank()).unwrap();
    let mut target_flash = MemTargetFlash::new();
    target_flash.fail_erase_offset = Some(Slot::Ota1.flash_offset());
    let mut driver = FlashOtaDriver::with_flash(bootloader, target_flash, pubkey);

    // Act
    let outcome = futures::executor::block_on(driver.handle_flash_stream(&mut stream, content_len));

    // Assert
    assert_eq!(outcome, Err(OtaDriverError::FlashWriteFailed));
}

#[test]
fn rejects_flash_write_failure() {
    // Arrange
    let (pubkey, privkey) = generate_test_keys();
    let image = build_test_esp_binary(4096);
    let wire_payload = build_signed_wire_payload(&image, &privkey);
    let content_len = wire_payload.len();
    let mut stream = MockStream::new(wire_payload);

    let bootloader = OtaBootloader::new(MemOtaFlash::blank()).unwrap();
    let mut target_flash = MemTargetFlash::new();
    target_flash.fail_write_offset = Some(Slot::Ota1.flash_offset());
    let mut driver = FlashOtaDriver::with_flash(bootloader, target_flash, pubkey);

    // Act
    let outcome = futures::executor::block_on(driver.handle_flash_stream(&mut stream, content_len));

    // Assert
    assert_eq!(outcome, Err(OtaDriverError::FlashWriteFailed));
}

#[test]
fn bootloader_mark_valid_lifecycle_transition() {
    // Arrange: Complete an OTA update to Slot::Ota1 (seq 2, state: New)
    let (pubkey, privkey) = generate_test_keys();
    let image = build_test_esp_binary(4096);
    let wire_payload = build_signed_wire_payload(&image, &privkey);
    let content_len = wire_payload.len();
    let mut stream = MockStream::new(wire_payload);

    let bootloader = OtaBootloader::new(MemOtaFlash::blank()).unwrap();
    let target_flash = MemTargetFlash::new();
    let mut driver = FlashOtaDriver::with_flash(bootloader, target_flash, pubkey);
    futures::executor::block_on(driver.handle_flash_stream(&mut stream, content_len)).unwrap();

    // Act: Mark the running slot valid
    let mark_result = driver.bootloader_mut().mark_valid();

    // Assert
    assert_eq!(mark_result, Ok(()));
    let active = driver.bootloader().get_active();
    assert_eq!(active.slot, Slot::Ota1);
    assert_eq!(active.seq, 2);
    assert_eq!(active.state, OtaState::Valid);
}

// ============================================================================
// Test Helpers
// ============================================================================

struct MockStream {
    incoming: Vec<u8>,
    read_pos: usize,
    chunk_size: usize,
    outgoing: Vec<u8>,
}

impl MockStream {
    fn new(data: Vec<u8>) -> Self {
        Self {
            incoming: data,
            read_pos: 0,
            chunk_size: 4096,
            outgoing: Vec::new(),
        }
    }

    fn with_chunk_size(mut self, size: usize) -> Self {
        self.chunk_size = size;
        self
    }
}

impl OtaStream for MockStream {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, ()> {
        if self.read_pos >= self.incoming.len() {
            return Ok(0);
        }
        let available = self.incoming.len() - self.read_pos;
        let to_read = core::cmp::min(buf.len(), core::cmp::min(self.chunk_size, available));
        buf[..to_read].copy_from_slice(&self.incoming[self.read_pos..self.read_pos + to_read]);
        self.read_pos += to_read;
        Ok(to_read)
    }

    async fn write_all(&mut self, buf: &[u8]) -> Result<(), ()> {
        self.outgoing.extend_from_slice(buf);
        Ok(())
    }

    async fn flush(&mut self) -> Result<(), ()> {
        Ok(())
    }
}

fn generate_test_keys() -> ([u8; 32], SigningKey) {
    let mut seed = [0u8; 32];
    for (i, b) in seed.iter_mut().enumerate() {
        *b = (i as u8).wrapping_mul(7).wrapping_add(13);
    }
    let signing_key = SigningKey::from_bytes(&seed);
    let verifying_key = signing_key.verifying_key();
    (verifying_key.to_bytes(), signing_key)
}

fn build_test_esp_binary(len: usize) -> Vec<u8> {
    let mut bin = vec![0xbb; len];
    bin[0] = ESP_IMAGE_MAGIC;
    if len >= 14 {
        bin[12] = (ESP32_C6_CHIP_ID & 0xff) as u8;
        bin[13] = ((ESP32_C6_CHIP_ID >> 8) & 0xff) as u8;
    }
    bin
}

fn build_signed_wire_payload(image: &[u8], signing_key: &SigningKey) -> Vec<u8> {
    let sig = signing_key.sign(image);
    let mut packet = Vec::with_capacity(OTA_HEADER_LEN + image.len());
    packet.extend_from_slice(OTA_HEADER_MAGIC);
    packet.extend_from_slice(&(image.len() as u32).to_le_bytes());
    packet.extend_from_slice(&sig.to_bytes());
    packet.extend_from_slice(image);
    packet
}
