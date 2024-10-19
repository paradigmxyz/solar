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
    let sha_short = &sha[0..7];

    let is_dirty = env::var("VERGEN_GIT_DIRTY").unwrap() == "true";
    // > git describe --always --tags
    // if not on a tag: v0.2.0-beta.3-82-g1939939b
    // if on a tag: v0.2.0-beta.3
    let not_on_tag = env::var("VERGEN_GIT_DESCRIBE").unwrap().ends_with(&format!("-g{sha_short}"));
    let is_dev = is_dirty || not_on_tag;
    println!("cargo:rustc-env=VERSION_SUFFIX={}", if is_dev { "-dev" } else { "" });
}
