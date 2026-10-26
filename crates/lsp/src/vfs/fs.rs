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
use crate::{
    file_operations::{FileMoveBatch, FileMoveError},
    folding_range,
    proto::LspPositionIndex,
    selection_range::SelectionRangeIndex,
    signature_help::StatementBoundaryIndex,
};
use crop::Rope;
use lsp_types::{Position, SelectionRange};
use solar_interface::data_structures::{map::rustc_hash::FxHashMap, sync::Mutex};
use std::{
    collections::hash_map::Entry,
    mem,
    path::PathBuf,
    sync::{Arc, OnceLock},
};

struct VfsFile {
    contents: Rope,
    analysis_source: OnceLock<Arc<String>>,
    positions: OnceLock<LspPositionIndex<Rope>>,
    selection_range_index: OnceLock<SelectionRangeIndex>,
    folding_ranges: OnceLock<Vec<lsp_types::FoldingRange>>,
    first_statement_boundary: OnceLock<(usize, usize)>,
    statement_boundaries: Mutex<StatementBoundaryIndex>,
}

impl VfsFile {
    fn new(contents: Rope) -> Self {
        Self {
            contents,
            analysis_source: OnceLock::new(),
            positions: OnceLock::new(),
            selection_range_index: OnceLock::new(),
            folding_ranges: OnceLock::new(),
            first_statement_boundary: OnceLock::new(),
            statement_boundaries: Mutex::default(),
        }
    }

    fn analysis_source(&self) -> Arc<String> {
        self.analysis_source
            .get_or_init(|| Arc::new(crate::utils::rope_to_string(&self.contents)))
            .clone()
    }
}

/// An exact-content handle for sharing source text and its position index.
#[derive(Clone)]
pub(crate) struct DocumentSource(Arc<VfsFile>);

impl DocumentSource {
    pub(crate) fn contents(&self) -> &Rope {
        &self.0.contents
    }

    pub(crate) fn positions(&self) -> &LspPositionIndex<Rope> {
        self.0.positions.get_or_init(|| LspPositionIndex::from_rope(self.0.contents.clone()))
    }

    pub(crate) fn source(&self) -> Arc<String> {
        self.0.analysis_source()
    }

    /// Returns the last statement boundary before any cursor in this exact source snapshot.
    pub(crate) fn statement_boundary(&self, cursor: usize) -> usize {
        if let Some(&(first_cursor, boundary)) = self.0.first_statement_boundary.get() {
            if first_cursor == cursor {
                return boundary;
            }
            return self.0.statement_boundaries.lock().at(&self.source(), cursor);
        }
        // A single request after an edit needs no index. Only build the broader index when
        // the cursor moves, keeping the first request's scan allocation-free.
        let source = self.source();
        let boundary = crate::signature_help::last_statement_boundary(&source[..cursor]);
        let _ = self.0.first_statement_boundary.set((cursor, boundary));
        boundary
    }
}

/// An exact-content handle for lazily answering selection-range requests.
#[derive(Clone)]
pub(crate) struct SelectionRangeSource(Arc<VfsFile>);

impl SelectionRangeSource {
    fn index(&self) -> &SelectionRangeIndex {
        self.0.selection_range_index.get_or_init(|| {
            SelectionRangeIndex::new(self.0.analysis_source(), self.0.contents.clone())
        })
    }

    pub(crate) fn selection_ranges(&self, positions: &[Position]) -> Option<Vec<SelectionRange>> {
        self.index().selection_ranges(positions)
    }
}

/// An exact-content handle whose folding ranges are parsed at most once.
#[derive(Clone)]
pub(crate) struct FoldingRangeSource(Arc<VfsFile>);

impl FoldingRangeSource {
    pub(crate) fn folding_ranges(&self) -> Vec<lsp_types::FoldingRange> {
        self.0
            .folding_ranges
            .get_or_init(|| folding_range::folding_ranges_from_rope(self.0.contents.clone()))
            .clone()
    }
}

