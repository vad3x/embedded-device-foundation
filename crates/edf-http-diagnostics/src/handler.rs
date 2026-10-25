use edf_core::domain::diagnostics::SystemDiagnostics;
use edf_core::domain::ecosystem::EcosystemType;
use edf_core::domain::reset::ResetScope;
use edf_core::events::SystemStateSender;
use edf_core::ports::SystemResetDriver;
use embassy_time::{Duration, Timer};

use crate::json::{format_status_json, DiagnosticPayload};
use crate::route::DiagnosticRoute;

const OK_BODY: &[u8] = b"{\"status\":\"ok\"}";

/// Default HTTP action delay, matching the source servers' post-response policy.
pub const DEFAULT_ACTION_DELAY: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeferredAction {
    Reboot,
    Reset(ResetScope),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticError {
    BufferTooSmall,
    InvalidLogLength,
}

/// Response metadata and body. The caller owns HTTP framing, writing, and flushing.
pub struct DiagnosticResponse<'a> {
    pub status: u16,
    pub body: &'a [u8],
    pub content_type: &'static str,
    pub content_encoding: Option<&'static str>,
    pub deferred_action: Option<DeferredAction>,
}

/// Build a response from already parsed route and supplied assets/telemetry.
pub fn handle_diagnostic_route<'a, D, T, L>(
    route: DiagnosticRoute,
    snapshot: &SystemDiagnostics<D, T>,
    dashboard_gzip: &'a [u8],
    logs: L,
    scratch: &'a mut [u8],
) -> Result<DiagnosticResponse<'a>, DiagnosticError>
where
    D: DiagnosticPayload,
    T: EcosystemType,
    L: FnOnce(&mut [u8]) -> usize,
{
    let (body, content_type, content_encoding, deferred_action) = match route {
        DiagnosticRoute::Dashboard => (
            dashboard_gzip,
            "text/html; charset=utf-8",
            Some("gzip"),
            None,
        ),
        DiagnosticRoute::Status => {
            let len = format_status_json(snapshot, scratch)
                .map_err(|_| DiagnosticError::BufferTooSmall)?;
            (&scratch[..len], "application/json", None, None)
        }
        DiagnosticRoute::Logs => {
            let len = logs(scratch);
            let body = scratch
                .get(..len)
                .ok_or(DiagnosticError::InvalidLogLength)?;
            (body, "text/plain; charset=utf-8", None, None)
        }
        DiagnosticRoute::Reboot => (
            OK_BODY,
            "application/json",
            None,
            Some(DeferredAction::Reboot),
        ),
        DiagnosticRoute::Reset(scope) => (
            OK_BODY,
            "application/json",
            None,
            Some(DeferredAction::Reset(scope)),
        ),
    };
    Ok(DiagnosticResponse {
        status: 200,
        body,
        content_type,
        content_encoding,
        deferred_action,
    })
}

/// Invoke only after the response has been successfully written and flushed.
pub async fn complete_deferred_action<R: SystemResetDriver>(
    action: DeferredAction,
    reset: &R,
    state_sender: Option<SystemStateSender<'_>>,
    delay: Duration,
) -> ! {
    match action {
        DeferredAction::Reboot => {}
        DeferredAction::Reset(ResetScope::Ecosystem) => {
            if let Some(sender) = state_sender {
                sender.ecosystem_reset();
            }
        }
        DeferredAction::Reset(ResetScope::Factory) => {
            if let Some(sender) = state_sender {
                sender.factory_reset();
            }
        }
    }
    if delay.as_micros() > 0 {
        Timer::after(delay).await;
    }
    if let DeferredAction::Reset(scope) = action {
        reset.erase_credentials(scope);
    }
    reset.reboot()
}

#[cfg(test)]
mod tests {
    use super::*;
    use edf_core::domain::ecosystem::EcosystemId;
    use edf_core::events::SystemState;

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

    #[test]
    fn dashboard_logs_and_status_return_borrowed_bodies_and_metadata() {
        let mut scratch = [0; 1024];
        let response = handle_diagnostic_route(
            DiagnosticRoute::Dashboard,
            &snapshot(),
            b"gzip",
            |_| 0,
            &mut scratch,
        )
        .unwrap();
        assert_eq!(response.body, b"gzip");
        assert_eq!(response.content_encoding, Some("gzip"));
        let response = handle_diagnostic_route(
            DiagnosticRoute::Logs,
            &snapshot(),
            b"gzip",
            |out| {
                out[..3].copy_from_slice(b"log");
                3
            },
            &mut scratch,
        )
        .unwrap();
        assert_eq!(response.body, b"log");
        assert_eq!(response.content_type, "text/plain; charset=utf-8");
        let response = handle_diagnostic_route(
            DiagnosticRoute::Status,
            &snapshot(),
            b"gzip",
            |_| 0,
            &mut scratch,
        )
        .unwrap();
        assert_eq!(response.content_type, "application/json");
        assert!(core::str::from_utf8(response.body)
            .unwrap()
            .contains("\"device_type\":\"test\""));
    }

    #[test]
    fn oversized_log_callback_result_returns_error_without_slicing() {
        let mut scratch = [0; 4];
        let result =
            handle_diagnostic_route(DiagnosticRoute::Logs, &snapshot(), b"", |_| 5, &mut scratch);
        assert!(matches!(result, Err(DiagnosticError::InvalidLogLength)));
    }
}
