use std::{
    io::{BufRead, BufReader},
    path::Path,
};

use crate::errors::Error;

/// Test file directives.
#[derive(Debug)]
pub struct TestProps {
    pub expected_errors: Vec<Error>,

    pub normalize_stdout: Vec<(String, String)>,
    pub normalize_stderr: Vec<(String, String)>,
    pub dont_check_compiler_stdout: bool,
    pub dont_check_compiler_stderr: bool,
    pub compare_output_lines_by_subset: bool,

    pub filecheck_stdout: bool,

    pub evm_version: Option<String>,
    pub compile_flags: Vec<String>,

    pub test_value: Option<String>,
}

impl Default for TestProps {
    fn default() -> Self {
        Self::new()
    }
}

impl TestProps {
    /// Creates the default `TestProps` instance.
    pub fn new() -> Self {
        Self {
            expected_errors: Vec::new(),
            normalize_stdout: Vec::new(),
            normalize_stderr: Vec::new(),
            dont_check_compiler_stdout: false,
            dont_check_compiler_stderr: false,
            compare_output_lines_by_subset: false,
            filecheck_stdout: false,
            evm_version: None,
            compile_flags: Vec::new(),
            test_value: None,
        }
    }

    /// Loads the test properties from a string.
    pub fn load(file: &str, cfg: Option<&str>) -> Self {
        let mut props = Self::new();
        props.expected_errors = Error::load(file.lines(), cfg);
        let comment = "//@";
        directives_str(comment, file, |revision, line, _| {
            if revision.is_some() && revision != cfg {
                return;
            }
            let mut parser = DirectiveParser::new(line);
            parser.parse_directive();
            match parser.directive.kind {
                DirectiveKind::DontCheckCompilerStdout => {
                    parser.bool(&mut props.dont_check_compiler_stdout)
                }
                DirectiveKind::DontCheckCompilerStderr => {
                    parser.bool(&mut props.dont_check_compiler_stderr)
                }
                DirectiveKind::CompareOutputLinesBySubset => {
                    parser.bool(&mut props.compare_output_lines_by_subset)
                }
                DirectiveKind::FilecheckStdout => parser.bool(&mut props.filecheck_stdout),

                DirectiveKind::EvmVersion => parser.word_value(&mut props.evm_version),
                DirectiveKind::CompileFlags => parser.words(&mut props.compile_flags),

                DirectiveKind::Dummy => {}
                DirectiveKind::TestValue => parser.word_value(&mut props.test_value),
            }
        });
        props
    }

    pub fn load_solc(file: &str, _cfg: Option<&str>) -> Self {
        let mut props = Self::new();
        props.expected_errors = Error::load_solc(file);
        props
    }

    /// Loads the test properties from a string.
    pub fn load_revisions(path: &Path) -> Vec<String> {
        let mut revisions = Vec::new();
        let comment = "//@";
        let file = std::fs::File::open(path).unwrap();
        directives_file(comment, file, |revision, line, _| {
            const S: &str = "revisions:";
            if revision.is_some() || !revisions.is_empty() || !line.starts_with(S) {
                return;
            }
            revisions.extend(line[S.len()..].split_ascii_whitespace().map(ToOwned::to_owned));
        });
        revisions
    }
}

struct TestDirective {
    negative: bool,
    kind: DirectiveKind,
}

impl TestDirective {
    const DUMMY: Self = Self { negative: false, kind: DirectiveKind::Dummy };
}

#[derive(Debug, PartialEq, Eq)]
enum DirectiveKind {
    DontCheckCompilerStdout,
    DontCheckCompilerStderr,
    CompareOutputLinesBySubset,
    FilecheckStdout,

    EvmVersion,

    CompileFlags,

    Dummy,
    TestValue,
}

impl DirectiveKind {
    fn from_str_(s: &str) -> Option<Self> {
        match s {
            "dont-check-compiler-stdout" => Some(Self::DontCheckCompilerStdout),
            "dont-check-compiler-stderr" => Some(Self::DontCheckCompilerStderr),
            "compare-output-lines-by-subset" => Some(Self::CompareOutputLinesBySubset),
            "filecheck-stdout" => Some(Self::FilecheckStdout),
            "evm-version" => Some(Self::EvmVersion),
            "test-value" => Some(Self::TestValue),
            "compile-flags" => Some(Self::CompileFlags),
            _ => None,
        }
    }
}

