//! Target filters for verbose log records.

/// Existing noisy target prefixes suppressed at Debug and Trace level.
pub const DEFAULT_SUPPRESSED_DEBUG_PREFIXES: &[&str] =
    &["rs_matter::transport::network::mdns", "trouble_host"];

/// Existing exact-target suppressions at Debug and Trace level.
pub const DEFAULT_SUPPRESSED_DEBUG_EXACT: &[&str] = &[];

pub(crate) fn enabled(metadata: &log::Metadata<'_>, exact: &[&str], prefixes: &[&str]) -> bool {
    if metadata.level() <= log::Level::Info {
        return true;
    }
    let target = metadata.target();
    !exact.contains(&target) && !prefixes.iter().any(|prefix| target.starts_with(prefix))
}
