use std::{
    collections::hash_map::DefaultHasher,
    env, fs,
    hash::{Hash, Hasher},
    path::PathBuf,
    process::Command,
};

fn main() {
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=build.rs");
    let rustc = env::var_os("RUSTC").unwrap();
    let version = Command::new(rustc).arg("-vV").output().expect("query Rust toolchain");
    assert!(version.status.success(), "failed to query Rust toolchain");
    let mut inputs = vec![
        version.stdout,
        include_bytes!("src/lib.rs").to_vec(),
        include_bytes!("build.rs").to_vec(),
    ];
    for key in [
        "CARGO_PKG_VERSION",
        "TARGET",
        "PROFILE",
        "OPT_LEVEL",
        "DEBUG",
        "CARGO_CFG_PANIC",
        "CARGO_CFG_TARGET_FEATURE",
        "CARGO_CFG_TARGET_POINTER_WIDTH",
        "CARGO_CFG_TARGET_ENDIAN",
        "CARGO_ENCODED_RUSTFLAGS",
    ] {
        println!("cargo:rerun-if-env-changed={key}");
        inputs.push(env::var(key).unwrap_or_default().into_bytes());
    }
    let flags = env::var("CARGO_ENCODED_RUSTFLAGS").unwrap_or_default();
    assert!(
        !flags.contains("randomize-layout"),
        "Rust plugins do not support randomized type layouts"
    );
    let mut fingerprint = Vec::new();
    for salt in 0_u64..4 {
        let mut hash = DefaultHasher::new();
        salt.hash(&mut hash);
        inputs.hash(&mut hash);
        fingerprint.extend_from_slice(&hash.finish().to_le_bytes());
    }
    let output = format!("const SDK_BUILD_FINGERPRINT: [u8; 32] = {fingerprint:?};\n");
    fs::write(PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("abi.rs"), output).unwrap();
}
