use std::{env, io::Read, path::Path};

const SOLC_VERSION_FALLBACK: &str = "0.8.38";
#[cfg(feature = "version")]
const VERSION_UNKNOWN: &str = "unknown";
#[cfg(feature = "version")]
const VERGEN_IDEMPOTENT_OUTPUT: &str = "VERGEN_IDEMPOTENT_OUTPUT";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let solc_version = solc_version();
    println!("cargo:rustc-env=SOLC_SEMVER_VERSION={solc_version}");

    #[cfg(feature = "version")]
    emit_version(&solc_version)?;

    Ok(())
}

#[cfg(feature = "version")]
fn emit_version(solc_version: &str) -> Result<(), Box<dyn std::error::Error>> {
    let cargo = smolgen::Cargo::builder().features(true).target_triple(true).build();
    let build = smolgen::Build::builder().build_timestamp(true).build();
    // Tracking the git HEAD reruns this script, and rebuilds every crate above this one, on every
    // commit and checkout. Debug builds skip it and keep the SHA from the last run of this script.
    // Release profiles keep tracking HEAD, so their version matches the checkout.
    let track_head = env::var("PROFILE")? != "debug";
    let git = smolgen::Gitcl::builder()
        .describe(true, true, None)
        .dirty(true)
        .sha(false)
        .vcs_info_fallback(true)
        .rerun_on_head_change(track_head)
        .build();
    smolgen::Emitter::new()
        .default_on_error()
        .add_instructions(&cargo)?
        .add_instructions(&build)?
        .add_instructions(&git)?
        .emit_and_set()?;

    let sha = vergen_env("VERGEN_GIT_SHA");
    // solc uses 8 hex digits, and tools match `commit\.[0-9a-f]{8}`.
    let sha_short = sha.get(..8).unwrap_or(&sha);

    let is_dirty = vergen_env("VERGEN_GIT_DIRTY") == "true";
    let is_dev = is_dirty || !is_release_describe(&vergen_env("VERGEN_GIT_DESCRIBE"));
    let version_suffix = if is_dev { "-dev" } else { "" };

    let version = env::var("CARGO_PKG_VERSION").unwrap();
    let version_suffixed = format!("{version}{version_suffix}");
    println!("cargo:rustc-env=SOLAR_VERSION={version_suffixed}");

    let solar_long_version = format!("{version_suffixed}+commit.{sha_short}");
    println!("cargo:rustc-env=SOLAR_LONG_VERSION={solar_long_version}");

    let solc_compat_version = format!("{solc_version}+commit.{sha_short}.solar.{version_suffixed}");
    println!("cargo:rustc-env=SOLC_COMPAT_VERSION={solc_compat_version}");

    let timestamp = env::var("VERGEN_BUILD_TIMESTAMP").unwrap();

    println!("cargo:rustc-env=SHORT_VERSION={solar_long_version}");

    // The profile is the directory above `build`: `target/[<triple>/]<profile>/build/...`.
    let out_dir = env::var("OUT_DIR").unwrap();
    let components = out_dir.split(std::path::MAIN_SEPARATOR).collect::<Vec<_>>();
    let profile = components
        .iter()
        .rposition(|&c| c == "build")
        .and_then(|i| i.checked_sub(1))
        .map_or(VERSION_UNKNOWN, |i| components[i]);

    let mut cargo_features = env::var("VERGEN_CARGO_FEATURES").unwrap();
    let ignore = ["clap", "version", "serde"];
    for feature in ignore {
        cargo_features = cargo_features
            .replace(&format!(",{feature}"), "")
            .replace(&format!("{feature},"), "")
            .replace(feature, "");
    }
    let long_version = format!(
        "{solar_long_version}\n\
         Solidity Version: {solc_version}\n\
         Solc-Compatible Version: {solc_compat_version}\n\
         Commit SHA: {sha}\n\
         Build Timestamp: {timestamp}\n\
         Build Features: {cargo_features}\n\
         Build Profile: {profile}",
    );
    assert_eq!(long_version.lines().count(), 7); // `version.rs` must be updated as well.
    for (i, line) in long_version.lines().enumerate() {
        println!("cargo:rustc-env=LONG_VERSION{i}={line}");
    }

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

/// Returns whether `git describe --always --tags --dirty` output names a release tag checkout.
///
/// On a tag it prints the tag (`v0.2.0`), after it `v0.2.0-856-gd9f23fbed`, and without reachable
/// tags (shallow clones) only the abbreviated SHA. Builds without Git metadata, such as from a
/// published crate, report unknown and come from a release.
#[cfg(feature = "version")]
fn is_release_describe(describe: &str) -> bool {
    if describe == VERSION_UNKNOWN {
        return true;
    }
    let describe = describe.strip_suffix("-dirty").unwrap_or(describe);
    let mut parts = describe.rsplitn(3, '-');
    let after_tag = matches!(
        (parts.next(), parts.next(), parts.next()),
        (Some(sha), Some(count), Some(_))
            if sha.strip_prefix('g').is_some_and(|sha| sha.bytes().all(|b| b.is_ascii_hexdigit()))
                && count.bytes().all(|b| b.is_ascii_digit())
    );
    describe.starts_with('v') && !after_tag
}

#[cfg(feature = "version")]
fn vergen_env(key: &str) -> String {
    match env::var(key) {
        Ok(value) if value != VERGEN_IDEMPOTENT_OUTPUT => value,
        _ => VERSION_UNKNOWN.to_string(),
    }
}

fn solc_version() -> String {
    let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") else {
        return SOLC_VERSION_FALLBACK.to_string();
    };
    let solc_dir = Path::new(&manifest_dir).join("../../testdata/solidity");

    // A missing path would rerun the build script on every build.
    let cmake = solc_dir.join("CMakeLists.txt");
    let watched = if cmake.exists() { cmake.as_path() } else { Path::new("build.rs") };
    println!("cargo:rerun-if-changed={}", watched.display());

    solc_version_from_cmake(&solc_dir).unwrap_or_else(|_| SOLC_VERSION_FALLBACK.to_string())
}

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
