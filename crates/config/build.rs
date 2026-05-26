#[cfg(feature = "version")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::env;

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

    let solc_compat_version = format!("0.8.30+commit.{sha_short}.solar.{version}");
    println!("cargo:rustc-env=SOLC_VERSION={solc_compat_version}");

    let long_version = format!(
        "the Solar compiler\n\
         Version: {short_version}",
    );
    assert_eq!(long_version.lines().count(), 2); // `version.rs` must be updated as well.
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

#[cfg(not(feature = "version"))]
fn main() {}
