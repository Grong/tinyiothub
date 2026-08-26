//! Version information embedded at compile time.

/// Crate version from Cargo.toml.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Crate name.
pub const NAME: &str = env!("CARGO_PKG_NAME");

/// Full version string: `tinyiothub-core/1.1.0`.
pub fn version_string() -> String {
    format!("{}/{}", NAME, VERSION)
}

/// Check if two versions are API-compatible (same major version).
pub fn is_compatible(major: u16) -> bool {
    // Parse our version
    let our_major = VERSION
        .split('.')
        .next()
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);
    our_major == major
}
