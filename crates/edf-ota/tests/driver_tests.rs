use ed25519_dalek::{Signer, SigningKey};
use edf_core::ports::ota::{OtaDriverError, OtaFlashDriver, OtaStream};
use edf_ota::{
    FlashOtaDriver, MemOtaFlash, MemTargetFlash, OtaBootloader, OtaState, Slot, ESP32_C6_CHIP_ID,
    ESP_IMAGE_MAGIC, MAX_OTA_PAYLOAD_SIZE, OTA_HEADER_LEN, OTA_HEADER_MAGIC,
};

// ============================================================================
// Unit Tests
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
fn rejects_tampered_signature() {
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

    // Bootloader state must remain untouched at Slot::Ota0, seq 0
    let active = driver.bootloader().get_active();
    assert_eq!(active.slot, Slot::Ota0);
    assert_eq!(active.seq, 0);
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
fn rejects_truncated_stream() {
    // Arrange
    let (pubkey, privkey) = generate_test_keys();
    let image = build_test_esp_binary(4096);
    let wire_payload = build_signed_wire_payload(&image, &privkey);
    let content_len = wire_payload.len();
    // Feed only 1000 bytes out of content_len
    let truncated_data = wire_payload[..1000].to_vec();
    let mut stream = MockStream::new(truncated_data);
    let mut driver = FlashOtaDriver::new(pubkey);

    // Act
    let outcome = futures::executor::block_on(driver.handle_flash_stream(&mut stream, content_len));

    // Assert
    assert_eq!(outcome, Err(OtaDriverError::TruncatedStream));
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
