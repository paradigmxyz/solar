//! Sulk test runner.
//!
//! This crate is invoked in `crates/sulk/tests.rs`.

#![allow(unreachable_pub)]

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

mod solc;
use solc::SolcError;

const TIMEOUT: Duration = Duration::from_millis(500);

pub fn solc_solidity_tests(cmd: &'static Path) {
    Runner::new(cmd).run_solc_solidity_tests();
}

pub fn solc_yul_tests(cmd: &'static Path) {
    Runner::new(cmd).run_solc_yul_tests();
}

struct Runner {
    cmd: &'static Path,
    root: PathBuf,

    error_re: Regex,
    source_delimiter: Regex,
    external_source_delimiter: Regex,
    #[allow(dead_code)]
    equals: Regex,

    passed_count: AtomicUsize,
    failed_count: AtomicUsize,
    skipped_count: AtomicUsize,
}

impl Runner {
    fn new(cmd: &'static Path) -> Self {
        Self {
            cmd,
            root: Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../")).canonicalize().unwrap(),

            error_re: Regex::new(
                r"// ----\r?\n(?://\s+Warning \d+: .*\n)*//\s+(\w+Error)( \d+)?: (.*)",
            )
            .unwrap(),
            source_delimiter: Regex::new(r"==== Source: (.*) ====").unwrap(),
            external_source_delimiter: Regex::new(r"==== ExternalSource: (.*) ====").unwrap(),
            equals: Regex::new("([a-zA-Z0-9_]+)=(.*)").unwrap(),

            passed_count: AtomicUsize::new(0),
            failed_count: AtomicUsize::new(0),
            skipped_count: AtomicUsize::new(0),
        }
    }

    fn run_solc_solidity_tests(&self) {
        eprintln!("running Solc Solidity tests with {}", self.cmd.display());

        let (collect_time, paths) = self.time(|| {
            WalkDir::new(self.root.join("testdata/solidity/test/"))
                .sort_by_file_name()
                .into_iter()
                .map(|entry| entry.unwrap())
                .filter(|entry| entry.path().extension() == Some("sol".as_ref()))
                .filter(|entry| solc_solidity_filter(entry.path()).is_none())
                .collect::<Vec<_>>()
        });
        eprintln!("collected {} test files in {collect_time:#?}", paths.len());

        let run = |entry: &walkdir::DirEntry| {
            let path = entry.path();
            let skip = |reason: &str| {
                let _ = reason;
                // eprintln!("---- skipping {} ({reason}) ----", path.display());
                TestResult::Skipped
            };

            if let Some(reason) = solc_solidity_skip(path) {
                return skip(reason);
            }

            let rel_path = path.strip_prefix(&self.root).expect("test path not in root");

            let Ok(src) = fs::read_to_string(path) else {
                return skip("invalid UTF-8");
            };
            let src = src.as_str();

            if self.source_delimiter.is_match(src) || self.external_source_delimiter.is_match(src) {
                return skip("matched delimiters");
            }

            let expected_error =
                self.error_re.captures(src).map(|captures| captures.get(3).unwrap().as_str());

            let mut cmd = self.cmd();
            cmd.arg(rel_path);
            self.run_cmd(&mut cmd, |output| match (expected_error, output.status.success()) {
                (None, true) => TestResult::Passed,
                (None, false) => {
                    eprintln!("\n---- unexpected error in {} ----", rel_path.display());
                    TestResult::Failed
                }
                (Some(e), true) => {
                    // TODO: Most of these are not syntax errors.
                    // eprintln!("\n---- unexpected success in {} ----", rel_path.display());
                    // eprintln!("-- expected error --\n{e}");
                    // true
                    let _ = e;
                    TestResult::Passed
                }
                (Some(_e), false) => TestResult::Passed,
            })
        };
        self.run_tests(&paths, run);
    }

