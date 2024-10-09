use crate::{
    context::{PathBufExt, TestOutput},
    Config, TestCx, TestFns, TestResult,
};
use std::path::Path;

pub(crate) const FNS: TestFns = TestFns { check, run };

fn check(_config: &Config, _path: &Path) -> TestResult {
    TestResult::Passed
}

fn run(cx: &TestCx<'_>) -> TestResult {
    let path = cx.paths.file.as_path();
    let mut cmd = cx.cmd();
    cmd.arg(path);
    if path.extension() == Some("yul".as_ref()) {
        cmd.arg("--language=yul").arg("-Zparse-yul");
    }
    cmd.args(&cx.props.compile_flags);
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

    if cx.props.filecheck_stdout {
        let stdout_path = cx.output_base_name().with_extra_extension("stdout");
        assert!(
            stdout_path.exists(),
            "stdout file missing for filecheck: {}",
            stdout_path.display()
        );
        let output = cx.verify_with_filecheck(&stdout_path);
        if !output.status.success() {
            cx.fatal_proc_rec("filecheck failed", &output);
        }
    }

    TestResult::Passed
}
