/// The build profile name.
pub const BUILD_PROFILE_NAME: &str = {
    // https://stackoverflow.com/questions/73595435/how-to-get-profile-from-cargo-toml-in-build-rs-or-at-runtime
    const OUT_DIR: &str = env!("OUT_DIR");
    let unix_parts = const_format::str_split!(OUT_DIR, '/');
    if unix_parts.len() >= 4 {
        unix_parts[unix_parts.len() - 4]
    } else {
        let win_parts = const_format::str_split!(OUT_DIR, '\\');
        win_parts[win_parts.len() - 4]
    }
};

/// The latest version from Cargo.toml.
pub const CARGO_PKG_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The full SHA of the latest commit.
pub const VERGEN_GIT_SHA_LONG: &str = env!("VERGEN_GIT_SHA");

/// The 8 character short SHA of the latest commit.
pub const VERGEN_GIT_SHA: &str = const_format::str_index!(VERGEN_GIT_SHA_LONG, ..8);

/// The build timestamp.
pub const VERGEN_BUILD_TIMESTAMP: &str = env!("VERGEN_BUILD_TIMESTAMP");

/// The target triple.
pub const VERGEN_CARGO_TARGET_TRIPLE: &str = env!("VERGEN_CARGO_TARGET_TRIPLE");

/// The build features.
pub const VERGEN_CARGO_FEATURES: &str = env!("VERGEN_CARGO_FEATURES");

/// The short version information.
pub const SHORT_VERSION: &str = const_format::concatcp!(
    env!("CARGO_PKG_VERSION"),
    env!("VERSION_SUFFIX"),
    " (",
    VERGEN_GIT_SHA,
    " ",
    VERGEN_BUILD_TIMESTAMP,
    ")",
);

/// The long version information.
pub const LONG_VERSION: &str = const_format::concatcp!(
    "Version: ",
    env!("CARGO_PKG_VERSION"),
    "\n",
    "Commit SHA: ",
    VERGEN_GIT_SHA_LONG,
    "\n",
    "Build Timestamp: ",
    env!("VERGEN_BUILD_TIMESTAMP"),
    "\n",
    "Build Features: ",
    env!("VERGEN_CARGO_FEATURES"),
    "\n",
    "Build Profile: ",
    BUILD_PROFILE_NAME,
);