    fn run_solc_yul_tests(&self) {
        eprintln!("running Solc Yul tests with {}", self.cmd.display());

        let object_re = Regex::new(r#"object\s*"(.*)"\s*\{"#).unwrap();

        let (collect_time, paths) = self.time(|| {
            WalkDir::new(self.root.join("testdata/solidity/test/libyul/"))
                .sort_by_file_name()
                .into_iter()
                .map(|entry| entry.unwrap())
                // Some tests are `.sol` but still in Yul.
                .filter(|entry| {
                    entry.path().extension() == Some("sol".as_ref())
                        || entry.path().extension() == Some("yul".as_ref())
                })
                .collect::<Vec<_>>()
        });
        eprintln!("collected {} test files in {collect_time:#?}", paths.len());

        let run = |entry: &walkdir::DirEntry| {
            let path = entry.path();
            let rel_path = path.strip_prefix(&self.root).expect("test path not in root");

            let skip = |reason: &str| {
                let _ = reason;
                // eprintln!("---- skipping {} ({reason}) ----", path.display());
                TestResult::Skipped
            };

            if let Some(reason) = solc_yul_skip(path) {
                return skip(reason);
            }

            let Ok(src) = fs::read_to_string(path) else {
                return skip("invalid UTF-8");
            };
            let src = src.as_str();

            if object_re.is_match(src) {
                return skip("object syntax is not yet supported");
            }

            if self.source_delimiter.is_match(src) || self.external_source_delimiter.is_match(src) {
                return skip("matched delimiters");
            }

            let error = self.get_expected_error(src);

            let mut cmd = self.cmd();
            cmd.arg("--language=yul").arg(rel_path);
            self.run_cmd(&mut cmd, |output| match (error, output.status.success()) {
                (None, true) => TestResult::Passed,
                (None, false) => {
                    // eprintln!("\n---- unexpected error in {} ----", rel_path.display());
                    // TestResult::Failed
                    TestResult::Passed
                }
                (Some(e), true) => {
                    if e.kind.parse_time_error() {
                        eprintln!("\n---- unexpected success in {} ----", rel_path.display());
                        eprintln!("-- expected error --\n{e}");
                        TestResult::Failed
                    } else {
                        TestResult::Passed
                    }
                }
                (Some(_e), false) => TestResult::Passed,
            })
        };
        self.run_tests(&paths, run);
    }

    fn run_tests<'a, T, F>(&self, inputs: &'a [T], run: F)
    where
        T: Send,
        [T]: IntoParallelRefIterator<'a, Item = &'a T>,
        F: Fn(&'a T) -> TestResult + Send + Sync,
    {
        let run = |input| {
            let result = run(input);
            let counter = match result {
                TestResult::Passed => &self.passed_count,
                TestResult::Skipped => &self.skipped_count,
                TestResult::Failed => &self.failed_count,
            };
            counter.fetch_add(1, Ordering::Relaxed);
        };
        let run_all = || inputs.par_iter().for_each(run);
        let run_all_real = || std::panic::catch_unwind(std::panic::AssertUnwindSafe(run_all));

        let (test_time, res) = self.time(run_all_real);
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

        let total = inputs.len();
        let passed = self.passed_count.load(Ordering::Relaxed);
        let skipped = self.skipped_count.load(Ordering::Relaxed);
        let failed = self.failed_count.load(Ordering::Relaxed);

        eprintln!("{total} tests: {passed} passed; {failed} failed; {skipped} skipped; finished in {test_time:#?}");
        if failed > 0 {
            panic!("some tests failed");
        }
    }

    fn time<R>(&self, f: impl FnOnce() -> R) -> (Duration, R) {
        let stopwatch = Instant::now();
        let r = f();
        let elapsed = stopwatch.elapsed();
        (elapsed, r)
    }

    fn cmd(&self) -> Command {
        let mut cmd = Command::new(self.cmd);
        cmd.current_dir(&self.root)
            .env("__SULK_IN_INTEGRATION_TEST", "1")
            .arg("--color=always")
            .timeout(TIMEOUT);
        cmd
    }

    fn run_cmd(&self, cmd: &mut Command, f: impl FnOnce(&Output) -> TestResult) -> TestResult {
        let output = cmd.output().unwrap();
        let r = f(&output);
        if r == TestResult::Failed {
            dump_output(&output);
        }
        r
    }

    fn get_expected_error<'a>(&self, haystack: &'a str) -> Option<SolcError> {
        self.error_re.captures(haystack).map(|captures| SolcError {
            kind: captures.get(1).unwrap().as_str().parse().unwrap(),
            code: captures.get(2).unwrap().as_str().trim_start().parse().unwrap(),
            message: captures.get(3).unwrap().as_str().to_owned(),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TestResult {
    Failed,
    Passed,
    Skipped,
}

impl TestResult {
    #[allow(dead_code)]
    fn from_success(b: bool) -> Self {
        if b {
            Self::Passed
        } else {
            Self::Failed
        }
    }
}

// Filter -> should not run
// Skip -> should run but we currently don't for reasons

fn solc_solidity_filter(path: &Path) -> Option<&str> {
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

fn solc_solidity_skip(path: &Path) -> Option<&str> {
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

fn solc_yul_skip(path: &Path) -> Option<&str> {
    if path_contains(path, "recursion_depth.yul") {
        return Some("stack overflow");
    }

    if path_contains(path, "/verbatim") {
        return Some("verbatim builtin is not implemented");
    }

    let stem = path.file_stem().unwrap().to_str().unwrap();
    #[rustfmt::skip]
    if matches!(
        stem,
        // Why should this fail?
        | "unicode_comment_direction_override"
        // This is custom test syntax, don't know why a test testing test syntax exists.
        | "surplus_input"
        // TODO: Probably implement outside of parsing.
        | "number_literals_3"
        | "number_literals_4"
        // TODO: Implemented with Yul object syntax.
        | "datacopy_shadowing"
        | "dataoffset_shadowing"
        | "datasize_shadowing"
        | "linkersymbol_shadowing"
        | "loadimmutable_shadowing"
        | "setimmutable_shadowing"
        // TODO: Special case this in the parser?
        | "pc_disallowed"
        // TODO: Not parser related, but should be implemented later.
        | "for_statement_nested_continue"
    ) {
        return Some("manually skipped");
    };
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
