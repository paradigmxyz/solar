//! Solar version information.

/// The short version information.
#[cfg(feature = "version")]
pub const SHORT_VERSION: &str = env!("SHORT_VERSION");

/// The long version information.
#[cfg(feature = "version")]
pub const LONG_VERSION: &str = concat!(
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

/// The semver version information.
pub const SEMVER_VERSION: &str = env!("CARGO_PKG_VERSION");
