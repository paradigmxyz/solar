use normalize_path::NormalizePath;
use std::{
    fmt,
    path::{Path, PathBuf},
};

/// A normalized, absolute file system path in [`Vfs`].
///
/// [`Vfs`]: super::Vfs
#[derive(Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub(crate) struct VfsPath(PathBuf);

impl VfsPath {
    pub(crate) fn as_path(&self) -> &Path {
        &self.0
    }
}

impl From<PathBuf> for VfsPath {
    fn from(v: PathBuf) -> Self {
        Self(v.normalize())
    }
}

impl fmt::Display for VfsPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.to_string_lossy().fmt(f)
    }
}

impl fmt::Debug for VfsPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl PartialEq<Path> for VfsPath {
    fn eq(&self, other: &Path) -> bool {
        self.0 == other
    }
}

impl PartialEq<VfsPath> for Path {
    fn eq(&self, other: &VfsPath) -> bool {
        other == self
    }
}
