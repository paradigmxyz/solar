//! Solar version information.

/// The short version information.
#[cfg(feature = "version")]
pub const SHORT_VERSION: &str = env!("SHORT_VERSION");

/// The long version information.
#[cfg(feature = "version")]
pub const VERSION: &str = concat!(
    env!("LONG_VERSION0"),
    "\n",
    env!("LONG_VERSION1"),
    "\n",
    env!("LONG_VERSION2"),
    "\n",
    env!("LONG_VERSION3"),
    "\n",
    env!("LONG_VERSION4"),
);

/// The solc-compatible long version information.
#[cfg(feature = "version")]
pub const SOLC_VERSION: &str =
    concat!(env!("SOLC_LONG_VERSION0"), "\n", env!("SOLC_LONG_VERSION1"));

/// The semver version information.
pub const SEMVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Returns the short version selected for the current environment.
#[cfg(feature = "version")]
pub fn short_version() -> &'static str {
    SHORT_VERSION
}

/// Returns the long version selected for the current environment.
#[cfg(feature = "version")]
pub fn version() -> &'static str {
    if solc_wrapper() { SOLC_VERSION } else { VERSION }
}

#[cfg(feature = "version")]
fn solc_wrapper() -> bool {
    std::env::var_os("SOLC_WRAPPER").is_some_and(|x| x == "1")
}
