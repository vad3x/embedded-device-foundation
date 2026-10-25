use edf_core::domain::reset::ResetScope;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticRoute {
    Dashboard,
    Status,
    Logs,
    Reboot,
    Reset(ResetScope),
}

/// Parses an already separated HTTP method and URI path.
pub fn parse_diagnostic_route(method: &str, path: &str) -> Option<DiagnosticRoute> {
    let path = path.split('?').next().unwrap_or(path);
    match (method, path) {
        ("GET", "/") => Some(DiagnosticRoute::Dashboard),
        ("GET", "/api/status") => Some(DiagnosticRoute::Status),
        ("GET", "/api/logs") => Some(DiagnosticRoute::Logs),
        ("POST", "/api/action/reboot") => Some(DiagnosticRoute::Reboot),
        ("POST", "/api/action/reset/matter") => Some(DiagnosticRoute::Reset(ResetScope::Ecosystem)),
        ("POST", "/api/action/reset/factory") => Some(DiagnosticRoute::Reset(ResetScope::Factory)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::{parse_diagnostic_route, DiagnosticRoute};
    use edf_core::domain::reset::ResetScope;

    #[test]
    fn common_routes_match_exact_method_and_path_with_query_ignored() {
        assert_eq!(
            parse_diagnostic_route("GET", "/"),
            Some(DiagnosticRoute::Dashboard)
        );
        assert_eq!(
            parse_diagnostic_route("GET", "/api/status?now=1"),
            Some(DiagnosticRoute::Status)
        );
        assert_eq!(
            parse_diagnostic_route("GET", "/api/logs"),
            Some(DiagnosticRoute::Logs)
        );
        assert_eq!(
            parse_diagnostic_route("POST", "/api/action/reboot"),
            Some(DiagnosticRoute::Reboot)
        );
        assert_eq!(
            parse_diagnostic_route("POST", "/api/action/reset/matter"),
            Some(DiagnosticRoute::Reset(ResetScope::Ecosystem))
        );
        assert_eq!(
            parse_diagnostic_route("POST", "/api/action/reset/factory"),
            Some(DiagnosticRoute::Reset(ResetScope::Factory))
        );
        assert_eq!(parse_diagnostic_route("get", "/"), None);
        assert_eq!(parse_diagnostic_route("POST", "/api/status"), None);
        assert_eq!(parse_diagnostic_route("GET", "/api/Status"), None);
        assert_eq!(parse_diagnostic_route("GET", "/api/status/"), None);
    }
}
