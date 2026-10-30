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
        .describe(true, false, None)
        // Untracked files do not change the build, so they must not make a release build `-dev`.
        .dirty(false)
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

    let version = env::var("CARGO_PKG_VERSION").unwrap();

    // `git describe --tags` prints the tag only on a tagged commit. Builds without Git metadata,
    // such as from a published crate, report unknown and come from a release.
    let describe = vergen_env("VERGEN_GIT_DESCRIBE");
    let is_release_checkout = describe == VERSION_UNKNOWN || describe == format!("v{version}");
    let is_dev = vergen_env("VERGEN_GIT_DIRTY") == "true" || !is_release_checkout;
    let version_suffix = if is_dev { "-dev" } else { "" };

    let version_suffixed = format!("{version}{version_suffix}");
    println!("cargo:rustc-env=SOLAR_VERSION={version_suffixed}");

    let solar_long_version = format!("{version_suffixed}+commit.{sha_short}");
    println!("cargo:rustc-env=SHORT_VERSION={solar_long_version}");

    let solc_compat_version = format!("{solc_version}+commit.{sha_short}.solar.{version_suffixed}");
    println!("cargo:rustc-env=SOLC_COMPAT_VERSION={solc_compat_version}");

    let timestamp = env::var("VERGEN_BUILD_TIMESTAMP").unwrap();

    // The profile is the directory above `build`: `target/[<triple>/]<profile>/build/...`.
    let out_dir = env::var("OUT_DIR").unwrap();
    let profile = out_dir
        .rsplit(std::path::MAIN_SEPARATOR)
        .skip_while(|&c| c != "build")
        .nth(1)
        .unwrap_or(VERSION_UNKNOWN);

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
