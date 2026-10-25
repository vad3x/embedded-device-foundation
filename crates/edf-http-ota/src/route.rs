#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OtaRoute {
    Flash,
}

/// Parses an already separated HTTP method and URI path.
pub fn parse_ota_route(method: &str, path: &str) -> Option<OtaRoute> {
    let path = path.split('?').next().unwrap_or(path);
    match (method, path) {
        ("POST", "/api/v1/ota/flash") => Some(OtaRoute::Flash),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::{parse_ota_route, OtaRoute};

    #[test]
    fn flash_route_is_exact_and_query_suffix_is_ignored() {
        assert_eq!(
            parse_ota_route("POST", "/api/v1/ota/flash?nonce=1"),
            Some(OtaRoute::Flash)
        );
        assert_eq!(parse_ota_route("GET", "/api/v1/ota/flash"), None);
        assert_eq!(parse_ota_route("post", "/api/v1/ota/flash"), None);
        assert_eq!(parse_ota_route("POST", "/api/v1/ota/flash/"), None);
    }
}
