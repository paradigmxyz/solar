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
///
/// With `SOLC_WRAPPER=1`, the solc-compatible version is reported instead of
/// the native one, and `SOLC_WRAPPER_VERSION` can override the emulated solc
/// version number. Tools like forge resolve compiler versions against source
/// pragmas, so driving a project that pins an exact solc version requires
/// emulating exactly that version.
#[cfg(feature = "version")]
pub fn version() -> &'static str {
    if solc_wrapper() { solc_version_override().unwrap_or(SOLC_VERSION) } else { VERSION }
}

#[cfg(feature = "version")]
fn solc_wrapper() -> bool {
    std::env::var_os("SOLC_WRAPPER").is_some_and(|x| x == "1")
}

/// Rewrites the version number in [`SOLC_VERSION`] to `SOLC_WRAPPER_VERSION`
/// when set, keeping the `+commit....` suffix.
#[cfg(feature = "version")]
fn solc_version_override() -> Option<&'static str> {
    static OVERRIDE: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    OVERRIDE
        .get_or_init(|| {
            let version = std::env::var("SOLC_WRAPPER_VERSION").ok()?;
            let (prefix, rest) = SOLC_VERSION.split_once("Version: ")?;
            let suffix = rest.find('+').map(|i| &rest[i..]).unwrap_or_default();
            Some(format!("{prefix}Version: {version}{suffix}"))
        })
        .as_deref()
}
