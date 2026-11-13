//! `-Zemit-lean-stages` never changes code generation, and what it writes does not depend on the
//! host or the run.
//!
//! Compiles every Solidity source of the codegen UI tests, the solar-tv milestones and examples,
//! and the verifier's examples at `-Ogas` and `-Osize`, with and without the flag. A source
//! the compiler accepts must give the same output, byte for byte, when exporting; a source the
//! exporter rejects must fail with no output and no manifest, so that nothing binds its stages.
//! Every accepted export must write the same files with one thread as with several.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

const SOLAR: &str = env!("CARGO_BIN_EXE_solar");

/// The optimization levels whose code the export must leave as it is.
const LEVELS: [&str; 2] = ["-Ogas", "-Osize"];

#[test]
fn export_keeps_bytecode_and_is_deterministic() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let sources = sources(&root);
    assert!(sources.len() > 1000, "found only {} sources", sources.len());
    let next = AtomicUsize::new(0);
    let failures = Mutex::new(Vec::new());
    let counts = Mutex::new(BTreeMap::<&str, usize>::new());
    let workers = std::thread::available_parallelism().map_or(1, |n| n.get());
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                while let Some(source) = sources.get(next.fetch_add(1, Ordering::Relaxed)) {
                    for level in LEVELS {
                        let label = check(&root, source, level).unwrap_or_else(|message| {
                            let failure = format!("{} {level}: {message}", source.display());
                            failures.lock().unwrap().push(failure);
                            "failed"
                        });
                        *counts.lock().unwrap().entry(label).or_default() += 1;
                    }
                }
            });
        }
    });
    let failures = failures.into_inner().unwrap();
    let counts = counts.into_inner().unwrap();
    assert!(failures.is_empty(), "{counts:?}\n{}", failures.join("\n"));
    assert!(counts.get("exported").copied().unwrap_or(0) > 100, "{counts:?}");
}

/// Compiles `source` at `level` with and without the export; returns what happened.
fn check(root: &Path, source: &Path, level: &str) -> Result<&'static str, String> {
    let args = [level, "--evm-version", "osaka", "--emit=bin,bin-runtime"];
    let plain = solar(root, source, &args, &[]);
    if !plain.status.success() {
        return Ok("not compiled");
    }
    let dir = tempfile::tempdir().unwrap();
    let flag = format!("-Zemit-lean-stages={}", dir.path().display());
    let exported = solar(root, source, &args, &[&flag]);
    if !exported.status.success() {
        if !exported.stdout.is_empty() {
            return Err("the rejected export printed output".into());
        }
        let manifests = files(dir.path())
            .into_keys()
            .filter(|path| path.ends_with("manifest.json") || path.ends_with("Manifest.lean"))
            .collect::<Vec<_>>();
        if !manifests.is_empty() {
            return Err(format!("the rejected export wrote manifests: {manifests:?}"));
        }
        return Ok("rejected");
    }
    if exported.stdout != plain.stdout {
        return Err("-Zemit-lean-stages changed the output".into());
    }
    let written = files(dir.path());
    if !written.keys().any(|path| path.ends_with("manifest.json")) {
        return Ok("no stages");
    }
    // The same files with one thread, up to the command line the manifests record.
    let one = tempfile::tempdir().unwrap();
    let flag = format!("-Zemit-lean-stages={}", one.path().display());
    let single = solar(root, source, &args, &[&flag, "-j1"]);
    if !single.status.success() || single.stdout != plain.stdout {
        return Err("the export with one thread differs in its output".into());
    }
    let written = normalize(&written, dir.path());
    let single = normalize(&files(one.path()), one.path());
    if written != single {
        let differ = written
            .keys()
            .chain(single.keys())
            .filter(|path| written.get(*path) != single.get(*path))
            .collect::<Vec<_>>();
        return Err(format!("the export with one thread writes other files: {differ:?}"));
    }
    Ok("exported")
}

fn solar(root: &Path, source: &Path, args: &[&str], extra: &[&str]) -> Output {
    Command::new(SOLAR)
        .current_dir(root)
        .arg(source)
        .args(args)
        .args(extra)
        .env("SOLAR_TEST_VERSION", "1")
        .output()
        .unwrap()
}

/// The sources to compile, relative to the repository root.
fn sources(root: &Path) -> Vec<PathBuf> {
    let mut sources = Vec::new();
    for dir in [
        "tests/ui/codegen",
        "solar-tv/milestone",
        "solar-tv/examples/solidity",
        "crates/verify/examples",
    ] {
        walk(root, Path::new(dir), &mut sources);
    }
    sources.sort();
    sources
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root.join(dir)) else { return };
    for entry in entries.flatten() {
        let path = dir.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            // Auxiliary sources are imported by the tests beside them, not compiled alone.
            if entry.file_name() != "auxiliary" {
                walk(root, &path, out);
            }
        } else if path.extension().is_some_and(|ext| ext == "sol") {
            out.push(path);
        }
    }
}

/// Every file under `dir`, by its path relative to `dir`.
// The exported files are the compiler's output, not sources the source map loads.
#[allow(clippy::disallowed_methods)]
fn files(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut paths = Vec::new();
    walk_files(dir, dir, &mut paths);
    paths.into_iter().map(|(name, path)| (name, fs::read(path).unwrap())).collect()
}

fn walk_files(base: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) {
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if entry.file_type().unwrap().is_dir() {
            walk_files(base, &path, out);
        } else {
            out.push((path.strip_prefix(base).unwrap().display().to_string(), path));
        }
    }
}

/// The files with the command line left out of the manifests, and the directory they were written
/// to in the recorded options replaced by `DIR`: the options must not record the thread count.
fn normalize(files: &BTreeMap<String, Vec<u8>>, dir: &Path) -> BTreeMap<String, Vec<u8>> {
    files
        .iter()
        .map(|(path, bytes)| {
            let bytes = if path.ends_with("manifest.json") {
                let mut json = serde_json::from_slice::<serde_json::Value>(bytes).unwrap();
                let object = json.as_object_mut().unwrap();
                object.remove("arguments");
                let options = object["options"].as_str().unwrap();
                object["options"] = options.replace(&dir.display().to_string(), "DIR").into();
                serde_json::to_vec(&json).unwrap()
            } else if path.ends_with("Manifest.lean") {
                let text = String::from_utf8(bytes.clone()).unwrap();
                let start = text.find("def arguments").unwrap();
                let end = text[start..].find("\n\n").unwrap() + start;
                format!("{}{}", &text[..start], &text[end..]).into_bytes()
            } else {
                bytes.clone()
            };
            (path.clone(), bytes)
        })
        .collect()
}