#[derive(Default)]
pub(crate) struct Vfs {
    data: FxHashMap<VfsPath, Arc<VfsFile>>,
    versions: FxHashMap<VfsPath, i32>,
    content_revision: u64,
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
        let contents_changed = if let Some(contents) = contents {
            let changed = match self.data.entry(path.clone()) {
                Entry::Occupied(entry) if entry.get().contents == contents => false,
                entry => {
                    entry.insert_entry(Arc::new(VfsFile::new(contents)));
                    true
                }
            };
            match version {
                Some(version) => self.versions.insert(path, version),
                None => self.versions.remove(&path),
            };
            changed
        } else {
            self.versions.remove(&path);
            self.data.remove(&path).is_some()
        };
        if contents_changed {
            self.bump_content_revision();
        }
        contents_changed
    }

    /// Update an existing file's client version without replacing its contents.
    pub(crate) fn set_file_version(&mut self, path: VfsPath, version: i32) {
        debug_assert!(self.data.contains_key(&path));
        self.versions.insert(path, version);
    }

    pub(crate) fn get_file_contents(&self, path: &VfsPath) -> Option<&Rope> {
        self.data.get(path).map(|file| &file.contents)
    }

    /// Returns a shared contiguous source for compiler analysis.
    pub(crate) fn get_file_analysis_source(&self, path: &VfsPath) -> Option<Arc<String>> {
        self.data.get(path).map(|file| file.analysis_source())
    }

    pub(crate) fn get_file_source(&self, path: &VfsPath) -> Option<DocumentSource> {
        self.data.get(path).cloned().map(DocumentSource)
    }

    /// Returns an exact-content handle whose derived index can initialize outside the VFS lock.
    pub(crate) fn get_file_selection_range_source(
        &self,
        path: &VfsPath,
    ) -> Option<SelectionRangeSource> {
        self.data.get(path).cloned().map(SelectionRangeSource)
    }

    pub(crate) fn get_file_folding_range_source(
        &self,
        path: &VfsPath,
    ) -> Option<FoldingRangeSource> {
        self.data.get(path).cloned().map(FoldingRangeSource)
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
                let new_path = moves
                    .map_path(path.as_path())
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
        self.record_change(changed);
        Ok(())
    }

    pub(crate) fn validate_rename_file_prefixes(
        &self,
        moves: &FileMoveBatch,
    ) -> Result<(), FileMoveError> {
        moves
            .validate_mapped_destinations(self.data.keys().map(|path| path.as_path().to_path_buf()))
    }

    /// Removes exact files and directory descendants from the VFS.
    pub(crate) fn remove_file_prefixes(&mut self, deleted_paths: &[PathBuf]) {
        if deleted_paths.is_empty() {
            return;
        }

        let old_len = self.data.len();
        self.data.retain(|path, _| !has_file_prefix(path, deleted_paths));
        self.versions.retain(|path, _| !has_file_prefix(path, deleted_paths));
        self.record_change(self.data.len() != old_len);
    }

    /// Returns an iterator over stored paths and their corresponding contents.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&VfsPath, &Rope)> {
        self.data.iter().map(|(path, file)| (path, &file.contents))
    }

    fn record_change(&mut self, changed: bool) {
        if changed {
            self.bump_content_revision();
        }
    }

    fn bump_content_revision(&mut self) {
        self.content_revision =
            self.content_revision.checked_add(1).expect("VFS content revision counter exhausted");
    }
}

