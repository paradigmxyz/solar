use crate::{Config, TestCx, TestFns, TestResult};
use std::path::Path;

pub(crate) const FNS: TestFns = TestFns { check, run };

fn check(_config: &Config, _path: &Path) -> TestResult {
    // let rel_path = path.strip_prefix(config.root).expect("test path not in root");

    #[allow(clippy::nonminimal_bool)]
    if true {
        return TestResult::Skipped("ui tests are not implemented yet");
    }

    TestResult::Passed
}

fn run(cx: &TestCx<'_>) -> TestResult {
    let _ = cx;
    // let TestCx { config, path, src, props } = *cx;
    // let mut cmd = config.ui_cmd();
    // cmd.arg(rel_path);
    // let _ = cmd;
    TestResult::Passed
}
