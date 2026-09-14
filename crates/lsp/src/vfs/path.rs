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
    #[expect(dead_code, reason = "We do not use virtual paths yet")]
    pub(crate) fn new_virtual_path(path: String) -> Self {
        assert!(path.starts_with('/'));
        Self(VfsPathRepr::VirtualPath(VirtualPath(path)))
    }

    /// Returns the `Path` representation of `self` if `self` is on the file system.
    pub(crate) fn as_path(&self) -> Option<&Path> {
        match &self.0 {
            VfsPathRepr::PathBuf(it) => Some(it.as_path()),
            VfsPathRepr::VirtualPath(_) => None,
        }
    }
}

/// Internal, private representation of [`VfsPath`].
#[derive(Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
enum VfsPathRepr {
    /// This is guaranteed to be absolute.
    PathBuf(PathBuf),
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
        match self {
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