fn has_file_prefix(path: &VfsPath, prefixes: &[PathBuf]) -> bool {
    prefixes.iter().any(|prefix| path.as_path().starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    type Files<'a> = [(&'a str, &'a str, i32)];

    fn path(path: &str) -> VfsPath {
        VfsPath::from(PathBuf::from(path))
    }

    fn insert(vfs: &mut Vfs, file_path: &str, contents: &str, version: i32) -> bool {
        vfs.set_file_contents_with_version(
            path(file_path),
            Some(Rope::from(contents)),
            Some(version),
        )
    }

    fn workspace_vfs(files: &Files<'_>) -> Vfs {
        let mut vfs = Vfs::default();
        for &(file, contents, version) in files {
            insert(&mut vfs, &format!("/workspace/{file}"), contents, version);
        }
        vfs
    }

    /// Returns every file as a workspace-relative path, its contents, and its version.
    fn files(vfs: &Vfs) -> Vec<(String, String, Option<i32>)> {
        let mut files = vfs
            .iter()
            .map(|(path, contents)| {
                let file = path.as_path().strip_prefix("/workspace").unwrap();
                let file = file.to_string_lossy().replace('\\', "/");
                (file, contents.to_string(), vfs.get_file_version(path))
            })
            .collect::<Vec<_>>();
        files.sort();
        files
    }

    fn expected(files: &Files<'_>) -> Vec<(String, String, Option<i32>)> {
        files
            .iter()
            .map(|&(file, contents, version)| (file.into(), contents.into(), Some(version)))
            .collect()
    }

    fn moves(moves: &[(&str, &str)]) -> FileMoveBatch {
        FileMoveBatch::new(moves.iter().map(|(old, new)| {
            (Path::new("/workspace").join(old), Path::new("/workspace").join(new))
        }))
        .unwrap()
    }

    #[test]
    fn set_file_contents_reports_content_changes() {
        let mut vfs = Vfs::default();
        let file = path("/workspace/Test.sol");

        assert!(insert(&mut vfs, "/workspace/Test.sol", "contract Test {}", 1));
        let revision = vfs.content_revision();

        assert!(!insert(&mut vfs, "/workspace/Test.sol", "contract Test {}", 2));
        assert_eq!(vfs.content_revision(), revision);
        assert_eq!(vfs.get_file_version(&file), Some(2));

        assert!(insert(&mut vfs, "/workspace/Test.sol", "contract Changed {}", 3));
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
    fn derived_sources_are_cached_until_contents_change() {
        let mut vfs = Vfs::default();
        let file = path("/workspace/Test.sol");
        let moved = path("/workspace/Moved.sol");
        insert(&mut vfs, "/workspace/Test.sol", "contract Old {}", 1);
        let handles = |vfs: &Vfs, file: &VfsPath| {
            let analysis = vfs.get_file_analysis_source(file).unwrap();
            let selection = vfs.get_file_selection_range_source(file).unwrap();
            let folding = vfs.get_file_folding_range_source(file).unwrap();
            assert!(Arc::ptr_eq(&selection.0, &folding.0));
            (analysis, selection, folding)
        };
        let (analysis, selection, folding) = handles(&vfs, &file);
        assert!(Arc::ptr_eq(&analysis, &vfs.get_file_analysis_source(&file).unwrap()));

        assert!(!insert(&mut vfs, "/workspace/Test.sol", "contract Old {}", 2));
        vfs.rename_file_prefixes(&moves(&[("Test.sol", "Moved.sol")])).unwrap();
        assert_eq!(vfs.get_file_analysis_source(&file), None);
        assert!(vfs.get_file_selection_range_source(&file).is_none());
        assert!(vfs.get_file_folding_range_source(&file).is_none());
        let (cached_analysis, cached_selection, _) = handles(&vfs, &moved);
        assert!(Arc::ptr_eq(&analysis, &cached_analysis));
        assert!(Arc::ptr_eq(&selection.0, &cached_selection.0));
        assert_eq!(vfs.get_file_version(&moved), Some(2));

        assert!(insert(&mut vfs, "/workspace/Moved.sol", "contract NewName {}", 3));
        let (changed_analysis, changed_selection, changed_folding) = handles(&vfs, &moved);
        assert_eq!(changed_analysis.as_str(), "contract NewName {}");
        assert!(!Arc::ptr_eq(&analysis, &changed_analysis));
        assert!(!Arc::ptr_eq(&folding.0, &changed_folding.0));
        // The replaced file's handle lazily builds its index from its own contents.
        assert!(!std::ptr::eq(selection.index(), changed_selection.index()));
        let end = |source: &SelectionRangeSource| {
            source.selection_ranges(&[Position::new(0, 9)]).unwrap()[0].range.end.character
        };
        assert_eq!(end(&selection), 12);
        assert_eq!(end(&changed_selection), 16);
    }

    #[test]
    fn document_sources_keep_text_and_positions_from_the_same_contents() {
        let mut vfs = Vfs::default();
        let file = path("/workspace/Test.sol");
        insert(&mut vfs, "/workspace/Test.sol", "α😀\r\nnext\rtail\n", 1);
        let original = vfs.get_file_source(&file).unwrap();
        let at = |source: &DocumentSource, line, character| {
            let position = Position::new(line, character);
            source.positions().checked_text_range(lsp_types::Range::new(position, position))
        };
        assert_eq!(at(&original, 0, 2), None);
        assert_eq!(at(&original, 0, 99), Some(6..6));
        assert_eq!(at(&original, 1, 2), Some(10..10));
        assert_eq!(at(&original, 2, 4), Some(17..17));
        assert_eq!(at(&original, 3, 0), Some(18..18));
        assert_eq!(at(&original, 4, 0), None);

        assert!(!vfs.set_file_contents_with_version(
            file.clone(),
            Some(original.contents().clone()),
            Some(2),
        ));
        let unchanged = vfs.get_file_source(&file).unwrap();
        assert!(std::ptr::eq(original.positions(), unchanged.positions()));
        assert!(Arc::ptr_eq(&original.source(), &unchanged.source()));

        insert(&mut vfs, "/workspace/Test.sol", "x\n😀z\n", 3);
        let changed = vfs.get_file_source(&file).unwrap();
        assert_eq!(at(&changed, 1, 2), Some(6..6));
        assert_eq!(at(&changed, 2, 0), Some(8..8));
        assert_eq!(changed.source().as_str(), "x\n😀z\n");
        assert_eq!(at(&original, 1, 2), Some(10..10));
        assert_eq!(original.source().as_str(), "α😀\r\nnext\rtail\n");

        let moved = path("/workspace/Moved.sol");
        vfs.rename_file_prefixes(&moves(&[("Test.sol", "Moved.sol")])).unwrap();
        assert!(vfs.get_file_source(&file).is_none());
        let renamed = vfs.get_file_source(&moved).unwrap();
        assert!(std::ptr::eq(changed.positions(), renamed.positions()));
        assert_eq!(at(&renamed, 1, 2), Some(6..6));
        vfs.set_file_contents(moved, None);
        assert_eq!(renamed.source().as_str(), "x\n😀z\n");
    }

    #[test]
    fn statement_boundaries_follow_source_snapshots() {
        let mut vfs = Vfs::default();
        let file = path("/workspace/Test.sol");
        let original = "start(); target(\";\", 1); // ;\nnext(2, 3);";
        insert(&mut vfs, "/workspace/Test.sol", original, 1);
        let source = vfs.get_file_source(&file).unwrap();
        let first = original.find(';').unwrap();
        let second = original.find("; //").unwrap();
        let earlier = original.find('1').unwrap();
        let later = original.find('3').unwrap();
        for cursor in [later, earlier, later] {
            assert_eq!(
                source.statement_boundary(cursor),
                if cursor == earlier { first } else { second }
            );
        }

        insert(&mut vfs, "/workspace/Test.sol", original, 2);
        let unchanged = vfs.get_file_source(&file).unwrap();
        assert!(Arc::ptr_eq(&source.0, &unchanged.0));
        assert_eq!(unchanged.statement_boundary(earlier), first);

        let edited = "start(); /* target(\";\", 1); */ next(2, 3);";
        insert(&mut vfs, "/workspace/Test.sol", edited, 3);
        let changed = vfs.get_file_source(&file).unwrap();
        assert_eq!(changed.statement_boundary(edited.find('3').unwrap()), first);
        assert_eq!(source.statement_boundary(later), second);
        vfs.set_file_contents(file, None);
        assert_eq!(changed.statement_boundary(edited.find('3').unwrap()), first);
        assert_eq!(source.statement_boundary(earlier), first);
    }

    #[test]
    fn rename_file_prefixes_uses_one_snapshot_and_preserves_versions() {
        for (initial, renames, renamed) in [
            (
                vec![("A.sol", "contract A {}", 1), ("B.sol", "contract B {}", 2)],
                vec![("A.sol", "B.sol"), ("B.sol", "C.sol")],
                vec![("B.sol", "contract A {}", 1), ("C.sol", "contract B {}", 2)],
            ),
            // An unmoved destination buffer wins over the moved file.
            (
                vec![("New.sol", "contract UnsavedNew {}", 7), ("Old.sol", "contract Old {}", 1)],
                vec![("Old.sol", "New.sol")],
                vec![("New.sol", "contract UnsavedNew {}", 7)],
            ),
            (
                vec![("pkg/Nested.sol", "contract Nested {}", 3), ("pkg2/Keep.sol", "", 4)],
                vec![("pkg", "moved")],
                vec![("moved/Nested.sol", "contract Nested {}", 3), ("pkg2/Keep.sol", "", 4)],
            ),
            (
                vec![("pkg/nested/Test.sol", "contract Test {}", 5)],
                vec![("pkg", "moved"), ("pkg/nested", "special")],
                vec![("special/Test.sol", "contract Test {}", 5)],
            ),
        ] {
            let mut vfs = workspace_vfs(&initial);
            vfs.rename_file_prefixes(&moves(&renames)).unwrap();
            assert_eq!(files(&vfs), expected(&renamed), "{renames:?}");
        }
    }

    #[test]
    fn rename_file_prefixes_rejects_expanded_destination_collision_atomically() {
        let initial = [("A/x.sol", "contract A {}", 1), ("B/x.sol", "contract B {}", 2)];
        let mut vfs = workspace_vfs(&initial);
        let revision = vfs.content_revision();

        let error = vfs
            .rename_file_prefixes(&moves(&[("A", "out"), ("B/x.sol", "out/x.sol")]))
            .unwrap_err();

        assert!(matches!(
            error,
            FileMoveError::ConflictingDestination { new_path, .. }
                if new_path == Path::new("/workspace/out/x.sol")
        ));
        assert_eq!(files(&vfs), expected(&initial));
        assert_eq!(vfs.content_revision(), revision);
    }

    #[test]
    fn remove_file_prefixes_removes_descendants_without_matching_sibling_prefixes() {
        let mut vfs = workspace_vfs(&[("pkg/Nested.sol", "", 3), ("pkg2/Keep.sol", "", 4)]);

        vfs.remove_file_prefixes(&[PathBuf::from("/workspace/pkg")]);

        assert_eq!(files(&vfs), expected(&[("pkg2/Keep.sol", "", 4)]));
        assert_eq!(vfs.get_file_version(&path("/workspace/pkg/Nested.sol")), None);
    }
}
