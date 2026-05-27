#[cfg(feature = "version")]
use std::{env, io::Read, path::Path};

#[cfg(feature = "version")]
const SOLC_VERSION_FALLBACK: &str = "0.8.35";

#[cfg(feature = "version")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cargo = vergen::Cargo::builder().features(true).target_triple(true).build();
    let build = vergen::Build::builder().build_timestamp(true).build();
    let git = vergen::Gitcl::builder().describe(false, true, None).dirty(true).sha(false).build();
    vergen::Emitter::new()
        .add_instructions(&cargo)?
        .add_instructions(&build)?
        .add_instructions(&git)?
        .emit_and_set()?;

    let sha = env::var("VERGEN_GIT_SHA").unwrap();
    let sha_short = &sha[..7];

    let is_dev = {
        let is_dirty = env::var("VERGEN_GIT_DIRTY").unwrap() == "true";

        // > git describe --always --tags
        // if not on a tag: v0.2.0-beta.3-82-g1939939b
        // if on a tag: v0.2.0-beta.3
        let not_on_tag =
            env::var("VERGEN_GIT_DESCRIBE").unwrap().ends_with(&format!("-g{sha_short}"));

        is_dirty || not_on_tag
    };
    let version_suffix = if is_dev { "-dev" } else { "" };

    let version = env::var("CARGO_PKG_VERSION").unwrap();
    let version_suffixed = format!("{version}{version_suffix}");

    let timestamp = env::var("VERGEN_BUILD_TIMESTAMP").unwrap();

    let short_version = format!("{version_suffixed} ({sha_short} {timestamp})");
    println!("cargo:rustc-env=SHORT_VERSION={short_version}");

    // Use the out dir to determine the profile being used
    let out_dir = env::var("OUT_DIR").unwrap();
    let profile = out_dir.rsplit(std::path::MAIN_SEPARATOR).nth(3).unwrap();

    let mut cargo_features = env::var("VERGEN_CARGO_FEATURES").unwrap();
    let ignore = ["clap", "version", "serde"];
    for feature in ignore {
        cargo_features = cargo_features
            .replace(&format!(",{feature}"), "")
            .replace(&format!("{feature},"), "")
            .replace(feature, "");
    }
    let long_version = format!(
        "Version: {version}\n\
         Commit SHA: {sha}\n\
         Build Timestamp: {timestamp}\n\
         Build Features: {cargo_features}\n\
         Build Profile: {profile}",
    );
    assert_eq!(long_version.lines().count(), 5); // `version.rs` must be updated as well.
    for (i, line) in long_version.lines().enumerate() {
        println!("cargo:rustc-env=LONG_VERSION{i}={line}");
    }

    let solc_version = solc_version();
    let solc_compat_version = format!("{solc_version}+commit.{sha_short}.solar.{version}");

    let solc_long_version = format!(
        "the Solidity compiler\n\
         Version: {solc_compat_version}",
    );
    assert_eq!(solc_long_version.lines().count(), 2); // `version.rs` must be updated as well.
    for (i, line) in solc_long_version.lines().enumerate() {
        println!("cargo:rustc-env=SOLC_LONG_VERSION{i}={line}");
    }

    Ok(())
}

#[cfg(not(feature = "version"))]
fn main() {}

#[cfg(feature = "version")]
fn solc_version() -> String {
    let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") else {
        return SOLC_VERSION_FALLBACK.to_string();
    };
    let solc_dir = Path::new(&manifest_dir).join("../../testdata/solidity");

    println!("cargo:rerun-if-changed={}", solc_dir.join("CMakeLists.txt").display());

    solc_version_from_cmake(&solc_dir).unwrap_or_else(|_| SOLC_VERSION_FALLBACK.to_string())
}

#[cfg(feature = "version")]
fn solc_version_from_cmake(dir: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let mut cmake = String::new();
    std::fs::File::open(dir.join("CMakeLists.txt"))?.read_to_string(&mut cmake)?;
    for line in cmake.lines() {
        let line = line.trim();
        if let Some(version) = line.strip_prefix("set(PROJECT_VERSION \"")
            && let Some(version) = version.strip_suffix("\")")
        {
            return Ok(version.to_string());
        }
    }
    Err("failed to determine solc version from CMakeLists.txt".into())
}
