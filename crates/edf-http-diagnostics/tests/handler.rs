use core::cell::Cell;
use edf_core::domain::diagnostics::SystemDiagnostics;
use edf_core::domain::ecosystem::EcosystemId;
use edf_core::domain::reset::ResetScope;
use edf_core::events::SystemState;
use edf_core::ports::SystemResetDriver;
use edf_http_diagnostics::{
    complete_deferred_action, handle_diagnostic_route, DeferredAction, DiagnosticResponse,
    DiagnosticRoute,
};
use embassy_time::Duration;
use futures::executor::block_on;
use std::cell::RefCell;

fn snapshot() -> SystemDiagnostics<()> {
    SystemDiagnostics {
        device_type: "test",
        state: SystemState::Operational,
        domain: (),
        uptime_sec: 0,
        wifi_ssid: None,
        wifi_rssi_dbm: None,
        ip: None,
        mac: [0; 6],
        active_slot: "ota_0",
        slot_valid: true,
        free_heap_bytes: 0,
        total_heap_bytes: 0,
        available_ecosystems: &[] as &[EcosystemId],
        active_ecosystem: None,
        version: "v",
        git_commit: "g",
        build_date: "d",
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
    erased: Cell<Option<ResetScope>>,
    rebooted: Cell<bool>,
    transport: &'a TransportState,
    expected_body: &'static [u8],
}
impl ResetFake<'_> {
    fn assert_response_flushed(&self) {
        assert!(self.transport.written.get());
        assert!(self.transport.flushed.get());
        assert_eq!(&*self.transport.bytes.borrow(), self.expected_body);
    }
}
impl SystemResetDriver for ResetFake<'_> {
    fn erase_credentials(&self, scope: ResetScope) {
        self.assert_response_flushed();
        self.erased.set(Some(scope));
    }
    fn reboot(&self) -> ! {
        self.assert_response_flushed();
        self.rebooted.set(true);
        panic!("reboot");
    }
}

fn write_flush_then_finish(
    response: &DiagnosticResponse<'_>,
    writer: &FakeResponseWriter<'_>,
    reset: &ResetFake<'_>,
) -> Result<(), ()> {
    writer.write_all(response.body)?;
    writer.flush()?;
    block_on(complete_deferred_action(
        response.deferred_action.unwrap(),
        reset,
        None,
        Duration::from_millis(0),
    ))
}

#[test]
fn reset_action_is_deferred_until_caller_explicitly_finishes_after_flush() {
    let transport = TransportState::default();
    let reset = ResetFake {
        erased: Cell::new(None),
        rebooted: Cell::new(false),
        transport: &transport,
        expected_body: b"{\"status\":\"ok\"}",
    };
    let mut scratch = [0; 16];
    let response = handle_diagnostic_route(
        DiagnosticRoute::Reset(ResetScope::Factory),
        &snapshot(),
        b"",
        |_| 0,
        &mut scratch,
    )
    .unwrap();
    assert_eq!(response.body, b"{\"status\":\"ok\"}");
    assert_eq!(
        response.deferred_action,
        Some(DeferredAction::Reset(ResetScope::Factory))
    );
    assert_eq!(reset.erased.get(), None);
    let writer = FakeResponseWriter {
        state: &transport,
        fail_write: false,
        fail_flush: false,
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        write_flush_then_finish(&response, &writer, &reset)
    }));
    assert!(result.is_err());
    assert_eq!(reset.erased.get(), Some(ResetScope::Factory));
    assert!(reset.rebooted.get());
}

#[test]
fn write_or_flush_failure_prevents_credentials_erasure_and_reboot() {
    for (fail_write, fail_flush) in [(true, false), (false, true)] {
        let transport = TransportState::default();
        let reset = ResetFake {
            erased: Cell::new(None),
            rebooted: Cell::new(false),
            transport: &transport,
            expected_body: b"{\"status\":\"ok\"}",
        };
        let writer = FakeResponseWriter {
            state: &transport,
            fail_write,
            fail_flush,
        };
        let mut scratch = [0; 16];
        let response = handle_diagnostic_route(
            DiagnosticRoute::Reset(ResetScope::Ecosystem),
            &snapshot(),
            b"",
            |_| 0,
            &mut scratch,
        )
        .unwrap();
        assert!(write_flush_then_finish(&response, &writer, &reset).is_err());
        assert_eq!(reset.erased.get(), None);
        assert!(!reset.rebooted.get());
        assert!(!transport.flushed.get());
    }
}
