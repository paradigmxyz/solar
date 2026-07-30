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

use super::VfsPath;
use crate::file_operations::{FileMoveBatch, FileMoveError};
use crop::Rope;
use solar_interface::data_structures::map::rustc_hash::FxHashMap;
use std::{
    collections::hash_map::Entry,
    mem,
    path::{Path, PathBuf},
};

#[derive(Default)]
pub(crate) struct Vfs {
    data: FxHashMap<VfsPath, Rope>,
    versions: FxHashMap<VfsPath, i32>,
    content_revision: u64,
    dirty: bool,
}

impl Vfs {
    /// Set the contents of a file. A content of `None` means the file is to be removed from the
    /// VFS.
    pub(crate) fn set_file_contents(&mut self, path: VfsPath, contents: Option<Rope>) {
        self.set_file_contents_with_version(path, contents, None);
    }

    pub(crate) fn set_file_contents_with_version(
        &mut self,
        path: VfsPath,
        contents: Option<Rope>,
        version: Option<i32>,
    ) -> bool {
        if let Some(contents) = contents {
            let contents_changed = match self.data.entry(path.clone()) {
                Entry::Occupied(mut entry) => {
                    let changed = entry.get() != &contents;
                    entry.insert(contents);
                    changed
                }
                Entry::Vacant(entry) => {
                    entry.insert(contents);
                    true
                }
            };
            if let Some(version) = version {
                self.versions.insert(path, version);
            } else {
                self.versions.remove(&path);
            }
            if contents_changed {
                self.bump_content_revision();
            }
            self.dirty = true;
            contents_changed
        } else {
            let contents_changed = self.data.remove(&path).is_some();
            self.versions.remove(&path);
            if contents_changed {
                self.bump_content_revision();
            }
            self.dirty = true;
            contents_changed
        }
    }

    pub(crate) fn get_file_contents(&self, path: &VfsPath) -> Option<&Rope> {
        self.data.get(path)
    }

    pub(crate) fn get_file_version(&self, path: &VfsPath) -> Option<i32> {
        self.versions.get(path).copied()
    }

    pub(crate) fn content_revision(&self) -> u64 {
        self.content_revision
    }

    pub(crate) fn exists(&self, path: &VfsPath) -> bool {
        self.data.contains_key(path)
    }

    /// Renames exact files and directory descendants from one snapshot of the VFS.
    pub(crate) fn rename_file_prefixes(
        &mut self,
        moves: &FileMoveBatch,
    ) -> Result<(), FileMoveError> {
        if moves.is_empty() {
            return Ok(());
        }
        self.validate_rename_file_prefixes(moves)?;

        let mut old_versions = mem::take(&mut self.versions);
        let mut files = mem::take(&mut self.data)
            .into_iter()
            .map(|(path, contents)| {
                let version = old_versions.remove(&path);
                let new_path = path
                    .as_path()
                    .and_then(|path| moves.map_path(path))
                    .map_or_else(|| path.clone(), |(_, path)| VfsPath::from(path));
                (path, new_path, contents, version)
            })
            .collect::<Vec<_>>();
        files.sort_by(|(lhs, ..), (rhs, ..)| lhs.cmp(rhs));

        let changed = files.iter().any(|(old_path, new_path, ..)| old_path != new_path);
        for moved in [false, true] {
            for (old_path, new_path, contents, version) in &files {
                if (old_path != new_path) != moved || self.data.contains_key(new_path) {
                    continue;
                }
                self.data.insert(new_path.clone(), contents.clone());
                if let Some(version) = version {
                    self.versions.insert(new_path.clone(), *version);
                }
            }
        }
        self.dirty |= changed;
        if changed {
            self.bump_content_revision();
        }
        Ok(())
    }

    pub(crate) fn validate_rename_file_prefixes(
        &self,
        moves: &FileMoveBatch,
    ) -> Result<(), FileMoveError> {
        moves.validate_mapped_destinations(
            self.data.keys().filter_map(|path| path.as_path().map(Path::to_path_buf)),
        )
    }

    /// Removes exact files and directory descendants from the VFS.
    pub(crate) fn remove_file_prefixes(&mut self, deleted_paths: &[PathBuf]) {
        if deleted_paths.is_empty() {
            return;
        }

        let old_len = self.data.len();
        self.data.retain(|path, _| !has_file_prefix(path, deleted_paths));
        self.versions.retain(|path, _| !has_file_prefix(path, deleted_paths));
        let changed = self.data.len() != old_len;
        self.dirty |= changed;
        if changed {
            self.bump_content_revision();
        }
    }

