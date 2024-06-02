use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TestResult {
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

pub(crate) fn path_contains(haystack: &Path, needle: &str) -> bool {
    let s = haystack.to_str().unwrap();
    #[cfg(windows)]
    let s = s.replace('\\', "/");
    s.contains(needle)
}

/// Sets the default [`yansi`] color output condition.
pub(crate) fn enable_paint() {
    let enable = yansi::Condition::os_support() && yansi::Condition::tty_and_color_live();
    yansi::whenever(yansi::Condition::cached(enable));
}
