//! Sweep test: compiling the self-contained corpus must never panic (ICE).
//!
//! Runs the freshly built `solar` binary over every self-contained `.sol` file
//! in the foundry suites' `src`/`test` directories and the tracked solc
//! benchmarks. A clean diagnostic (e.g. an unsupported construct) is fine — the
//! test only fails on an internal compiler error: a Rust panic or a crash by
//! signal. Files with `import`s are skipped, since they need a resolver/
//! remappings the foundry harness provides instead.
#![allow(unused_crate_dependencies)]
// This is a test harness scanning the corpus on disk, not the compiler reading
// sources, so the `SourceMap` file loader the lint steers toward does not apply.
#![allow(clippy::disallowed_methods)]

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn solar_binary() -> &'static str {
    env!("CARGO_BIN_EXE_solar")
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap().to_path_buf()
}

/// Recursively collects `.sol` files under `dir` that match `keep`.
fn collect_sol(dir: &Path, keep: &dyn Fn(&Path) -> bool, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_sol(&path, keep, out);
        } else if path.extension().is_some_and(|e| e == "sol") && keep(&path) {
            out.push(path);
        }
    }
}

/// A file is self-contained if it has no `import` directives, so `solar` can
/// resolve it standalone.
fn is_self_contained(path: &Path) -> bool {
    let Ok(src) = fs::read_to_string(path) else { return false };
    !src.lines().any(|line| line.trim_start().starts_with("import "))
}

#[test]
fn no_ice_on_self_contained_corpus() {
    let root = workspace_root();
    let mut files = Vec::new();

    // Foundry suite contracts and tests (their `lib/` is third-party).
    collect_sol(
        &root.join("tests/foundry"),
        &|p| {
            let s = p.to_string_lossy();
            !s.contains("/lib/") && (s.contains("/src/") || s.contains("/test/"))
        },
        &mut files,
    );
    // The tracked solc benchmark contracts.
    collect_sol(&root.join("testdata/solidity/test/benchmarks"), &|_| true, &mut files);

    files.retain(|p| is_self_contained(p));
    files.sort();
    assert!(!files.is_empty(), "no corpus files found under {}", root.display());

    let mut ices = Vec::new();
    for file in &files {
        let output = Command::new(solar_binary())
            .arg("-Zcodegen")
            .arg("--emit=bin-runtime")
            .arg(file)
            .output()
            .expect("failed to run solar");
        let stderr = String::from_utf8_lossy(&output.stderr);
        // A panic prints "panicked at ..."; the ICE handler prints "internal
        // compiler error"; a hard abort leaves no exit code (killed by signal).
        let panicked = stderr.contains("panicked")
            || stderr.contains("internal compiler error")
            || output.status.code().is_none();
        if panicked {
            let reason = stderr
                .lines()
                .find(|l| l.contains("panicked") || l.contains("internal compiler error"))
                .unwrap_or("crashed (no exit code)")
                .trim()
                .to_string();
            ices.push(format!("{}: {reason}", file.strip_prefix(&root).unwrap().display()));
        }
    }

    assert!(ices.is_empty(), "{} file(s) ICEd:\n{}", ices.len(), ices.join("\n"));
}
