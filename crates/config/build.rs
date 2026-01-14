#[cfg(feature = "version")]
fn main() {
    use std::env;

    vergen::EmitBuilder::builder()
        .git_describe(false, true, None)
        .git_dirty(true)
        .git_sha(false)
        .build_timestamp()
        .cargo_features()
        .cargo_target_triple()
        .emit_and_set()
        .unwrap();

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
    let _profile = out_dir.rsplit(std::path::MAIN_SEPARATOR).nth(3).unwrap();

    let mut cargo_features = env::var("VERGEN_CARGO_FEATURES").unwrap();
    let ignore = ["clap", "version", "serde"];
    for feature in ignore {
        cargo_features = cargo_features
            .replace(&format!(",{feature}"), "")
            .replace(&format!("{feature},"), "")
            .replace(feature, "");
    }
    // Format version to be compatible with solc for tools like Foundry
    // solc format: "solc, the solidity compiler commandline interface\nVersion:
    // 0.8.15+commit.xxx.OS.compiler" We match exactly - Foundry parses the second line for
    // semver
    let solc_compat_version = format!("0.8.28+commit.{sha_short}.solar.{version}");

    // Output exactly 2 lines like solc does
    let long_version = format!(
        "the Solidity compiler\n\
         Version: {solc_compat_version}",
    );
    // We changed from 5 lines to 2 lines, update version.rs accordingly
    for (i, line) in long_version.lines().enumerate() {
        println!("cargo:rustc-env=LONG_VERSION{i}={line}");
    }
    // Pad remaining slots with empty strings
    for i in long_version.lines().count()..5 {
        println!("cargo:rustc-env=LONG_VERSION{i}=");
    }
}

#[cfg(not(feature = "version"))]
fn main() {}
