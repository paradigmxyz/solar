//! Virtual File System
//!
//! The VFS is an overlay on top of the regular file system. Any files not in the VFS (e.g. imports)
//! are assumed to be read by the Solar compiler.
//!
//! Files in the VFS are pushed to the Solar compiler context via constructed in-memory
//! [`SourceFile`]s. The files in the VFS at any time are primarily files that are open by the LSP
//! client, and as such are not up to date on disk, as they are managed by the client.
//!
//! The VFS currently is just a set of dumb-ish maps, and some crude change detection, which is
//! useful for knowing when to trigger another analysis pass, or the flywheel check.
//!
//! If performance becomes a bottleneck, the VFS is an excellent starting point, as there are a few
//! readily available optimizations we can do, such as path interning, and moving IO out of the hot
//! path, which would be more [`rust-analyzer`](https://github.com/rust-lang/rust-analyzer/)-esque.
//!
//! It is also possible to change the VFS to use a [rope](https://en.wikipedia.org/wiki/Rope_(data_structure)) internally. Originally this was considered, but it does not seem to offer a lot of performance benefit in regular use cases for the scale that most Solidity projects have.
//!
//! We can also cache source files in-memory as we compile, as the compiler output includes all
//! loaded source files along with their paths. This can prevent additional IO, but care must be
//! taken here as to not end up loading the entire project into memory needlessly.
//!
//! [`SourceFile`]: solar_interface::source_map::SourceFile

use crop::Rope;
use solar_interface::data_structures::map::rustc_hash::FxHashMap;

use super::VfsPath;

#[derive(Default)]
pub(crate) struct Vfs {
    data: FxHashMap<VfsPath, Rope>,
    dirty: bool,
}

impl Vfs {
    /// Set the contents of a file. A content of `None` means the file is to be removed from the
    /// VFS.
    pub(crate) fn set_file_contents(&mut self, path: VfsPath, contents: Option<Rope>) {
        if let Some(contents) = contents {
            self.data.insert(path, contents);
        } else {
            self.data.remove(&path);
        }
        self.dirty = true;
    }

    pub(crate) fn get_file_contents(&self, path: &VfsPath) -> Option<&Rope> {
        self.data.get(path)
    }

    pub(crate) fn exists(&self, path: &VfsPath) -> bool {
        self.data.contains_key(path)
    }

    /// Whether the VFS is dirty or not.
    ///
    /// The VFS is considered dirty if a file was modified, changed, or removed.
    pub(crate) fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Mark the VFS as clean and return whether it was dirty to begin with.
    pub(crate) fn mark_clean(&mut self) -> bool {
        let was_dirty = self.dirty;
        self.dirty = false;
        was_dirty
    }

    /// Returns an iterator over stored paths and their corresponding contents.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&VfsPath, &Rope)> {
        self.data.iter()
    }
}
