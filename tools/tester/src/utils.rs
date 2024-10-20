use std::path::Path;

pub(crate) fn path_contains_curry(haystack: &Path) -> impl Fn(&str) -> bool + '_ {
    let s = haystack.to_str().unwrap();
    #[cfg(windows)]
    let s = s.replace('\\', "/");
    move |needle| s.contains(needle)
}
