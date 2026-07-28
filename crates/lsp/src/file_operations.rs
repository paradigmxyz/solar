//! Shared normalization and matching for workspace file moves.

use lsp_types::{RenameFilesParams, Url};
use normalize_path::NormalizePath;
use std::{
    error::Error,
    fmt,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FileMoveId(usize);

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct FileMoveBatch {
    moves: Vec<FileMove>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FileMove {
    old_path: PathBuf,
    new_path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum FileMoveError {
    ConflictingSource { old_path: PathBuf, first_new_path: PathBuf, second_new_path: PathBuf },
    ConflictingDestination { new_path: PathBuf, first_old_path: PathBuf, second_old_path: PathBuf },
}

impl FileMoveBatch {
    pub(crate) fn new(
        moves: impl IntoIterator<Item = (PathBuf, PathBuf)>,
    ) -> Result<Self, FileMoveError> {
        let mut moves = moves
            .into_iter()
            .map(|(old_path, new_path)| FileMove {
                old_path: old_path.normalize(),
                new_path: new_path.normalize(),
            })
            .collect::<Vec<_>>();
        moves.sort_unstable_by(|lhs, rhs| {
            (&lhs.old_path, &lhs.new_path).cmp(&(&rhs.old_path, &rhs.new_path))
        });
        moves.dedup();

        for pair in moves.windows(2) {
            if pair[0].old_path == pair[1].old_path {
                return Err(FileMoveError::ConflictingSource {
                    old_path: pair[0].old_path.clone(),
                    first_new_path: pair[0].new_path.clone(),
                    second_new_path: pair[1].new_path.clone(),
                });
            }
        }

        moves.sort_unstable_by(|lhs, rhs| {
            (&lhs.new_path, &lhs.old_path).cmp(&(&rhs.new_path, &rhs.old_path))
        });
        for pair in moves.windows(2) {
            if pair[0].new_path == pair[1].new_path {
                return Err(FileMoveError::ConflictingDestination {
                    new_path: pair[0].new_path.clone(),
                    first_old_path: pair[0].old_path.clone(),
                    second_old_path: pair[1].old_path.clone(),
                });
            }
        }

        moves.sort_unstable_by(|lhs, rhs| {
            rhs.old_path
                .components()
                .count()
                .cmp(&lhs.old_path.components().count())
                .then_with(|| lhs.old_path.cmp(&rhs.old_path))
        });
        Ok(Self { moves })
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.moves.is_empty()
    }

    pub(crate) fn map_path(&self, path: &Path) -> Option<(FileMoveId, PathBuf)> {
        let path = path.normalize();
        self.moves.iter().enumerate().find_map(|(index, file_move)| {
            let suffix = path.strip_prefix(&file_move.old_path).ok()?;
            Some((FileMoveId(index), file_move.new_path.join(suffix)))
        })
    }

    pub(crate) fn old_paths(&self) -> impl Iterator<Item = &Path> {
        self.moves.iter().map(|file_move| file_move.old_path.as_path())
    }

    pub(crate) fn new_paths(&self) -> impl Iterator<Item = &Path> {
        self.moves.iter().map(|file_move| file_move.new_path.as_path())
    }
}

impl TryFrom<RenameFilesParams> for FileMoveBatch {
    type Error = FileMoveError;

    fn try_from(params: RenameFilesParams) -> Result<Self, Self::Error> {
        let moves = params.files.into_iter().filter_map(|file| {
            Some((parse_file_uri(&file.old_uri)?, parse_file_uri(&file.new_uri)?))
        });
        Self::new(moves)
    }
}

pub(crate) fn parse_file_uri(uri: &str) -> Option<PathBuf> {
    let uri = Url::parse(uri).ok()?;
    file_path_from_url(&uri)
}

pub(crate) fn file_path_from_url(uri: &Url) -> Option<PathBuf> {
    (uri.scheme() == "file").then(|| uri.to_file_path().ok()).flatten()
}

impl fmt::Display for FileMoveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ConflictingSource { old_path, first_new_path, second_new_path } => write!(
                f,
                "rename source `{}` has conflicting destinations `{}` and `{}`",
                old_path.display(),
                first_new_path.display(),
                second_new_path.display()
            ),
            Self::ConflictingDestination { new_path, first_old_path, second_old_path } => write!(
                f,
                "rename destination `{}` has conflicting sources `{}` and `{}`",
                new_path.display(),
                first_old_path.display(),
                second_old_path.display()
            ),
        }
    }
}

impl Error for FileMoveError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(path: &str) -> PathBuf {
        PathBuf::from(path)
    }

    #[test]
    fn exact_normalized_duplicates_are_harmless() {
        let batch = FileMoveBatch::new([
            (path("/workspace/src/../src"), path("/workspace/moved/.")),
            (path("/workspace/src"), path("/workspace/moved")),
        ])
        .unwrap();

        assert_eq!(batch.old_paths().count(), 1);
        assert_eq!(
            batch.map_path(Path::new("/workspace/src/Test.sol")).unwrap().1,
            path("/workspace/moved/Test.sol")
        );
    }

    #[test]
    fn conflicting_normalized_sources_are_rejected() {
        let error = FileMoveBatch::new([
            (path("/workspace/src/../src"), path("/workspace/first")),
            (path("/workspace/src"), path("/workspace/second")),
        ])
        .unwrap_err();

        assert!(matches!(
            error,
            FileMoveError::ConflictingSource { old_path, .. }
                if old_path == path("/workspace/src")
        ));
    }

    #[test]
    fn conflicting_normalized_destinations_are_rejected() {
        let error = FileMoveBatch::new([
            (path("/workspace/first"), path("/workspace/out/../shared")),
            (path("/workspace/second"), path("/workspace/shared")),
        ])
        .unwrap_err();

        assert!(matches!(
            error,
            FileMoveError::ConflictingDestination { new_path, .. }
                if new_path == path("/workspace/shared")
        ));
    }

    #[test]
    fn matching_prefers_the_most_specific_source_and_not_prefix_siblings() {
        let batch = FileMoveBatch::new([
            (path("/workspace/pkg/nested"), path("/workspace/special")),
            (path("/workspace/pkg"), path("/workspace/moved")),
        ])
        .unwrap();

        assert_eq!(
            batch.map_path(Path::new("/workspace/pkg/nested/Test.sol")).unwrap().1,
            path("/workspace/special/Test.sol")
        );
        assert!(batch.map_path(Path::new("/workspace/pkg2/Test.sol")).is_none());
    }

    #[test]
    fn mapping_uses_one_snapshot_without_chaining() {
        let batch = FileMoveBatch::new([
            (path("/workspace/A"), path("/workspace/B")),
            (path("/workspace/B"), path("/workspace/C")),
        ])
        .unwrap();

        assert_eq!(
            batch.map_path(Path::new("/workspace/A/Test.sol")).unwrap().1,
            path("/workspace/B/Test.sol")
        );
    }

    #[test]
    fn non_file_uris_are_ignored() {
        let file_uri = Url::from_file_path(std::env::temp_dir().join("Old.sol")).unwrap();
        let old_uri = Url::parse(&file_uri.as_str().replacen("file:", "untitled:", 1)).unwrap();
        let new_uri = Url::from_file_path(std::env::temp_dir().join("New.sol")).unwrap();
        let batch = FileMoveBatch::try_from(RenameFilesParams {
            files: vec![lsp_types::FileRename {
                old_uri: old_uri.to_string(),
                new_uri: new_uri.to_string(),
            }],
        })
        .unwrap();

        assert!(batch.is_empty());
    }
}
