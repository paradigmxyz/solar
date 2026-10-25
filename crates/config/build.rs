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
    let git = smolgen::Gitcl::builder()
        .describe(false, true, None)
        .dirty(true)
        .sha(false)
        .vcs_info_fallback(true)
        .build();
    let mut emitter = smolgen::Emitter::new();
    emitter
        .default_on_error()
        .add_instructions(&cargo)?
        .add_instructions(&build)?
        .add_instructions(&git)?;

    let mut instructions = Vec::new();
    emitter.emit_to(&mut instructions)?;
    // Tracking the git HEAD reruns this script, and rebuilds every crate above this one, on every
    // commit and checkout. Debug builds skip it and keep the SHA from the last run of this script.
    // Release profiles keep tracking HEAD, so their version matches the checkout.
    let skip_head_tracking = env::var("PROFILE").is_ok_and(|profile| profile == "debug");
    for line in String::from_utf8(instructions)?.lines() {
        let is_git_path = line.starts_with("cargo:rerun-if-changed=")
            && line != "cargo:rerun-if-changed=build.rs";
        if !(skip_head_tracking && is_git_path) {
            println!("{line}");
        }
    }

    let vars = emitter.cargo_rustc_env_map();
    let vergen_env = |key: smolgen::VergenKey| match vars.get(&key) {
        Some(value) if value != VERGEN_IDEMPOTENT_OUTPUT => value.as_str(),
        _ => VERSION_UNKNOWN,
    };

    let sha = vergen_env(smolgen::VergenKey::GitSha);
    let sha_short = sha.get(..7).unwrap_or(&sha);

    let is_dev = {
        let is_dirty = vergen_env(smolgen::VergenKey::GitDirty) == "true";

        // > git describe --always --tags
        // if not on a tag: v0.2.0-beta.3-82-g1939939b
        // if on a tag: v0.2.0-beta.3
        let not_on_tag =
            vergen_env(smolgen::VergenKey::GitDescribe).ends_with(&format!("-g{sha_short}"));

        is_dirty || not_on_tag
    };
    let version_suffix = if is_dev { "-dev" } else { "" };

    let version = env::var("CARGO_PKG_VERSION").unwrap();
    let version_suffixed = format!("{version}{version_suffix}");

    let timestamp = vergen_env(smolgen::VergenKey::BuildTimestamp);

    let short_version = format!("{version_suffixed} ({sha_short} {timestamp})");
    println!("cargo:rustc-env=SHORT_VERSION={short_version}");

    // Use the out dir to determine the profile being used
    let out_dir = env::var("OUT_DIR").unwrap();
    let profile = out_dir.rsplit(std::path::MAIN_SEPARATOR).nth(3).unwrap();

    let mut cargo_features = vergen_env(smolgen::VergenKey::CargoFeatures).to_string();
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
