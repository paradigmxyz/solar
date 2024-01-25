#![allow(dead_code)]

use crate::{
    compute_diff::write_diff,
    errors::{Error, ErrorKind},
    Config, TestProps,
};
use assert_cmd::Command;
use once_cell::sync::Lazy;
use regex::{Captures, Regex};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    process::ExitStatus,
    time::Duration,
};

const TIMEOUT: Duration = Duration::from_millis(500);

pub enum TestOutput {
    Compile,
    Run,
}

#[derive(Debug, Clone)]
pub struct TestPaths {
    pub file: PathBuf,         // e.g., compile-test/foo/bar/baz.rs
    pub relative_dir: PathBuf, // e.g., foo/bar
}

pub struct TestCx<'a> {
    pub config: &'a Config,
    pub paths: TestPaths,
    pub src: &'a str,
    #[allow(dead_code)]
    pub props: TestProps,
    pub revision: Option<&'a str>,
}

impl TestCx<'_> {
    // NOTE: Adding `.env()` to the command SIGNIFICANTLY slows down tests due to every command now
    // having to re-capture the running process' environment.

    pub fn cmd(&self) -> Command {
        let mut cmd = self.cmd_common();
        cmd.arg("-Zui-testing");
        cmd.arg("--error-format=json");
        cmd
    }

    fn cmd_common(&self) -> Command {
        let mut cmd = Command::new(self.config.cmd);
        cmd.current_dir(self.config.root);
        cmd.arg("--color=always");
        cmd.timeout(TIMEOUT);
        cmd
    }

    pub fn run_cmd(&self, mut cmd: Command) -> ProcRes {
        let cmdline = format!("{cmd:?}");
        let output = cmd.output().unwrap();
        ProcRes {
            status: output.status,
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            cmdline,
        }
    }

    pub fn file(&self) -> &Path {
        &self.paths.file
    }

    pub fn relpath(&self) -> &Path {
        self.file().strip_prefix(self.config.root).expect("test path not in root")
    }

    pub fn load_compare_outputs(
        &self,
        proc_res: &ProcRes,
        output_kind: TestOutput,
        explicit_format: bool,
    ) -> usize {
        let (stderr_kind, stdout_kind) = match output_kind {
            TestOutput::Compile => (UI_STDERR, UI_STDOUT),
            TestOutput::Run => (UI_RUN_STDERR, UI_RUN_STDOUT),
        };

        let expected_stderr = self.load_expected_output(stderr_kind);
        let expected_stdout = self.load_expected_output(stdout_kind);

        let normalized_stdout =
            self.normalize_output(&proc_res.stdout, &self.props.normalize_stdout);

        let stderr = if explicit_format {
            proc_res.stderr.clone()
        } else {
            crate::json::extract_rendered(&proc_res.stderr)
        };

        let normalized_stderr = self.normalize_output(&stderr, &self.props.normalize_stderr);
        let mut errors = 0;
        match output_kind {
            TestOutput::Compile => {
                if !self.props.dont_check_compiler_stdout {
                    errors += self.compare_output(
                        stdout_kind,
                        &normalized_stdout,
                        &expected_stdout,
                        self.props.compare_output_lines_by_subset,
                    );
                }
                if !self.props.dont_check_compiler_stderr {
                    errors += self.compare_output(
                        stderr_kind,
                        &normalized_stderr,
                        &expected_stderr,
                        self.props.compare_output_lines_by_subset,
                    );
                }
            }
            TestOutput::Run => {
                errors += self.compare_output(
                    stdout_kind,
                    &normalized_stdout,
                    &expected_stdout,
                    self.props.compare_output_lines_by_subset,
                );
                errors += self.compare_output(
                    stderr_kind,
                    &normalized_stderr,
                    &expected_stderr,
                    self.props.compare_output_lines_by_subset,
                );
            }
        }
        errors
    }

    fn load_expected_output(&self, kind: &str) -> String {
        let path = self.expected_output_path(kind);
        if path.exists() {
            match self.load_expected_output_from_path(&path) {
                Ok(x) => x,
                Err(x) => self.fatal(&x),
            }
        } else {
            String::new()
        }
    }

    fn expected_output_path(&self, kind: &str) -> PathBuf {
        expected_output_path(self.file(), self.revision, kind)
    }

    fn load_expected_output_from_path(&self, path: &Path) -> Result<String, String> {
        fs::read_to_string(path).map_err(|err| {
            format!("failed to load expected output from `{}`: {}", path.display(), err)
        })
    }

    fn delete_file(&self, file: &PathBuf) {
        if !file.exists() {
            // Deleting a nonexistent file would error.
            return;
        }
        if let Err(e) = fs::remove_file(file) {
            self.fatal(&format!("failed to delete `{}`: {}", file.display(), e,));
        }
    }

    fn compare_output(
        &self,
        kind: &str,
        actual: &str,
        expected: &str,
        compare_output_by_lines: bool,
    ) -> usize {
        if actual == expected {
            return 0;
        }

        let tmp;
        let (expected, actual): (&str, &str) = if compare_output_by_lines {
            let actual_lines: HashSet<_> = actual.lines().collect();
            let expected_lines: Vec<_> = expected.lines().collect();
            let mut used = expected_lines.clone();
            used.retain(|line| actual_lines.contains(line));
            // check if `expected` contains a subset of the lines of `actual`
            if used.len() == expected_lines.len() && (expected.is_empty() == actual.is_empty()) {
                return 0;
            }
            if expected_lines.is_empty() {
                // if we have no lines to check, force a full overwite
                ("", actual)
            } else {
                tmp = (expected_lines.join("\n"), used.join("\n"));
                (&tmp.0, &tmp.1)
            }
        } else {
            (expected, actual)
        };

        if !self.config.bless {
            if expected.is_empty() {
                println!("normalized {kind}:\n{actual}\n");
            } else {
                println!("diff of {kind}:\n");
                print!("{}", write_diff(expected, actual, 3));
            }
        }

        let output_file = self
            .output_base_name()
            .with_extra_extension(self.revision.unwrap_or(""))
            .with_extra_extension(kind);

        let mut files = vec![output_file];
        if self.config.bless {
            // Delete non-revision .stderr/.stdout file if revisions are used.
            // Without this, we'd just generate the new files and leave the old files around.
            if self.revision.is_some() {
                let old = expected_output_path(self.file(), None, kind);
                self.delete_file(&old);
            }
            files.push(expected_output_path(self.file(), self.revision, kind));
        }

        for output_file in &files {
            if actual.is_empty() {
                self.delete_file(output_file);
            } else {
                println!("writing to {}", output_file.display());
                if let Err(err) = fs::write(output_file, actual) {
                    self.fatal(&format!(
                        "failed to write {} to `{}`: {}",
                        kind,
                        output_file.display(),
                        err,
                    ));
                }
            }
        }

        println!("\nThe actual {kind} differed from the expected {kind}.");
        for output_file in files {
            println!("Actual {} saved to {}", kind, output_file.display());
        }
        if self.config.bless {
            0
        } else {
            1
        }
    }

    pub fn check_expected_errors(&self, output: &ProcRes) {
        let expected_errors = &self.props.expected_errors[..];

        let is_solc = expected_errors.iter().all(|e| e.solc_kind.is_some());
        if is_solc {
            let expected_error = expected_errors.iter().find(|e| e.is_error());
            let failed = match (expected_error, output.status.success()) {
                (None, true) => false,
                (None, false) => {
                    eprintln!("\n---- unexpected error in {} ----", self.file().display());
                    true
                }
                (Some(e), true) => {
                    if e.solc_kind.unwrap().is_parser_error() {
                        eprintln!("\n---- unexpected success in {} ----", self.file().display());
                        eprintln!("-- expected error --\n{e:?}");
                        true
                    } else {
                        false
                    }
                }
                (Some(_e), false) => false,
            };
            if failed {
                dump_output(output);
                panic!();
            }
            return;
        }

        if output.status.success() && expected_errors.iter().any(Error::is_error) {
            self.fatal_proc_rec("process did not return an error status", output);
        }

        // On Windows, translate all '\' path separators to '/'
        let file_name = self.file().display().to_string().replace('\\', "/");

        // On Windows, keep all '\' path separators to match the paths reported in the JSON output
        // from the compiler
        let diagnostic_file_name = self.file().display().to_string();

        // If the testcase being checked contains at least one expected "help"
        // message, then we'll ensure that all "help" messages are expected.
        // Otherwise, all "help" messages reported by the compiler will be ignored.
        // This logic also applies to "note" messages.
        let expect_help = expected_errors.iter().any(|ee| ee.kind == Some(ErrorKind::Help));
        let expect_note = expected_errors.iter().any(|ee| ee.kind == Some(ErrorKind::Note));

        // Parse the JSON output from the compiler and extract out the messages.
        let actual_errors =
            crate::json::parse_output(&diagnostic_file_name, &output.stderr, output);
        let mut unexpected = Vec::new();
        let mut found = vec![false; expected_errors.len()];
        for mut actual_error in actual_errors {
            actual_error.msg = self.normalize_output(&actual_error.msg, &[]);

            let opt_index =
                expected_errors.iter().enumerate().position(|(index, expected_error)| {
                    !found[index]
                        && actual_error.line_num == expected_error.line_num
                        && (expected_error.kind.is_none()
                            || actual_error.kind == expected_error.kind)
                        && actual_error.msg.contains(&expected_error.msg)
                });

            match opt_index {
                Some(index) => {
                    // found a match, everybody is happy
                    assert!(!found[index]);
                    found[index] = true;
                }
                None => {
                    if self.is_unexpected_compiler_message(&actual_error, expect_help, expect_note)
                    {
                        self.error(&format!(
                            "{}:{}: unexpected {}: '{}'",
                            file_name,
                            actual_error.line_num,
                            actual_error
                                .kind
                                .as_ref()
                                .map_or(String::from("message"), |k| k.to_string()),
                            actual_error.msg
                        ));
                        unexpected.push(actual_error);
                    }
                }
            }
        }

        let mut not_found = Vec::new();
        // anything not yet found is a problem
        for (index, expected_error) in expected_errors.iter().enumerate() {
            if !found[index] {
                self.error(&format!(
                    "{}:{}: expected {} not found: {}",
                    file_name,
                    expected_error.line_num,
                    expected_error.kind.as_ref().map_or("message".into(), |k| k.to_string()),
                    expected_error.msg
                ));
                not_found.push(expected_error);
            }
        }

        if !unexpected.is_empty() || !not_found.is_empty() {
            self.error(&format!(
                "{} unexpected errors found, {} expected errors not found",
                unexpected.len(),
                not_found.len()
            ));
            println!("status: {}\ncommand: {}", output.status, output.cmdline);
            if !unexpected.is_empty() {
                println!("unexpected errors (from JSON output): {unexpected:#?}\n");
            }
            if !not_found.is_empty() {
                println!("not found errors (from test file): {not_found:#?}\n");
            }
            panic!();
        }
    }

    /// Returns `true` if we should report an error about `actual_error`,
    /// which did not match any of the expected error. We always require
    /// errors/warnings to be explicitly listed, but only require
    /// helps/notes if there are explicit helps/notes given.
    fn is_unexpected_compiler_message(
        &self,
        actual_error: &Error,
        expect_help: bool,
        expect_note: bool,
    ) -> bool {
        !actual_error.msg.is_empty()
            && match actual_error.kind {
                Some(ErrorKind::Help) => expect_help,
                Some(ErrorKind::Note) => expect_note,
                Some(ErrorKind::Error) | Some(ErrorKind::Warning) => true,
                Some(ErrorKind::Suggestion) | None => false,
            }
    }

    fn normalize_output(&self, output: &str, custom_rules: &[(String, String)]) -> String {
        // let rflags = self.props.run_flags.as_ref();
        // let cflags = self.props.compile_flags.join(" ");
        // let json = rflags
        //     .map_or(false, |s| s.contains("--format json") || s.contains("--format=json"))
        //     || cflags.contains("--error-format json")
        //     || cflags.contains("--error-format pretty-json")
        //     || cflags.contains("--error-format=json")
        //     || cflags.contains("--error-format=pretty-json")
        //     || cflags.contains("--output-format json")
        //     || cflags.contains("--output-format=json");
        let json = true;

        let mut normalized = output.to_string();

        let mut normalize_path = |from: &Path, to: &str| {
            let mut from = from.display().to_string();
            if json {
                from = from.replace('\\', "\\\\");
            }
            normalized = normalized.replace(&from, to);
        };

        let parent_dir = self.file().parent().unwrap();
        normalize_path(parent_dir, "$DIR");
        normalize_path(parent_dir.strip_prefix(self.config.root).unwrap(), "$DIR");

        // if self.props.remap_src_base {
        //     let mut remapped_parent_dir = PathBuf::from(FAKE_SRC_BASE);
        //     if self.testpaths.relative_dir != Path::new("") {
        //         remapped_parent_dir.push(&self.testpaths.relative_dir);
        //     }
        //     normalize_path(&remapped_parent_dir, "$DIR");
        // }

        // Paths into the build directory
        let test_build_dir = &self.config.build_base;
        let parent_build_dir = test_build_dir.parent().unwrap().parent().unwrap().parent().unwrap();

        // eg. /home/user/rust/build/x86_64-unknown-linux-gnu/test/ui
        normalize_path(test_build_dir, "$TEST_BUILD_DIR");
        // eg. /home/user/rust/build
        normalize_path(parent_build_dir, "$BUILD_DIR");

        if json {
            // escaped newlines in json strings should be readable
            // in the stderr files. There's no point int being correct,
            // since only humans process the stderr files.
            // Thus we just turn escaped newlines back into newlines.
            normalized = normalized.replace("\\n", "\n");
        }

        // If there are `$SRC_DIR` normalizations with line and column numbers, then replace them
        // with placeholders as we do not want tests needing updated when compiler source code
        // changes.
        // eg. $SRC_DIR/libcore/mem.rs:323:14 becomes $SRC_DIR/libcore/mem.rs:LL:COL
        static SRC_DIR_RE: Lazy<Regex> =
            Lazy::new(|| Regex::new("SRC_DIR(.+):\\d+:\\d+(: \\d+:\\d+)?").unwrap());

        normalized = SRC_DIR_RE.replace_all(&normalized, "SRC_DIR$1:LL:COL").into_owned();

        normalized = Self::normalize_platform_differences(&normalized);
        normalized = normalized.replace('\t', "\\t"); // makes tabs visible

        // Remove test annotations like `//~ ERROR text` from the output,
        // since they duplicate actual errors and make the output hard to read.
        // This mirrors the regex in src/tools/tidy/src/style.rs, please update
        // both if either are changed.
        static ANNOTATION_RE: Lazy<Regex> =
            Lazy::new(|| Regex::new("\\s*//(\\[.*\\])?~.*").unwrap());

        normalized = ANNOTATION_RE.replace_all(&normalized, "").into_owned();

        // Custom normalization rules
        for rule in custom_rules {
            let re = Regex::new(&rule.0).expect("bad regex in custom normalization rule");
            normalized = re.replace_all(&normalized, &rule.1[..]).into_owned();
        }
        normalized
    }

    /// Normalize output differences across platforms. Generally changes Windows output to be more
    /// Unix-like.
    ///
    /// Replaces backslashes in paths with forward slashes, and replaces CRLF line endings
    /// with LF.
    fn normalize_platform_differences(output: &str) -> String {
        /// Used to find Windows paths.
        ///
        /// It's not possible to detect paths in the error messages generally, but this is a
        /// decent enough heuristic.
        static PATH_BACKSLASH_RE: Lazy<Regex> = Lazy::new(|| {
            Regex::new(
                r#"(?x)
                (?:
                  # Match paths that don't include spaces.
                  (?:\\[\pL\pN\.\-_']+)+\.\pL+
                |
                  # If the path starts with a well-known root, then allow spaces and no file extension.
                  \$(?:DIR|SRC_DIR|TEST_BUILD_DIR|BUILD_DIR|LIB_DIR)(?:\\[\pL\pN\.\-_'\ ]+)+
                )"#,
            )
            .unwrap()
        });

        let output = output.replace(r"\\", r"\");

        PATH_BACKSLASH_RE
            .replace_all(&output, |caps: &Captures<'_>| {
                println!("{}", &caps[0]);
                caps[0].replace('\\', "/")
            })
            .replace("\r\n", "\n")
    }

    /// Gets the absolute path to the directory where all output for the given
    /// test/revision should reside.
    /// E.g., `/path/to/build/host-triple/test/ui/relative/testname.revision.mode/`.
    pub fn output_base_dir(&self) -> PathBuf {
        output_base_dir(self.config, &self.paths, self.revision)
    }

    /// Gets the absolute path to the base filename used as output for the given
    /// test/revision.
    /// E.g., `/.../relative/testname.revision.mode/testname`.
    pub fn output_base_name(&self) -> PathBuf {
        output_base_name(self.config, &self.paths, self.revision)
    }

    pub fn fatal_proc_rec(&self, err: &str, output: &ProcRes) -> ! {
        self.error(err);
        output.fatal(None);
    }

    pub fn error(&self, err: &str) {
        match self.revision {
            Some(rev) => println!("\nerror in revision `{rev}`: {err}"),
            None => println!("\nerror: {err}"),
        }
    }

    pub fn fatal(&self, err: &str) -> ! {
        self.error(err);
        panic!("fatal error");
    }
}

pub struct ProcRes {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
    pub cmdline: String,
}

impl ProcRes {
    pub fn fatal(&self, err: Option<&str>) -> ! {
        if let Some(e) = err {
            println!("\nerror: {e}");
        }
        dump_output(self);
        // Use resume_unwind instead of panic!() to prevent a panic message + backtrace from
        // compiletest, which is unnecessary noise.
        std::panic::resume_unwind(Box::new(()));
    }
}

fn dump_output(output: &ProcRes) {
    eprintln!("-- command --");
    eprintln!("{}", output.cmdline);
    eprintln!("-- status --");
    eprintln!("{}", output.status);
    let stdout = output.stdout.trim();
    if !stdout.is_empty() {
        eprintln!("-- stdout --");
        eprintln!("{stdout}");
    }
    let stderr = output.stderr.trim();
    if !stderr.is_empty() {
        eprintln!("-- stderr --");
        eprintln!("{stderr}");
    }
}

pub const UI_EXTENSIONS: &[&str] = &[
    UI_STDERR,
    UI_STDOUT,
    UI_FIXED,
    UI_RUN_STDERR,
    UI_RUN_STDOUT,
    UI_STDERR_64,
    UI_STDERR_32,
    UI_STDERR_16,
    UI_COVERAGE,
    UI_COVERAGE_MAP,
];
pub const UI_STDERR: &str = "stderr";
pub const UI_STDOUT: &str = "stdout";
pub const UI_FIXED: &str = "fixed";
pub const UI_RUN_STDERR: &str = "run.stderr";
pub const UI_RUN_STDOUT: &str = "run.stdout";
pub const UI_STDERR_64: &str = "64bit.stderr";
pub const UI_STDERR_32: &str = "32bit.stderr";
pub const UI_STDERR_16: &str = "16bit.stderr";
pub const UI_COVERAGE: &str = "coverage";
pub const UI_COVERAGE_MAP: &str = "cov-map";

/// Used by `ui` tests to generate things like `foo.stderr` from `foo.rs`.
fn expected_output_path(file: &Path, revision: Option<&str>, kind: &str) -> PathBuf {
    assert!(UI_EXTENSIONS.contains(&kind));
    let mut parts = Vec::new();

    if let Some(x) = revision {
        parts.push(x);
    }
    parts.push(kind);

    let extension = parts.join(".");
    file.with_extension(extension)
}

/// Absolute path to the directory where all output for all tests in the given
/// `relative_dir` group should reside. Example:
///   /path/to/build/host-triple/test/ui/relative/
/// This is created early when tests are collected to avoid race conditions.
pub fn output_relative_path(config: &Config, relative_dir: &Path) -> PathBuf {
    config.build_base.join(relative_dir)
}

/// Generates a unique name for the test, such as `testname.revision.mode`.
pub fn output_testname_unique(testpaths: &TestPaths, revision: Option<&str>) -> PathBuf {
    PathBuf::from(&testpaths.file.file_stem().unwrap()).with_extra_extension(revision.unwrap_or(""))
}

/// Absolute path to the directory where all output for the given
/// test/revision should reside. Example:
///   /path/to/build/host-triple/test/ui/relative/testname.revision.mode/
pub fn output_base_dir(config: &Config, testpaths: &TestPaths, revision: Option<&str>) -> PathBuf {
    output_relative_path(config, &testpaths.relative_dir)
        .join(output_testname_unique(testpaths, revision))
}

/// Absolute path to the base filename used as output for the given
/// test/revision. Example:
///   /path/to/build/host-triple/test/ui/relative/testname.revision.mode/testname
pub fn output_base_name(config: &Config, testpaths: &TestPaths, revision: Option<&str>) -> PathBuf {
    output_base_dir(config, testpaths, revision).join(testpaths.file.file_stem().unwrap())
}

/// Absolute path to the directory to use for incremental compilation. Example:
///   /path/to/build/host-triple/test/ui/relative/testname.mode/testname.inc
pub fn incremental_dir(config: &Config, testpaths: &TestPaths, revision: Option<&str>) -> PathBuf {
    output_base_name(config, testpaths, revision).with_extension("inc")
}

pub trait PathBufExt {
    /// Append an extension to the path, even if it already has one.
    fn with_extra_extension<S: AsRef<std::ffi::OsStr>>(&self, extension: S) -> PathBuf;
}

impl PathBufExt for PathBuf {
    fn with_extra_extension<S: AsRef<std::ffi::OsStr>>(&self, extension: S) -> PathBuf {
        if extension.as_ref().is_empty() {
            self.clone()
        } else {
            let mut fname = self.file_name().unwrap().to_os_string();
            if !extension.as_ref().to_str().unwrap().starts_with('.') {
                fname.push(".");
            }
            fname.push(extension);
            self.with_file_name(fname)
        }
    }
}
