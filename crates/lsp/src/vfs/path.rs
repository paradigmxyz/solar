use normalize_path::NormalizePath;
use std::{
    fmt,
    path::{Path, PathBuf},
};

/// A path in [`Vfs`].
///
/// The VFS contains both virtual and real files, which is why we don't just use the path types in
/// `std`. This also means `VfsPath` is an opaque identifier.
///
/// Adapted from [`rust-analyzer`](https://github.com/rust-lang/rust-analyzer).
///
/// [`Vfs`]: super::Vfs
#[derive(Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub(crate) struct VfsPath(VfsPathRepr);

impl VfsPath {
    /// Creates an "in-memory" path from `/`-separated string.
    ///
    /// This is most useful for testing, to avoid windows/linux differences
    ///
    /// # Panics
    ///
    /// Panics if `path` does not start with `'/'`.
    #[expect(unused, reason = "We do not use virtual paths yet.")]
    pub(crate) fn new_virtual_path(path: String) -> Self {
        assert!(path.starts_with('/'));
        Self(VfsPathRepr::VirtualPath(VirtualPath(path)))
    }

    /// Create a path from string.
    ///
    /// The input should be a string representation of an absolute path inside filesystem.
    ///
    /// # Panics
    ///
    /// Panics if the path is not absolute.
    pub(crate) fn new_real_path(path: String) -> Self {
        let p: PathBuf = path.into();
        if !p.is_absolute() {
            panic!("expected an absolute path, got {}", p.to_string_lossy())
        }

        Self::from(p)
    }

    /// Returns the `Path` representation of `self` if `self` is on the file system.
    pub(crate) fn as_path(&self) -> Option<&Path> {
        match &self.0 {
            VfsPathRepr::PathBuf(it) => Some(it.as_path()),
            VfsPathRepr::VirtualPath(_) => None,
        }
    }

    pub(crate) fn into_abs_path(self) -> Option<PathBuf> {
        match self.0 {
            VfsPathRepr::PathBuf(it) => Some(it),
            VfsPathRepr::VirtualPath(_) => None,
        }
    }

    /// Creates a new `VfsPath` with `path` adjoined to `self`.
    pub(crate) fn join(&self, path: &str) -> Option<Self> {
        match &self.0 {
            VfsPathRepr::PathBuf(it) => {
                let res = it.join(path).normalize();
                Some(Self(VfsPathRepr::PathBuf(res)))
            }
            VfsPathRepr::VirtualPath(it) => {
                let res = it.join(path)?;
                Some(Self(VfsPathRepr::VirtualPath(res)))
            }
        }
    }

    /// Remove the last component of `self` if there is one.
    ///
    /// If `self` has no component, returns `false`; else returns `true`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut path = VfsPath::from(PathBuf::from("/foo/bar".into()));
    /// assert!(path.pop());
    /// assert_eq!(path, VfsPath::from(PathBuf::from("/foo".into())));
    /// assert!(path.pop());
    /// assert_eq!(path, VfsPath::from(PathBuf::from("/".into())));
    /// assert!(!path.pop());
    /// ```
    pub(crate) fn pop(&mut self) -> bool {
        match &mut self.0 {
            VfsPathRepr::PathBuf(it) => it.pop(),
            VfsPathRepr::VirtualPath(it) => it.pop(),
        }
    }

    /// Returns `true` if `other` is a prefix of `self`.
    pub(crate) fn starts_with(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (VfsPathRepr::PathBuf(lhs), VfsPathRepr::PathBuf(rhs)) => lhs.starts_with(rhs),
            (VfsPathRepr::VirtualPath(lhs), VfsPathRepr::VirtualPath(rhs)) => lhs.starts_with(rhs),
            (VfsPathRepr::PathBuf(_) | VfsPathRepr::VirtualPath(_), _) => false,
        }
    }

    /// Returns the `VfsPath` without its final component, if there is one.
    ///
    /// Returns [`None`] if the path is a root or prefix.
    pub(crate) fn parent(&self) -> Option<Self> {
        let mut parent = self.clone();
        if parent.pop() { Some(parent) } else { None }
    }
}

/// Internal, private representation of [`VfsPath`].
#[derive(Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
enum VfsPathRepr {
    /// This is guaranteed to be absolute.
    PathBuf(PathBuf),
    #[expect(unused, reason = "We do not use virtual paths yet.")]
    VirtualPath(VirtualPath),
}

impl From<PathBuf> for VfsPath {
    fn from(v: PathBuf) -> Self {
        Self(VfsPathRepr::PathBuf(v.normalize()))
    }
}

impl fmt::Display for VfsPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            VfsPathRepr::PathBuf(it) => it.to_string_lossy().fmt(f),
            VfsPathRepr::VirtualPath(VirtualPath(it)) => it.fmt(f),
        }
    }
}

impl fmt::Debug for VfsPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl fmt::Debug for VfsPathRepr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self {
            Self::PathBuf(it) => it.fmt(f),
            Self::VirtualPath(VirtualPath(it)) => it.fmt(f),
        }
    }
}

impl PartialEq<Path> for VfsPath {
    fn eq(&self, other: &Path) -> bool {
        match &self.0 {
            VfsPathRepr::PathBuf(lhs) => lhs == other,
            VfsPathRepr::VirtualPath(_) => false,
        }
    }
}
impl PartialEq<VfsPath> for Path {
    fn eq(&self, other: &VfsPath) -> bool {
        other == self
    }
}

/// `/`-separated virtual path.
///
/// This is used to describe files that do not reside on the file system.
#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
struct VirtualPath(String);

impl VirtualPath {
    /// Returns `true` if `other` is a prefix of `self` (as strings).
    #[expect(unused, reason = "We do not use virtual paths yet.")]
    fn starts_with(&self, other: &Self) -> bool {
        self.0.starts_with(&other.0)
    }

    /// Remove the last component of `self`.
    ///
    /// This will find the last `'/'` in `self`, and remove everything after it,
    /// including the `'/'`.
    ///
    /// If `self` contains no `'/'`, returns `false`; else returns `true`.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let mut path = VirtualPath("/foo/bar".to_string());
    /// path.pop();
    /// assert_eq!(path.0, "/foo");
    /// path.pop();
    /// assert_eq!(path.0, "");
    /// ```
    fn pop(&mut self) -> bool {
        let pos = match self.0.rfind('/') {
            Some(pos) => pos,
            None => return false,
        };
        self.0 = self.0[..pos].to_string();
        true
    }

    /// Append the given *relative* path `path` to `self`.
    ///
    /// This will resolve any leading `"../"` in `path` before appending it.
    ///
    /// Returns [`None`] if `path` has more leading `"../"` than the number of
    /// components in `self`.
    ///
    /// # Notes
    ///
    /// In practice, appending here means `self/path` as strings.
    #[expect(unused, reason = "We do not use virtual paths yet.")]
    fn join(&self, mut path: &str) -> Option<Self> {
        let mut res = self.clone();
        while path.starts_with("../") {
            if !res.pop() {
                return None;
            }
            path = &path["../".len()..];
        }
        path = path.trim_start_matches("./");
        res.0 = format!("{}/{path}", res.0);
        Some(res)
    }
}
