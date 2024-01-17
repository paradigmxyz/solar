//! Sulk test runner.
//!
//! This crate is invoked in `crates/sulk/tests.rs`.

#![allow(unreachable_pub)]
#![cfg_attr(feature = "nightly", feature(test))]

#[cfg(feature = "nightly")]
extern crate test as _test;
#[cfg(feature = "nightly")]
use _test::test;
#[cfg(feature = "nightly")]
use tester as _;

#[cfg(not(feature = "nightly"))]
use tester::{self as _test, test};

use assert_cmd::Command;
use once_cell::sync::Lazy;
use regex::Regex;
use std::{
    path::Path,
    process::Output,
    sync::Arc,
    time::{Duration, Instant},
};

mod solc;
use solc::SolcError;

mod ui;

const TIMEOUT: Duration = Duration::from_millis(500);

static ERROR_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"// ----\r?\n(?://\s+Warning \d+: .*\n)*//\s+(\w+Error)( \d+)?: (.*)").unwrap()
});

type TestFn = fn(&Runner, &Path, check: bool) -> TestResult;

#[derive(Clone, Copy)]
pub enum Mode {
    Ui,
    SolcSolidity,
    SolcYul,
}

pub fn run_tests(cmd: &'static Path) {
    let args = std::env::args().collect::<Vec<_>>();
    let mut opts = match test::parse_opts(&args) {
        Some(Ok(o)) => o,
        Some(Err(msg)) => {
            eprintln!("error: {msg}");
            std::process::exit(101);
        }
        None => return,
    };
    // Condense output if not explicitly requested.
    let requested_pretty = || args.iter().any(|x| x.contains("--format"));
    if opts.format == _test::OutputFormat::Pretty && !requested_pretty() {
        opts.format = _test::OutputFormat::Terse;
    }
    // [`tester`] currently (0.9.1) uses `num_cpus::get_physical`;
    // use all available threads instead.
    if opts.test_threads.is_none() {
        opts.test_threads = std::thread::available_parallelism().map(|x| x.get()).ok();
    }

    let mut tests = Vec::new();
    for mode in [Mode::Ui, Mode::SolcSolidity, Mode::SolcYul] {
        let runner = Runner::new(cmd, mode);
        let inputs = runner.collect();
        let f = match mode {
            Mode::Ui => Runner::run_ui_test,
            Mode::SolcSolidity => Runner::run_solc_solidity_test,
            Mode::SolcYul => Runner::run_solc_yul_test,
        };
        make_tests(Arc::new(runner), &mut tests, &inputs, f);
    }
    tests.sort_by(|a, b| a.desc.name.as_slice().cmp(b.desc.name.as_slice()));

    match _test::run_tests_console(&opts, tests) {
        Ok(true) => {}
        Ok(false) => {
            println!("Some tests failed");
            std::process::exit(1);
        }
        Err(e) => {
            println!("I/O failure during tests: {e}");
            std::process::exit(1);
        }
    }
}

fn make_tests(
    runner: Arc<Runner>,
    tests: &mut Vec<test::TestDescAndFn>,
    inputs: &[walkdir::DirEntry],
    run: TestFn,
) {
    tests.reserve(inputs.len());
    for input in inputs {
        let runner = runner.clone();
        let path = input.path().to_path_buf();

        let rel_path = path.strip_prefix(runner.root).unwrap_or(&path);
        let mode = match runner.mode {
            Mode::Ui => "ui",
            Mode::SolcSolidity => "solc-solidity",
            Mode::SolcYul => "solc-yul",
        };
        let name = format!("[{mode}] {}", rel_path.display());
        let ignore_reason = match run(&runner, &path, true) {
            TestResult::Skipped(reason) => Some(reason),
            _ => None,
        };
        tests.push(test::TestDescAndFn {
            #[cfg(feature = "nightly")]
            desc: test::TestDesc {
                name: test::TestName::DynTestName(name),
                ignore: ignore_reason.is_some(),
                ignore_message: ignore_reason,
                source_file: "",
                start_line: 0,
                start_col: 0,
                end_line: 0,
                end_col: 0,
                should_panic: test::ShouldPanic::No,
                compile_fail: false,
                no_run: false,
                test_type: test::TestType::Unknown,
            },
            #[cfg(not(feature = "nightly"))]
            desc: test::TestDesc {
                name: test::TestName::DynTestName(name),
                ignore: ignore_reason.is_some(),
                should_panic: test::ShouldPanic::No,
                allow_fail: false,
                test_type: test::TestType::Unknown,
            },
            testfn: test::DynTestFn(Box::new(move || {
                let r = run(&runner, &path, false);
                if r == TestResult::Failed {
                    #[cfg(not(feature = "nightly"))]
                    panic!("test failed");
                    #[cfg(feature = "nightly")]
                    return Err(String::from("test failed"));
                }
                #[cfg(feature = "nightly")]
                Ok(())
            })),
        })
    }
}

#[derive(Clone)]
struct Runner {
    cmd: &'static Path,
    root: &'static Path,
    mode: Mode,
    verbose: bool,
}

impl Runner {
    fn new(cmd: &'static Path, mode: Mode) -> Self {
        Self {
            cmd,
            root: Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap(),
            mode,
            verbose: false,
        }
    }

    fn collect(&self) -> Vec<walkdir::DirEntry> {
        let path = match self.mode {
            Mode::Ui => self.root.join("tests/ui/"),
            Mode::SolcSolidity => self.root.join("testdata/solidity/test/"),
            Mode::SolcYul => self.root.join("testdata/solidity/test/libyul/"),
        };
        let yul = match self.mode {
            Mode::Ui => true,
            Mode::SolcSolidity => false,
            Mode::SolcYul => true,
        };
        self.collect_files(&path, yul)
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
        if self.verbose {
            eprintln!("collected {} test files in {time:#?}", r.len());
        }
        r
    }

    fn time<R>(&self, f: impl FnOnce() -> R) -> (Duration, R) {
        let stopwatch = Instant::now();
        let r = f();
        let elapsed = stopwatch.elapsed();
        (elapsed, r)
    }

    fn cmd(&self) -> Command {
        let mut cmd = Command::new(self.cmd);
        cmd.current_dir(self.root)
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
