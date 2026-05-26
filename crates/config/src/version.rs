//! Solar version information.

/// The short version information.
#[cfg(feature = "version")]
pub const SHORT_VERSION: &str = env!("SHORT_VERSION");

/// The long version information.
#[cfg(feature = "version")]
pub const LONG_VERSION: &str = concat!(env!("LONG_VERSION0"), "\n", env!("LONG_VERSION1"));

/// The solc-compatible version information.
#[cfg(feature = "version")]
pub const SOLC_VERSION: &str = env!("SOLC_VERSION");

/// The solc-compatible long version information.
#[cfg(feature = "version")]
pub const SOLC_LONG_VERSION: &str =
    concat!(env!("SOLC_LONG_VERSION0"), "\n", env!("SOLC_LONG_VERSION1"));

/// The semver version information.
pub const SEMVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Returns the long version selected for the current environment.
#[cfg(feature = "version")]
pub fn long_version() -> &'static str {
    if matches!(std::env::var("FOUNDRY").as_deref(), Ok("1")) {
        SOLC_LONG_VERSION
    } else {
        LONG_VERSION
    }
}
