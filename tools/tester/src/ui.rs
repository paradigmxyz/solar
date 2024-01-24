use crate::{context::TestOutput, Config, TestCx, TestFns, TestResult};
use std::path::Path;

pub(crate) const FNS: TestFns = TestFns { check, run };

fn check(_config: &Config, _path: &Path) -> TestResult {
    TestResult::Passed
}

fn run(cx: &TestCx<'_>) -> TestResult {
    let path = cx.paths.file.as_path();
    let mut cmd = cx.ui_cmd();
    cmd.arg(path);
    let output = cx.run_cmd(cmd);

    let errors = cx.load_compare_outputs(&output, TestOutput::Compile, false);

    if errors > 0 {
        println!("To update references, rerun the tests and pass the `--bless` flag");
        let relative_path_to_file = cx.paths.relative_dir.join(cx.paths.file.file_name().unwrap());
        println!(
            "To only update this specific test, also pass `--test-args {}`",
            relative_path_to_file.display(),
        );
        cx.fatal_proc_rec(&format!("{errors} errors occurred comparing output."), &output);
    }

    cx.check_expected_errors(&output);

    TestResult::Passed
}
