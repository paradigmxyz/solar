//! Sulk test runner.
//!
//! This crate is used to run

use assert_cmd::Command;
use rayon::prelude::*;
use regex::Regex;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Output,
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};
use walkdir::WalkDir;

const TIMEOUT: Duration = Duration::from_millis(500);

pub fn solc_tests(cmd: &'static Path) {
    Runner::new(cmd).run_solc_tests();
}

struct Runner {
    cmd: &'static Path,
    root: PathBuf,
}

impl Runner {
    fn new(cmd: &'static Path) -> Self {
        let root =
            Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../")).canonicalize().unwrap();
        Self { cmd, root }
    }

    fn run_solc_tests(&self) {
        eprintln!("running solc tests with {}", self.cmd.display());

        let error_re = r"// ----\r?\n(//\s+Warning \d+: .*\n)*//\s+\w+Error( \d+)?: (.*)";
        let error_re = Regex::new(error_re).unwrap();

        let source_delimiter = Regex::new(r"==== Source: (.*) ====").unwrap();
        let external_source_delimiter = Regex::new(r"==== ExternalSource: (.*) ====").unwrap();
        // let equals = Regex::new("([a-zA-Z0-9_]+)=(.*)").unwrap();

        let stopwatch = Instant::now();
        let paths: Vec<_> = WalkDir::new(self.root.join("testdata/solidity/test/"))
            .sort_by_file_name()
            .into_iter()
            .map(|entry| entry.unwrap())
            .filter(|entry| entry.path().extension() == Some("sol".as_ref()))
            .filter(|entry| solc_test_filter(entry.path()).is_none())
            .collect();
        let collect_time = stopwatch.elapsed();
        let total = paths.len();

        eprintln!("collected {total} test files in {collect_time:#?}");

        let passed_count = AtomicUsize::new(0);
        let failed_count = AtomicUsize::new(0);
        let skipped_count = AtomicUsize::new(0);

        let run = |entry: &walkdir::DirEntry| {
            // if failed.load(Ordering::SeqCst) {
            //     return;
            // }

            let path = entry.path();
            let skip = |reason: &str| {
                let _ = reason;
                // eprintln!("---- skipping {} ({reason}) ----", path.display());
                skipped_count.fetch_add(1, Ordering::Relaxed);
            };

            if let Some(reason) = solc_test_skip_reason(path) {
                skip(reason);
                return;
            }

            let rel_path = path.strip_prefix(&self.root).expect("test path not in root");

            let Ok(src) = fs::read_to_string(path) else {
                skip("invalid UTF-8");
                return;
            };
            let src = src.as_str();

            if source_delimiter.is_match(src) || external_source_delimiter.is_match(src) {
                skip("matched delimiters");
                return;
            }

            let mut cmd = self.cmd();
            cmd.arg(rel_path);
            let output = cmd.output().unwrap();

            let expected_error =
                error_re.captures(src).map(|captures| captures.get(3).unwrap().as_str());

            let failed_test = match (expected_error, output.status.success()) {
                (None, true) => false,
                (None, false) => {
                    eprintln!("\n---- unexpected error in {} ----", rel_path.display());
                    true
                }
                (Some(e), true) => {
                    // TODO: Most of these are not syntax errors.
                    // eprintln!("\n---- unexpected success in {} ----", rel_path.display());
                    // eprintln!("-- expected error --\n{e}");
                    // true
                    let _ = e;
                    false
                }
                (Some(_e), false) => false,
            };
            if failed_test {
                dump_output(&output);
                failed_count.fetch_add(1, Ordering::Relaxed);
            } else {
                passed_count.fetch_add(1, Ordering::Relaxed);
            }
        };
        let run_all = || paths.par_iter().for_each(run);

        let stopwatch = Instant::now();
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(run_all));
        let test_time = stopwatch.elapsed();
        match res {
            Ok(()) => {}
            Err(e) => {
                let msg = if let Some(s) = e.downcast_ref::<&'static str>() {
                    *s
                } else if let Some(s) = e.downcast_ref::<String>() {
                    s.as_str()
                } else {
                    "Box<dyn Any>"
                };
                eprintln!("test runner panicked with {msg}");
            }
        };

        let passed = passed_count.into_inner();
        let skipped = skipped_count.into_inner();
        let failed = failed_count.into_inner();

        eprintln!("{total} tests: {passed} passed; {failed} failed; {skipped} skipped; finished in {test_time:#?}");
        if failed > 0 {
            panic!("some tests failed");
        }
    }

    fn cmd(&self) -> Command {
        let mut cmd = Command::new(self.cmd);
        cmd.current_dir(&self.root).arg("--color=always").timeout(TIMEOUT);
        cmd
    }
}

fn solc_test_filter(path: &Path) -> Option<&str> {
    if path_contains(path, "libyul") {
        return Some("actually a Yul test");
    }

    if path_contains(path, "cmdlineTests") {
        return Some("not same format as everything else");
    }

    if path_contains(path, "experimental") {
        return Some("solidity experimental");
    }

    // We don't parse licenses.
    if path_contains(path, "license") {
        return Some("license test");
    }

    None
}

fn solc_test_skip_reason(path: &Path) -> Option<&str> {
    let stem = path.file_stem().unwrap().to_str().unwrap();
    if matches!(
        stem,
        // Exponent is too large, but apparently it's fine in Solc because the result is 0.
        "rational_number_exp_limit_fine"
    ) {
        return Some("manually skipped");
    }

    None
}

fn dump_output(output: &Output) {
    eprintln!("-- status --");
    eprintln!("{}", output.status);
    let stdout = utf8(&output.stdout).trim();
    if !stdout.is_empty() {
        eprintln!("-- stdout --");
        eprintln!("{stdout}");
    }
    let stderr = utf8(&output.stderr).trim();
    if !stderr.is_empty() {
        eprintln!("-- stderr --");
        eprintln!("{stderr}");
    }
}

fn utf8(s: &[u8]) -> &str {
    std::str::from_utf8(s).expect("could not decode utf8")
}

fn path_contains(haystack: &Path, needle: &str) -> bool {
    if needle.contains('/') {
        let s = haystack.to_str().unwrap();
        #[cfg(windows)]
        let s = s.replace('\\', "/");
        s.contains(needle)
    } else {
        haystack.components().any(|c| c.as_os_str() == needle)
    }
}
