//! Status emitter that prints only failures and the final summary.
//!
//! `ui_test`'s text emitter prints one `<path> ... ok` line per test when stdout is not a terminal,
//! which buries the few failures of a run under thousands of lines. This emitter prints the
//! failure header for each failed test and delegates the error rendering and the `test result:`
//! line to [`Text`], so those stay identical to the default output.

use std::{
    fmt::Debug,
    path::{Path, PathBuf},
};
use ui_test::{
    status_emitter::{RevisionStyle, StatusEmitter, Summary, TestStatus, Text},
    test_result::TestResult,
};

pub(crate) struct Quiet {
    summary: Text,
}

impl Quiet {
    pub(crate) fn new() -> Self {
        Self { summary: Text::verbose() }
    }
}

impl StatusEmitter for Quiet {
    fn register_test(&self, path: PathBuf) -> Box<dyn TestStatus> {
        Box::new(QuietTest { path, revision: String::new() })
    }

    fn finalize(
        &self,
        failed: usize,
        succeeded: usize,
        ignored: usize,
        filtered: usize,
        aborted: bool,
    ) -> Box<dyn Summary> {
        self.summary.finalize(failed, succeeded, ignored, filtered, aborted)
    }
}

struct QuietTest {
    path: PathBuf,
    revision: String,
}

impl TestStatus for QuietTest {
    fn for_revision(&self, revision: &str, _style: RevisionStyle) -> Box<dyn TestStatus> {
        Box::new(Self { path: self.path.clone(), revision: revision.to_string() })
    }

    fn for_path(&self, path: &Path) -> Box<dyn TestStatus> {
        Box::new(Self { path: path.to_path_buf(), revision: String::new() })
    }

    fn failed_test<'a>(
        &'a self,
        cmd: &'a str,
        _stderr: &'a [u8],
        _stdout: &'a [u8],
    ) -> Box<dyn Debug + 'a> {
        let revision = match self.revision.as_str() {
            "" => String::new(),
            revision => format!(" (revision `{revision}`)"),
        };
        println!("\nFAILED TEST: {}{revision}\ncommand: {cmd}\n", self.path.display());
        Box::new(())
    }

    fn done(&self, _result: &TestResult, _aborted: bool) {}

    fn path(&self) -> &Path {
        &self.path
    }

    fn revision(&self) -> &str {
        &self.revision
    }
}
