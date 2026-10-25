use edf_core::domain::{classify_reset_hold, ResetScope};
use edf_core::events::{SystemState, SystemStateSender, SystemStateWatch};
use edf_gpio::config::{ACTIVITY_PULSE_MS, ECOSYSTEM_RESET_HOLD_MS, FACTORY_RESET_HOLD_MS};
use edf_gpio::{EspFlash, EspFlashError, GpioBootButtonConfig, GpioLedDriver};
use embedded_storage_async::nor_flash::{NorFlash, ReadNorFlash};

#[test]
fn led_activity_pulse_and_lifecycle_state() {
    let driver = GpioLedDriver::new();
    let watch = SystemStateWatch::new();
    let sender = SystemStateSender::new(&watch);
    let receiver = watch.receiver().unwrap();

    futures::executor::block_on(async {
        let steps = async {
            sender.send(SystemState::Operational);
            embassy_time::Timer::after_millis(50).await;
            assert_eq!(driver.observed_state(), Some(SystemState::Operational));
            assert!(driver.is_on());
            driver.activity_pulse();
            embassy_time::Timer::after_millis(20).await;
            assert!(!driver.is_on());
            embassy_time::Timer::after_millis(ACTIVITY_PULSE_MS + 30).await;
            assert!(driver.is_on());
        };
        match embassy_futures::select::select(driver.run(receiver), steps).await {
            embassy_futures::select::Either::First(_) => unreachable!(),
            embassy_futures::select::Either::Second(()) => {}
        }
    });
}

#[test]
fn button_thresholds_are_default_and_configurable() {
    let defaults = GpioBootButtonConfig::new();
    assert_eq!(defaults.ecosystem_hold_ms, 5000);
    assert_eq!(defaults.factory_hold_ms, 10000);
    assert_eq!(ECOSYSTEM_RESET_HOLD_MS, 5000);
    assert_eq!(FACTORY_RESET_HOLD_MS, 10000);
    assert!(defaults.pin.is_high());

    let custom = GpioBootButtonConfig::new().with_hold_durations(3000, 6000);
    assert_eq!(custom.ecosystem_hold_ms, 3000);
    assert_eq!(custom.factory_hold_ms, 6000);
    assert_eq!(
        classify_reset_hold(
            embassy_time::Duration::from_millis(3500),
            true,
            embassy_time::Duration::from_millis(custom.ecosystem_hold_ms),
            embassy_time::Duration::from_millis(custom.factory_hold_ms)
        ),
        Some(ResetScope::Ecosystem)
    );
    assert_eq!(
        classify_reset_hold(
            embassy_time::Duration::from_millis(6000),
            false,
            embassy_time::Duration::from_millis(custom.ecosystem_hold_ms),
            embassy_time::Duration::from_millis(custom.factory_hold_ms)
        ),
        Some(ResetScope::Factory)
    );
}

#[test]
fn flash_partition_checks_alignment_bounds_and_nor_bits() {
    let mut flash = EspFlash::new(0x10000..0x12000);
    assert_eq!(flash.capacity(), 0x2000);
    futures::executor::block_on(async {
        flash.erase(0x10000, 0x11000).await.unwrap();
        flash
            .write(0x10000, &[0xAA, 0xBB, 0xCC, 0xDD])
            .await
            .unwrap();
        flash
            .write(0x10000, &[0x0F, 0xFF, 0xFF, 0xFF])
            .await
            .unwrap();
        let mut bytes = [0; 4];
        flash.read(0x10000, &mut bytes).await.unwrap();
        assert_eq!(bytes, [0x0A, 0xBB, 0xCC, 0xDD]);
        assert_eq!(
            flash.write(0x10001, &[0; 4]).await,
            Err(EspFlashError::NotAligned)
        );
        assert_eq!(
            flash.erase(0x10000, 0x10100).await,
            Err(EspFlashError::NotAligned)
        );
        assert_eq!(
            flash.read(0x12000, &mut bytes).await,
            Err(EspFlashError::OutOfBounds)
        );
        assert_eq!(
            flash.write(0x12000, &[0; 4]).await,
            Err(EspFlashError::OutOfBounds)
        );
        assert_eq!(
            flash.erase(0x10000, 0x13000).await,
            Err(EspFlashError::OutOfBounds)
        );
    });
}

#[test]
fn led_generic_blink_drives_finite_pattern() {
    // Arrange
    let driver = GpioLedDriver::new();
    let watch = SystemStateWatch::new();
    let receiver = watch.receiver().unwrap();

    // Act & Assert
    futures::executor::block_on(async {
        let steps = async {
            driver.blink(100, 100, Some(1));
            embassy_time::Timer::after_millis(20).await;
            assert!(driver.is_on());

            embassy_time::Timer::after_millis(100).await;
            assert!(!driver.is_on());

            embassy_time::Timer::after_millis(120).await;
            assert!(driver.is_on());

            embassy_time::Timer::after_millis(100).await;
            assert!(driver.is_on()); // finite: steady ON; continuous would be OFF (300–400 ms)
        };

        match embassy_futures::select::select(driver.run(receiver), steps).await {
            embassy_futures::select::Either::First(_) => unreachable!(),
            embassy_futures::select::Either::Second(()) => {}
        }
    });
}
