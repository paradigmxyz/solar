use std::env;

fn main() {
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

    let long_version = format!(
        "Version: {version}\nCommit SHA: {sha}\nBuild Timestamp: {timestamp}\nBuild Features: {}\nBuild Profile: {}",
        env::var("VERGEN_CARGO_FEATURES").unwrap(),
        env::var("PROFILE").unwrap(),
    );
    println!("cargo:rustc-env=LONG_VERSION={long_version}");
}
