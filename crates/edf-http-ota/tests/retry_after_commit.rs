use core::cell::{Cell, RefCell};
use core::convert::Infallible;
use ed25519_dalek::{Signer, SigningKey};
use edf_core::domain::reset::ResetScope;
use edf_core::ports::{OtaStream, SystemResetDriver};
use edf_http_ota::{complete_ota_response, handle_ota_flash, OtaPolicy, OtaResponse};
use edf_ota::{
    FlashOtaDriver, MemOtaFlash, MemTargetFlash, OtaBootloader, OtaFlashStorage, Slot,
    ESP32_C6_CHIP_ID, ESP_IMAGE_MAGIC, OTA_DATA_ENTRY_SIZE, OTA_HEADER_MAGIC, SECTOR_SIZE,
};
use embassy_time::Duration;
use futures::executor::block_on;
use std::rc::Rc;

struct Upload {
    bytes: Vec<u8>,
    position: usize,
}

impl OtaStream for Upload {
    async fn read(&mut self, out: &mut [u8]) -> Result<usize, ()> {
        let size = out.len().min(self.bytes.len() - self.position);
        out[..size].copy_from_slice(&self.bytes[self.position..self.position + size]);
        self.position += size;
        Ok(size)
    }

    async fn write_all(&mut self, _: &[u8]) -> Result<(), ()> {
        Ok(())
    }

    async fn flush(&mut self) -> Result<(), ()> {
        Ok(())
    }
}

#[derive(Clone)]
struct SharedOtadata(Rc<RefCell<(MemOtaFlash, usize)>>);

impl OtaFlashStorage for SharedOtadata {
    type Error = Infallible;

    fn read_otadata(
        &mut self,
        sector0: &mut [u8; OTA_DATA_ENTRY_SIZE],
        sector1: &mut [u8; OTA_DATA_ENTRY_SIZE],
    ) -> Result<(), Self::Error> {
        let state = self.0.borrow();
        sector0.copy_from_slice(&state.0.sector0);
        sector1.copy_from_slice(&state.0.sector1);
        Ok(())
    }

    fn write_otadata_sector(
        &mut self,
        sector_index: usize,
        data: &[u8; OTA_DATA_ENTRY_SIZE],
    ) -> Result<(), Self::Error> {
        let mut state = self.0.borrow_mut();
        match sector_index {
            0 => state.0.sector0.copy_from_slice(data),
            1 => state.0.sector1.copy_from_slice(data),
            _ => unreachable!(),
        }
        state.1 += 1;
        Ok(())
    }
}

struct FailedFlush<'a> {
    written: &'a Cell<bool>,
}

impl FailedFlush<'_> {
    fn write_all(&self, _: &[u8]) -> Result<(), ()> {
        self.written.set(true);
        Ok(())
    }

    fn flush(&self) -> Result<(), ()> {
        Err(())
    }
}

struct Reset<'a>(&'a Cell<bool>);

impl SystemResetDriver for Reset<'_> {
    fn erase_credentials(&self, _: ResetScope) {
        unreachable!()
    }

    fn reboot(&self) -> ! {
        self.0.set(true);
        panic!("unexpected reboot")
    }
}

fn send_response(
    response: &OtaResponse,
    writer: &FailedFlush<'_>,
    reset: &Reset<'_>,
) -> Result<(), ()> {
    writer.write_all(response.body)?;
    writer.flush()?;
    block_on(complete_ota_response(response, reset, policy()));
    Ok(())
}

fn policy() -> OtaPolicy {
    OtaPolicy {
        max_content_length: edf_http_ota::MAX_CONTENT_LENGTH,
        failure_delay: Duration::from_millis(0),
        reboot_delay: Duration::from_millis(0),
    }
}

#[test]
fn retry_after_commit_and_failed_response_flush_preserves_running_and_fallback_image() {
    let signing_key = SigningKey::from_bytes(&[7; 32]);
    let mut image = vec![0xa5; SECTOR_SIZE];
    image[0] = ESP_IMAGE_MAGIC;
    image[12..14].copy_from_slice(&ESP32_C6_CHIP_ID.to_le_bytes());
    let signature = signing_key.sign(&image);
    let mut bytes = Vec::from(OTA_HEADER_MAGIC.as_slice());
    bytes.extend_from_slice(&(image.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&signature.to_bytes());
    bytes.extend_from_slice(&image);

    let otadata = SharedOtadata(Rc::new(RefCell::new((MemOtaFlash::blank(), 0))));
    let bootloader = OtaBootloader::new(otadata.clone()).unwrap();
    let mut target = MemTargetFlash::new();
    target
        .sectors
        .insert(Slot::Ota0.flash_offset(), [0x5a; SECTOR_SIZE]);
    let mut driver =
        FlashOtaDriver::with_flash(bootloader, target, signing_key.verifying_key().to_bytes());

    let mut first = Upload {
        bytes: bytes.clone(),
        position: 0,
    };
    let first_response = block_on(handle_ota_flash(
        Some(bytes.len()),
        &mut first,
        &mut driver,
        None,
        policy(),
    ));
    assert_eq!(first_response.status, 200);
    assert!(first_response.reboot_required);

    let response_written = Cell::new(false);
    let rebooted = Cell::new(false);
    let result = send_response(
        &first_response,
        &FailedFlush {
            written: &response_written,
        },
        &Reset(&rebooted),
    );
    assert_eq!(result, Err(()));
    assert!(response_written.get());
    assert!(!rebooted.get());

    let before_metadata = otadata.0.borrow().clone();
    let before_erases = driver.target_flash().erased_sectors.clone();
    let before_sectors = driver.target_flash().sectors.clone();
    let mut retry = Upload { bytes, position: 0 };
    let retry_response = block_on(handle_ota_flash(
        Some(retry.bytes.len()),
        &mut retry,
        &mut driver,
        None,
        policy(),
    ));

    assert_eq!(retry_response.status, 400);
    assert!(!retry_response.reboot_required);
    assert_eq!(retry.position, 0, "rejected before reading retry stream");
    assert_eq!(driver.target_flash().erased_sectors, before_erases);
    assert_eq!(driver.target_flash().sectors, before_sectors);
    let after_metadata = otadata.0.borrow();
    assert_eq!(after_metadata.0.sector0, before_metadata.0.sector0);
    assert_eq!(after_metadata.0.sector1, before_metadata.0.sector1);
    assert_eq!(after_metadata.1, before_metadata.1);
    assert_eq!(
        driver
            .target_flash()
            .read_bytes(Slot::Ota0.flash_offset(), SECTOR_SIZE),
        vec![0x5a; SECTOR_SIZE]
    );
}