    /// Whether the VFS is dirty or not.
    ///
    /// The VFS is considered dirty if a file was modified, changed, or removed.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "VFS dirty state is scaffolded for future incremental analysis"
        )
    )]
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

    fn bump_content_revision(&mut self) {
        self.content_revision =
            self.content_revision.checked_add(1).expect("VFS content revision counter exhausted");
    }
}

fn has_file_prefix(path: &VfsPath, prefixes: &[PathBuf]) -> bool {
    path.as_path().is_some_and(|path| prefixes.iter().any(|prefix| path.starts_with(prefix)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn path(path: &str) -> VfsPath {
        VfsPath::from(PathBuf::from(path))
    }

    fn insert(vfs: &mut Vfs, file_path: &str, contents: &str, version: i32) {
        vfs.set_file_contents_with_version(
            path(file_path),
            Some(Rope::from(contents)),
            Some(version),
        );
    }

    fn moves(moves: impl IntoIterator<Item = (PathBuf, PathBuf)>) -> FileMoveBatch {
        FileMoveBatch::new(moves).unwrap()
    }

    #[test]
    fn set_file_contents_reports_content_changes() {
        let mut vfs = Vfs::default();
        let file = path("/workspace/Test.sol");

        assert!(vfs.set_file_contents_with_version(
            file.clone(),
            Some(Rope::from("contract Test {}")),
            Some(1),
        ));
        let revision = vfs.content_revision();
        vfs.mark_clean();

        assert!(!vfs.set_file_contents_with_version(
            file.clone(),
            Some(Rope::from("contract Test {}")),
            Some(2),
        ));
        assert_eq!(vfs.content_revision(), revision);
        assert_eq!(vfs.get_file_version(&file), Some(2));
        assert!(vfs.is_dirty());

        assert!(vfs.set_file_contents_with_version(
            file.clone(),
            Some(Rope::from("contract Changed {}")),
            Some(3),
        ));
        assert_eq!(vfs.content_revision(), revision + 1);
        assert_eq!(vfs.get_file_version(&file), Some(3));
        assert!(vfs.set_file_contents_with_version(file.clone(), None, None));
        assert_eq!(vfs.content_revision(), revision + 2);
        assert_eq!(vfs.get_file_contents(&file), None);
        assert_eq!(vfs.get_file_version(&file), None);
        assert!(!vfs.set_file_contents_with_version(file, None, None));
        assert_eq!(vfs.content_revision(), revision + 2);
    }

    #[test]
    fn rename_file_prefixes_uses_one_snapshot_and_preserves_versions() {
        let mut vfs = Vfs::default();
        insert(&mut vfs, "/workspace/A.sol", "contract A {}", 1);
        insert(&mut vfs, "/workspace/B.sol", "contract B {}", 2);

        vfs.rename_file_prefixes(&moves([
            (PathBuf::from("/workspace/A.sol"), PathBuf::from("/workspace/B.sol")),
            (PathBuf::from("/workspace/B.sol"), PathBuf::from("/workspace/C.sol")),
        ]))
        .unwrap();

        assert!(!vfs.exists(&path("/workspace/A.sol")));
        assert_eq!(
            vfs.get_file_contents(&path("/workspace/B.sol")).unwrap().to_string(),
            "contract A {}"
        );
        assert_eq!(vfs.get_file_version(&path("/workspace/B.sol")), Some(1));
        assert_eq!(
            vfs.get_file_contents(&path("/workspace/C.sol")).unwrap().to_string(),
            "contract B {}"
        );
        assert_eq!(vfs.get_file_version(&path("/workspace/C.sol")), Some(2));
    }

    #[test]
    fn rename_file_prefixes_keeps_unmoved_destination_buffer() {
        let mut vfs = Vfs::default();
        insert(&mut vfs, "/workspace/Old.sol", "contract Old {}", 1);
        insert(&mut vfs, "/workspace/New.sol", "contract UnsavedNew {}", 7);

        vfs.rename_file_prefixes(&moves([(
            PathBuf::from("/workspace/Old.sol"),
            PathBuf::from("/workspace/New.sol"),
        )]))
        .unwrap();

        assert!(!vfs.exists(&path("/workspace/Old.sol")));
        assert_eq!(
            vfs.get_file_contents(&path("/workspace/New.sol")).unwrap().to_string(),
            "contract UnsavedNew {}"
        );
        assert_eq!(vfs.get_file_version(&path("/workspace/New.sol")), Some(7));
    }

    #[test]
    fn rename_file_prefixes_moves_descendants_without_matching_sibling_prefixes() {
        let mut vfs = Vfs::default();
        insert(&mut vfs, "/workspace/pkg/Nested.sol", "contract Nested {}", 3);
        insert(&mut vfs, "/workspace/pkg2/Keep.sol", "contract Keep {}", 4);

        vfs.rename_file_prefixes(&moves([(
            PathBuf::from("/workspace/pkg"),
            PathBuf::from("/workspace/moved"),
        )]))
        .unwrap();

        assert!(!vfs.exists(&path("/workspace/pkg/Nested.sol")));
        assert_eq!(
            vfs.get_file_contents(&path("/workspace/moved/Nested.sol")).unwrap().to_string(),
            "contract Nested {}"
        );
        assert_eq!(vfs.get_file_version(&path("/workspace/moved/Nested.sol")), Some(3));
        assert!(vfs.exists(&path("/workspace/pkg2/Keep.sol")));
    }

    #[test]
    fn rename_file_prefixes_prefers_the_most_specific_move() {
        let mut vfs = Vfs::default();
        insert(&mut vfs, "/workspace/pkg/nested/Test.sol", "contract Test {}", 5);

        vfs.rename_file_prefixes(&moves([
            (PathBuf::from("/workspace/pkg"), PathBuf::from("/workspace/moved")),
            (PathBuf::from("/workspace/pkg/nested"), PathBuf::from("/workspace/special")),
        ]))
        .unwrap();

        assert!(!vfs.exists(&path("/workspace/moved/nested/Test.sol")));
        assert_eq!(
            vfs.get_file_contents(&path("/workspace/special/Test.sol")).unwrap().to_string(),
            "contract Test {}"
        );
        assert_eq!(vfs.get_file_version(&path("/workspace/special/Test.sol")), Some(5));
    }

    #[test]
    fn rename_file_prefixes_rejects_expanded_destination_collision_atomically() {
        let mut vfs = Vfs::default();
        insert(&mut vfs, "/workspace/A/x.sol", "contract A {}", 1);
        insert(&mut vfs, "/workspace/B/x.sol", "contract B {}", 2);
        vfs.mark_clean();

        let error = vfs
            .rename_file_prefixes(&moves([
                (PathBuf::from("/workspace/A"), PathBuf::from("/workspace/out")),
                (PathBuf::from("/workspace/B/x.sol"), PathBuf::from("/workspace/out/x.sol")),
            ]))
            .unwrap_err();

        assert!(matches!(
            error,
            FileMoveError::ConflictingDestination { new_path, .. }
                if new_path == Path::new("/workspace/out/x.sol")
        ));
        assert_eq!(
            vfs.get_file_contents(&path("/workspace/A/x.sol")).unwrap().to_string(),
            "contract A {}"
        );
        assert_eq!(vfs.get_file_version(&path("/workspace/A/x.sol")), Some(1));
        assert_eq!(
            vfs.get_file_contents(&path("/workspace/B/x.sol")).unwrap().to_string(),
            "contract B {}"
        );
        assert_eq!(vfs.get_file_version(&path("/workspace/B/x.sol")), Some(2));
        assert!(!vfs.exists(&path("/workspace/out/x.sol")));
        assert!(!vfs.is_dirty());
    }

    #[test]
    fn remove_file_prefixes_removes_descendants_without_matching_sibling_prefixes() {
        let mut vfs = Vfs::default();
        insert(&mut vfs, "/workspace/pkg/Nested.sol", "contract Nested {}", 3);
        insert(&mut vfs, "/workspace/pkg2/Keep.sol", "contract Keep {}", 4);

        vfs.remove_file_prefixes(&[PathBuf::from("/workspace/pkg")]);

        assert!(!vfs.exists(&path("/workspace/pkg/Nested.sol")));
        assert!(vfs.exists(&path("/workspace/pkg2/Keep.sol")));
        assert_eq!(vfs.get_file_version(&path("/workspace/pkg/Nested.sol")), None);
        assert_eq!(vfs.get_file_version(&path("/workspace/pkg2/Keep.sol")), Some(4));
    }
}
