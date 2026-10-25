use core::cell::Cell;
use edf_core::domain::reset::ResetScope;
use edf_core::ports::{OtaDriverError, OtaFlashDriver, OtaStream, SystemResetDriver};
use edf_http_ota::{
    complete_ota_response, handle_ota_flash, OtaPolicy, OtaResponse, MAX_CONTENT_LENGTH,
};
use embassy_time::Duration;
use futures::executor::block_on;
use std::cell::RefCell;

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

#[derive(Default)]
struct TransportState {
    written: Cell<bool>,
    flushed: Cell<bool>,
    bytes: RefCell<Vec<u8>>,
}

struct FakeResponseWriter<'a> {
    state: &'a TransportState,
    fail_write: bool,
    fail_flush: bool,
}
impl FakeResponseWriter<'_> {
    fn write_all(&self, body: &[u8]) -> Result<(), ()> {
        if self.fail_write {
            return Err(());
        }
        self.state.bytes.borrow_mut().extend_from_slice(body);
        self.state.written.set(true);
        Ok(())
    }
    fn flush(&self) -> Result<(), ()> {
        if self.fail_flush {
            return Err(());
        }
        assert!(self.state.written.get());
        self.state.flushed.set(true);
        Ok(())
    }
}

struct ResetFake<'a> {
    rebooted: Cell<bool>,
    transport: &'a TransportState,
    expected_body: &'static [u8],
}
impl SystemResetDriver for ResetFake<'_> {
    fn erase_credentials(&self, _: ResetScope) {
        panic!("OTA must not erase credentials")
    }
    fn reboot(&self) -> ! {
        assert!(self.transport.written.get());
        assert!(self.transport.flushed.get());
        assert_eq!(&*self.transport.bytes.borrow(), self.expected_body);
        self.rebooted.set(true);
        panic!("reboot")
    }
}

fn write_flush_then_finish(
    response: &OtaResponse,
    writer: &FakeResponseWriter<'_>,
    reset: &ResetFake<'_>,
) -> Result<(), ()> {
    writer.write_all(response.body)?;
    writer.flush()?;
    block_on(complete_ota_response(response, reset, policy()));
    Ok(())
}
#[test]
fn reboot_occurs_only_when_caller_explicitly_finishes_after_flush() {
    let mut stream = Stream {
        bytes: &[1; 72],
        pos: 0,
    };
    let mut driver = Driver {
        calls: 0,
        observed: Vec::new(),
        fail: false,
    };
    let response = block_on(handle_ota_flash(
        Some(72),
        &mut stream,
        &mut driver,
        None,
        policy(),
    ));
    let transport = TransportState::default();
    let reset = ResetFake {
        rebooted: Cell::new(false),
        transport: &transport,
        expected_body: b"{\"status\":\"ok\"}",
    };
    assert!(!reset.rebooted.get());
    let writer = FakeResponseWriter {
        state: &transport,
        fail_write: false,
        fail_flush: false,
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        write_flush_then_finish(&response, &writer, &reset)
    }));
    assert!(result.is_err());
    assert!(reset.rebooted.get());
}

#[test]
fn write_or_flush_failure_prevents_ota_reboot() {
    for (fail_write, fail_flush) in [(true, false), (false, true)] {
        let mut stream = Stream {
            bytes: &[1; 72],
            pos: 0,
        };
        let mut driver = Driver {
            calls: 0,
            observed: Vec::new(),
            fail: false,
        };
        let response = block_on(handle_ota_flash(
            Some(72),
            &mut stream,
            &mut driver,
            None,
            policy(),
        ));
        assert!(response.reboot_required);
        let transport = TransportState::default();
        let reset = ResetFake {
            rebooted: Cell::new(false),
            transport: &transport,
            expected_body: b"{\"status\":\"ok\"}",
        };
        let writer = FakeResponseWriter {
            state: &transport,
            fail_write,
            fail_flush,
        };
        assert!(write_flush_then_finish(&response, &writer, &reset).is_err());
        assert!(!reset.rebooted.get());
        assert!(!transport.flushed.get());
    }
}
