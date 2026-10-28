//! Solar version information.

use std::sync::LazyLock;

/// The short version information: [`SOLAR_VERSION`] with the commit as build metadata, such as
/// `0.3.0+commit.d9f23fbe`.
#[cfg(feature = "version")]
pub const SHORT_VERSION: &str = env!("SHORT_VERSION");

/// Solar's version, with a `-dev` suffix unless built from a clean release tag checkout.
#[cfg(feature = "version")]
pub const SOLAR_VERSION: &str = env!("SOLAR_VERSION");

/// The solc-compatible version: `0.8.38+commit.d9f23fbe.solar.0.3.0`.
///
/// Its semver core is the emulated solc version, so tools that gate on solc versions keep working,
/// and its build metadata names the Solar build. Metadata and verifiers use this form.
#[cfg(feature = "version")]
pub const SOLC_COMPAT_VERSION: &str = env!("SOLC_COMPAT_VERSION");

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
    "\n",
    env!("LONG_VERSION5"),
    "\n",
    env!("LONG_VERSION6"),
);

/// The solc-compatible long version information.
#[cfg(feature = "version")]
pub const SOLC_VERSION: &str =
    concat!(env!("SOLC_LONG_VERSION0"), "\n", env!("SOLC_LONG_VERSION1"));

/// The semver version information.
pub const SEMVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The solc version number that Solar implements.
pub const SOLC_SEMVER_VERSION: &str = env!("SOLC_SEMVER_VERSION");

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

/// Returns [`SOLAR_VERSION`], or the release version under [`TEST_VERSION_ENV`].
#[cfg(feature = "version")]
pub fn solar_version() -> &'static str {
    if test_version() { SEMVER_VERSION } else { SOLAR_VERSION }
}

/// Returns [`SOLC_COMPAT_VERSION`], or a fixed commit and the release version under
/// [`TEST_VERSION_ENV`].
#[cfg(feature = "version")]
pub fn solc_compat_version() -> &'static str {
    if test_version() { TEST_SOLC_COMPAT_VERSION } else { SOLC_COMPAT_VERSION }
}

/// The solc-compatible version that compiler output reports under [`TEST_VERSION_ENV`].
pub const TEST_SOLC_COMPAT_VERSION: &str =
    concat!(env!("SOLC_SEMVER_VERSION"), "+commit.00000000.solar.", env!("CARGO_PKG_VERSION"));

/// Setting this to `1` in debug builds replaces the commit and `-dev` suffix in compiler output
/// with fixed values, so that test snapshots of metadata and bytecode do not change with every
/// commit.
pub const TEST_VERSION_ENV: &str = "SOLAR_TEST_VERSION";

#[cfg(feature = "version")]
fn test_version() -> bool {
    cfg!(debug_assertions) && std::env::var_os(TEST_VERSION_ENV).is_some_and(|x| x == "1")
}

/// Returns the solc version that Solar implements, parsed from [`SOLC_SEMVER_VERSION`].
///
/// This ignores `SOLC_WRAPPER_VERSION`, which only changes the version that `--version`
/// reports, not the language semantics.
pub fn solc_semver_version() -> &'static semver::Version {
    static VERSION: LazyLock<semver::Version> =
        LazyLock::new(|| semver::Version::parse(SOLC_SEMVER_VERSION).unwrap());
    &VERSION
}
