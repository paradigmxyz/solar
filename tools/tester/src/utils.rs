use std::path::Path;

pub(crate) fn path_contains(haystack: &Path, needle: &str) -> bool {
    let s = haystack.to_str().unwrap();
    #[cfg(windows)]
    let s = s.replace('\\', "/");
    s.contains(needle)
}
