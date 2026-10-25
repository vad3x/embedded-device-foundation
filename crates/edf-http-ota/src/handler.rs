use edf_core::events::SystemStateSender;
use edf_core::ports::{OtaFlashDriver, OtaStream, SystemResetDriver};
use embassy_time::{Duration, Timer};

/// OTA header plus maximum 0x350000-byte application slot.
pub const MAX_CONTENT_LENGTH: usize = 72 + 0x350000;
pub const MIN_CONTENT_LENGTH: usize = 72;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OtaPolicy {
    pub max_content_length: usize,
    pub failure_delay: Duration,
    pub reboot_delay: Duration,
}
impl Default for OtaPolicy {
    fn default() -> Self {
        Self {
            max_content_length: MAX_CONTENT_LENGTH,
            failure_delay: Duration::from_secs(5),
            reboot_delay: Duration::from_millis(500),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OtaResponse {
    pub status: u16,
    pub body: &'static [u8],
    pub content_type: &'static str,
    pub reboot_required: bool,
}
impl OtaResponse {
    const fn ok() -> Self {
        Self {
            status: 200,
            body: b"{\"status\":\"ok\"}",
            content_type: "application/json",
            reboot_required: true,
        }
    }
    const fn bad_request() -> Self {
        Self {
            status: 400,
            body: b"{\"error\":\"bad_request\"}",
            content_type: "application/json",
            reboot_required: false,
        }
    }
}

/// Validate a parsed body length, flash using the supplied byte stream, and return response data.
pub async fn handle_ota_flash<S: OtaStream, D: OtaFlashDriver>(
    content_length: Option<usize>,
    stream: &mut S,
    driver: &mut D,
    state_sender: Option<SystemStateSender<'_>>,
    policy: OtaPolicy,
) -> OtaResponse {
    let max_content_length = policy.max_content_length.min(MAX_CONTENT_LENGTH);
    let Some(length) =
        content_length.filter(|len| *len >= MIN_CONTENT_LENGTH && *len <= max_content_length)
    else {
        wait_failure(policy.failure_delay).await;
        return OtaResponse::bad_request();
    };
    if let Some(sender) = state_sender {
        sender.updating();
    }
    match driver.handle_flash_stream(stream, length).await {
        Ok(()) => OtaResponse::ok(),
        Err(_) => {
            if let Some(sender) = state_sender {
                sender.operational();
            }
            wait_failure(policy.failure_delay).await;
            OtaResponse::bad_request()
        }
    }
}

async fn wait_failure(delay: Duration) {
    if delay.as_micros() > 0 {
        Timer::after(delay).await;
    }
}

/// Invoke only after the successful OTA response has been written and flushed.
pub async fn complete_ota_response<R: SystemResetDriver>(
    response: &OtaResponse,
    reset: &R,
    policy: OtaPolicy,
) {
    if response.reboot_required {
        if policy.reboot_delay.as_micros() > 0 {
            Timer::after(policy.reboot_delay).await;
        }
        reset.reboot();
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use edf_core::events::{SystemState, SystemStateWatch};
    use edf_core::ports::OtaDriverError;
    use futures::executor::block_on;
    use std::vec::Vec;

    struct Stream {
        bytes: &'static [u8],
        pos: usize,
    }
    impl OtaStream for Stream {
        async fn read(&mut self, out: &mut [u8]) -> Result<usize, ()> {
            let count = out.len().min(self.bytes.len() - self.pos);
            out[..count].copy_from_slice(&self.bytes[self.pos..self.pos + count]);
            self.pos += count;
            Ok(count)
        }
        async fn write_all(&mut self, _buf: &[u8]) -> Result<(), ()> {
            Ok(())
        }
        async fn flush(&mut self) -> Result<(), ()> {
            Ok(())
        }
    }
    struct Driver {
        calls: usize,
        observed: Vec<u8>,
        fail: bool,
    }
    impl OtaFlashDriver for Driver {
        async fn handle_flash_stream<S: OtaStream>(
            &mut self,
            stream: &mut S,
            content_length: usize,
        ) -> Result<(), OtaDriverError> {
            self.calls += 1;
            let mut buf = [0; 8];
            while self.observed.len() < content_length {
                let n = stream
                    .read(&mut buf)
                    .await
                    .map_err(|_| OtaDriverError::TruncatedStream)?;
                if n == 0 {
                    return Err(OtaDriverError::TruncatedStream);
                }
                self.observed.extend_from_slice(&buf[..n]);
            }
            if self.fail {
                Err(OtaDriverError::VerificationFailed)
            } else {
                Ok(())
            }
        }
    }
    fn policy() -> OtaPolicy {
        OtaPolicy {
            max_content_length: MAX_CONTENT_LENGTH,
            failure_delay: Duration::from_millis(0),
            reboot_delay: Duration::from_millis(0),
        }
    }

    #[test]
    fn missing_and_oversized_lengths_reject_before_flash() {
        let mut stream = Stream { bytes: &[], pos: 0 };
        let mut driver = Driver {
            calls: 0,
            observed: Vec::new(),
            fail: false,
        };
        let missing = block_on(handle_ota_flash(
            None,
            &mut stream,
            &mut driver,
            None,
            policy(),
        ));
        let oversized = block_on(handle_ota_flash(
            Some(MAX_CONTENT_LENGTH + 1),
            &mut stream,
            &mut driver,
            None,
            policy(),
        ));
        assert_eq!(missing.status, 400);
        assert_eq!(oversized.body, b"{\"error\":\"bad_request\"}");
        assert_eq!(driver.calls, 0);
    }

    #[test]
    fn configurable_limit_cannot_exceed_physical_payload_maximum() {
        let mut stream = Stream { bytes: &[], pos: 0 };
        let mut driver = Driver {
            calls: 0,
            observed: Vec::new(),
            fail: false,
        };
        let mut policy = policy();
        policy.max_content_length = usize::MAX;
        let response = block_on(handle_ota_flash(
            Some(MAX_CONTENT_LENGTH + 1),
            &mut stream,
            &mut driver,
            None,
            policy,
        ));
        assert_eq!(response.status, 400);
        assert_eq!(driver.calls, 0);
    }

    #[test]
    fn success_and_failure_map_to_json_and_reboot_intent() {
        let bytes: &'static [u8] = &[7; 72];
        let mut stream = Stream { bytes, pos: 0 };
        let mut driver = Driver {
            calls: 0,
            observed: Vec::new(),
            fail: false,
        };
        let success = block_on(handle_ota_flash(
            Some(72),
            &mut stream,
            &mut driver,
            None,
            policy(),
        ));
        assert_eq!(success.status, 200);
        assert_eq!(success.body, b"{\"status\":\"ok\"}");
        assert!(success.reboot_required);
        assert_eq!(driver.observed, bytes);
        let mut stream = Stream { bytes, pos: 0 };
        driver.fail = true;
        driver.observed.clear();
        let failure = block_on(handle_ota_flash(
            Some(72),
            &mut stream,
            &mut driver,
            None,
            policy(),
        ));
        assert_eq!(failure.status, 400);
        assert!(!failure.reboot_required);
    }

    #[test]
    fn lifecycle_enters_updating_and_returns_to_operational_on_failure() {
        let watch = SystemStateWatch::new();
        let sender = SystemStateSender::new(&watch);
        sender.operational();
        let mut stream = Stream {
            bytes: &[1; 72],
            pos: 0,
        };
        let mut driver = Driver {
            calls: 0,
            observed: Vec::new(),
            fail: false,
        };
        let result = block_on(handle_ota_flash(
            Some(72),
            &mut stream,
            &mut driver,
            Some(sender),
            policy(),
        ));
        assert_eq!(result.status, 200);
        assert_eq!(sender.current_state(), Some(SystemState::Updating));

        let mut stream = Stream {
            bytes: &[1; 72],
            pos: 0,
        };
        driver.observed.clear();
        driver.fail = true;
        let result = block_on(handle_ota_flash(
            Some(72),
            &mut stream,
            &mut driver,
            Some(sender),
            policy(),
        ));
        assert_eq!(result.status, 400);
        assert_eq!(sender.current_state(), Some(SystemState::Operational));
    }
}