struct DirectiveParser<'a> {
    line: &'a str,
    directive: TestDirective,
}

impl<'a> DirectiveParser<'a> {
    fn new(line: &'a str) -> Self {
        Self { line, directive: TestDirective::DUMMY }
    }

    fn parse_directive(&mut self) {
        let mut word = self.next_word();
        if word.is_empty() {
            return;
        }

        let negative = word.starts_with("no-");
        if negative {
            word = &word[3..];
        }

        let Some(kind) = DirectiveKind::from_str_(word) else { return };

        self.line = &self.line[word.len()..];
        self.directive = TestDirective { negative, kind };
    }

    #[inline]
    fn bool(&mut self, value: &mut bool) {
        *value = !self.directive.negative;
    }

    fn word_value<T>(&mut self, value: &mut Option<T>)
    where
        T: std::str::FromStr,
        T::Err: std::fmt::Debug,
    {
        self.expect_no_negative();

        self.char(':');
        self.whitespace();

        let word = self.next_word();
        if word.is_empty() {
            panic!("expected a word value @ {:?}", self.line);
        }
        self.line = &self.line[word.len()..];
        *value = Some(word.parse().unwrap());
    }

    fn words<T>(&mut self, value: &mut Vec<T>)
    where
        T: std::str::FromStr,
        T::Err: std::fmt::Debug,
    {
        self.expect_no_negative();

        self.char(':');
        self.whitespace();

        value.extend(self.line.split_ascii_whitespace().map(|s| s.parse().unwrap()));
    }

    fn char(&mut self, c: char) {
        if !self.line.starts_with(c) {
            panic!("expected {c:?} @ {:?}", self.line);
        }
        self.line = &self.line[c.len_utf8()..];
    }

    fn whitespace(&mut self) {
        self.line = self.line.trim_start();
    }

    fn next_word(&self) -> &'a str {
        fn is_word_char(c: &u8) -> bool {
            matches!(c, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_')
        }
        let c = self.line.bytes().take_while(is_word_char).count();
        &self.line[..c]
    }

    fn expect_no_negative(&self) {
        if self.directive.negative {
            panic!("unexpected negative directive for {:?}", self.directive.kind);
        }
    }
}

fn directives_str(comment: &str, file: &str, mut it: impl FnMut(Option<&str>, &str, usize)) {
    for (line_number, line) in file.lines().enumerate() {
        if let Some((lncfg, ln)) = line_directive(comment, line) {
            it(lncfg, ln, line_number);
        }
    }
}

fn directives_file(
    comment: &str,
    file: std::fs::File,
    mut it: impl FnMut(Option<&str>, &str, usize),
) {
    let mut rdr = BufReader::new(file);
    let mut ln = String::new();
    let mut line_number = 0;

    loop {
        line_number += 1;
        ln.clear();
        if rdr.read_line(&mut ln).unwrap() == 0 {
            break;
        }

        let ln = ln.trim();
        if let Some((lncfg, ln)) = line_directive(comment, ln) {
            it(lncfg, ln, line_number);
        }
    }
}

/// Extract a `(Option<line_config>, directive)` directive from a line if comment is present.
fn line_directive<'line>(
    comment: &str,
    ln: &'line str,
) -> Option<(Option<&'line str>, &'line str)> {
    let ln = ln.trim_start();
    if let Some(ln) = ln.strip_prefix(comment) {
        let ln = ln.trim_start();
        if ln.starts_with('[') {
            // A comment like `//[foo]` is specific to revision `foo`
            let Some(close_brace) = ln.find(']') else {
                panic!("malformed condition directive: expected `{comment}[foo]`, found `{ln}`");
            };

            let lncfg = &ln[1..close_brace];
            Some((Some(lncfg), ln[(close_brace + 1)..].trim_start()))
        } else {
            Some((None, ln))
        }
    } else {
        None
    }
}
