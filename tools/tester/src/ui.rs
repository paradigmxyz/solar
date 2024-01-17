use crate::{Runner, TestResult};
use std::path::Path;

impl Runner {
    #[allow(unreachable_code)]
    pub(crate) fn run_ui_test(&self, path: &Path, check: bool) -> TestResult {
        let _ = path;
        let _ = check;

        // TODO
        return TestResult::Passed;

        let mut cmd = self.cmd();
        cmd.arg("--error-format=json");
        todo!();
    }
}
