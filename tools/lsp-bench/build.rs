use sha2::{Digest, Sha256};
use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::Command,
};

const UNAVAILABLE: &str = "unavailable";

fn main() {
    println!("cargo::rerun-if-env-changed=SOLAR_LSP_BENCH_BUILD_REVISION");

    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let workspace = manifest_dir.parent().and_then(Path::parent).unwrap();
    let revision = build_revision(workspace).unwrap_or_else(|| UNAVAILABLE.into());
    let dirty = git_output(workspace, &["status", "--porcelain", "--untracked-files=normal"])
        .map(|status| !status.is_empty())
        .unwrap_or(true);
    let contract = contract_sha256(workspace, &manifest_dir);

    println!("cargo::rustc-env=SOLAR_LSP_BENCH_BUILD_REVISION={revision}");
    println!("cargo::rustc-env=SOLAR_LSP_BENCH_BUILD_DIRTY={dirty}");
    println!("cargo::rustc-env=SOLAR_LSP_BENCH_CONTRACT_SHA256={contract}");
}

fn build_revision(workspace: &Path) -> Option<String> {
    if let Ok(revision) = env::var("SOLAR_LSP_BENCH_BUILD_REVISION") {
        assert!(is_sha(&revision), "SOLAR_LSP_BENCH_BUILD_REVISION must be a full Git SHA");
        return Some(revision);
    }
    git_output(workspace, &["rev-parse", "HEAD"]).filter(|revision| is_sha(revision))
}

fn contract_sha256(workspace: &Path, manifest_dir: &Path) -> String {
    let mut files = vec![
        manifest_dir.join("Cargo.toml"),
        manifest_dir.join("build.rs"),
        workspace.join("Cargo.lock"),
    ];
    collect_rust_files(&manifest_dir.join("src"), &mut files);
    files.sort();

    let mut hasher = Sha256::new();
    for path in files {
        println!("cargo::rerun-if-changed={}", path.display());
        let relative = path.strip_prefix(workspace).unwrap();
        let mut file = fs::File::open(&path).unwrap();
        let length = file.metadata().unwrap().len();
        hasher.update(relative.to_string_lossy().as_bytes());
        hasher.update([0]);
        hasher.update(length.to_le_bytes());
        assert_eq!(io::copy(&mut file, &mut hasher).unwrap(), length);
    }
    format!("{:x}", hasher.finalize())
}

fn collect_rust_files(directory: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if entry.file_type().unwrap().is_dir() {
            collect_rust_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

fn git_output(workspace: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git").arg("-C").arg(workspace).args(args).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn is_sha(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
