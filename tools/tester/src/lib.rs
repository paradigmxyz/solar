//! Sulk test runner.
//!
//! This crate is invoked in `crates/sulk/tests.rs`.

#![allow(unreachable_pub)]

use assert_cmd::Command;
use once_cell::sync::Lazy;
use rayon::prelude::*;
use regex::Regex;
use std::{
    collections::HashMap,
    path::Path,
    process::Output,
    sync::{Mutex, PoisonError},
    time::{Duration, Instant},
};

mod solc;
use solc::SolcError;

mod ui;

const TIMEOUT: Duration = Duration::from_millis(500);

static ERROR_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"// ----\r?\n(?://\s+Warning \d+: .*\n)*//\s+(\w+Error)( \d+)?: (.*)").unwrap()
});

pub enum Mode {
    Ui,
    SolcSolidity,
    SolcYul,
}

pub fn run_tests(cmd: &'static Path, mode: Mode) {
    let runner = Runner::new(cmd);
    match mode {
        Mode::Ui => runner.run_ui_tests(),
        Mode::SolcSolidity => runner.run_solc_solidity_tests(),
        Mode::SolcYul => runner.run_solc_yul_tests(),
    }
}

struct Runner {
    cmd: &'static Path,
    root: &'static Path,
}

impl Runner {
    fn new(cmd: &'static Path) -> Self {
        Self {
            cmd,
            root: Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap(),
        }
    }

    fn collect_files(&self, path: &Path, yul: bool) -> Vec<walkdir::DirEntry> {
        let (time, r) = self.time(|| {
            walkdir::WalkDir::new(path)
                .sort_by_file_name()
                .into_iter()
                .map(|entry| entry.unwrap())
                .filter(|entry| {
                    entry.path().extension() == Some("sol".as_ref())
                        || (yul && entry.path().extension() == Some("yul".as_ref()))
                })
                .collect::<Vec<_>>()
        });
        eprintln!("collected {} test files in {time:#?}", r.len());
        r
    }

    fn run_tests<'a, T, F>(&self, inputs: &'a [T], run: F)
    where
        T: std::fmt::Debug + Send + Sync,
        [T]: IntoParallelRefIterator<'a, Item = &'a T>,
        F: Fn(&'a T) -> TestResult + Send + Sync,
    {
        let results = Mutex::new(Vec::with_capacity(inputs.len()));
        let run = |input| {
            let stopwatch = Instant::now();
            let result = run(input);
            let elapsed = stopwatch.elapsed();
            results.lock().unwrap_or_else(PoisonError::into_inner).push((input, result, elapsed));
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

        let mut results = results.into_inner().unwrap();
        results.sort_by_key(|(_, _, d)| *d);
        let total = inputs.len();
        let mut passed = 0;
        let mut skipped = 0;
        let mut failed = 0;
        let mut all_skipped = HashMap::<&'static str, usize>::new();
        for (i, (t, result, time)) in results.iter().rev().enumerate() {
            if i < 10 {
                let _ = (i, t, time);
                // eprintln!("- {result:?} in {time:#?} for {t:#?}");
            }
            let counter = match result {
                TestResult::Passed => &mut passed,
                TestResult::Skipped(_) => &mut skipped,
                TestResult::Failed => &mut failed,
            };
            *counter += 1;
            if let TestResult::Skipped(reason) = result {
                *all_skipped.entry(reason).or_default() += 1;
            }
        }

        if !all_skipped.is_empty() {
            let mut v = all_skipped.into_iter().collect::<Vec<_>>();
            v.sort_by_key(|(_, count)| *count);
            let max_count = v.iter().map(|(_, count)| *count).max().unwrap();
            let max_count_len = max_count.to_string().len();
            eprintln!();
            for (reason, count) in v.into_iter().rev() {
                eprintln!("skipped {count:>max_count_len$}: {reason}");
            }
        }

        eprintln!("\n{total} tests: {passed} passed; {failed} failed; {skipped} skipped; finished in {test_time:#?}");
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
            .env("RUST_LOG", "debug")
            .arg("--color=always")
            .timeout(TIMEOUT);
        cmd
    }

    fn run_cmd(&self, cmd: &mut Command, f: impl FnOnce(&Output) -> TestResult) -> TestResult {
        let output = cmd.output().unwrap();
        let r = f(&output);
        if r == TestResult::Failed {
            dump_output(cmd, &output);
        }
        r
    }

    fn get_expected_error(&self, haystack: &str) -> Option<SolcError> {
        ERROR_RE.captures(haystack).map(|captures| SolcError {
            kind: captures.get(1).unwrap().as_str().parse().unwrap(),
            code: captures.get(2).map(|m| m.as_str().trim_start().parse().unwrap()),
            message: captures.get(3).unwrap().as_str().to_owned(),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TestResult {
    Failed,
    Passed,
    Skipped(&'static str),
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

fn dump_output(cmd: &Command, output: &Output) {
    eprintln!("-- command --");
    eprintln!("{cmd:?}");
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
    let s = haystack.to_str().unwrap();
    #[cfg(windows)]
    let s = s.replace('\\', "/");
    s.contains(needle)
}
