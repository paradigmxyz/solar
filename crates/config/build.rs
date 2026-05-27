#[cfg(feature = "version")]
use std::{
    env,
    path::Path,
    process::{Command, Stdio},
};

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

    let solc_version = solc_version()?;
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
fn solc_version() -> Result<String, Box<dyn std::error::Error>> {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR")?;
    let solc_dir = Path::new(&manifest_dir).join("../../testdata/solidity");

    println!("cargo:rerun-if-changed={}", solc_dir.join(".git").display());

    if let Ok(tag) = git(&solc_dir, ["describe", "--tags", "--exact-match", "HEAD"]) {
        return Ok(normalize_solc_tag(&tag));
    }

    Ok(SOLC_VERSION_FALLBACK.to_string())
}

#[cfg(feature = "version")]
fn git<const N: usize>(dir: &Path, args: [&str; N]) -> Result<String, Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .args(["-C", &dir.display().to_string()])
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()?;
    if !output.status.success() {
        return Err(format!("git command failed in `{}`", dir.display()).into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

#[cfg(feature = "version")]
fn normalize_solc_tag(tag: &str) -> String {
    tag.trim_start_matches('v').to_string()
}
