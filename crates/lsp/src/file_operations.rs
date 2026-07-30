//! Shared normalization and matching for workspace file moves.

use lsp_types::{FileChangeType, RenameFilesParams, Url};
use normalize_path::NormalizePath;
use std::{
    error::Error,
    fmt,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering as AtomicOrdering},
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FileMoveId(usize);

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct FileMoveBatch {
    moves: Vec<FileMove>,
}

#[derive(Debug, Default)]
pub(crate) struct FileOperationCoordinator {
    renames: Vec<RenameTransaction>,
    direct_events: Vec<DirectFileEventTransaction>,
    watched_events: Vec<DirectFileEventTransaction>,
}

#[derive(Debug)]
struct RenameTransaction {
    moves: FileMoveBatch,
    activation: Option<Arc<AtomicU8>>,
    watcher: RenameWatcherEvidence,
    state: RenameTransactionState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RenameTransactionState {
    Prepared,
    Applied,
}

#[derive(Debug)]
pub(crate) struct RenamePreparation {
    activation: Arc<AtomicU8>,
    activated: bool,
}

#[derive(Debug)]
struct RenameWatcherEvidence {
    paths: Vec<RenameWatcherPath>,
    moves: FileMoveBatch,
}

#[derive(Debug)]
struct RenameWatcherPath {
    move_id: FileMoveId,
    old_path: PathBuf,
    new_path: PathBuf,
    deleted: bool,
    created: bool,
}

#[derive(Debug)]
struct DirectFileEventTransaction {
    typ: FileChangeType,
    paths: Vec<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum WatchedFileAction {
    ApplyRenames(Vec<FileMoveBatch>),
    Ignore,
    Process,
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
            let new_path = if suffix.as_os_str().is_empty() {
                file_move.new_path.clone()
            } else {
                file_move.new_path.join(suffix)
            };
            Some((FileMoveId(index), new_path))
        })
    }

    pub(crate) fn reverse_map_path(&self, path: &Path) -> Option<(FileMoveId, PathBuf)> {
        let path = path.normalize();
        self.moves
            .iter()
            .enumerate()
            .filter_map(|(index, file_move)| {
                let suffix = path.strip_prefix(&file_move.new_path).ok()?;
                let old_path = if suffix.as_os_str().is_empty() {
                    file_move.old_path.clone()
                } else {
                    file_move.old_path.join(suffix)
                };
                Some((file_move.new_path.components().count(), FileMoveId(index), old_path))
            })
            .max_by_key(|(depth, _, _)| *depth)
            .map(|(_, move_id, path)| (move_id, path))
    }

    pub(crate) fn old_paths(&self) -> impl Iterator<Item = &Path> {
        self.moves.iter().map(|file_move| file_move.old_path.as_path())
    }

    pub(crate) fn new_paths(&self) -> impl Iterator<Item = &Path> {
        self.moves.iter().map(|file_move| file_move.new_path.as_path())
    }

    pub(crate) fn validate_mapped_destinations(
        &self,
        paths: impl IntoIterator<Item = PathBuf>,
    ) -> Result<(), FileMoveError> {
        let mut mapped = paths
            .into_iter()
            .filter_map(|old_path| {
                let (_, new_path) = self.map_path(&old_path)?;
                Some((new_path, old_path.normalize()))
            })
            .collect::<Vec<_>>();
        mapped.sort_unstable();
        mapped.dedup();

        for pair in mapped.windows(2) {
            if pair[0].0 == pair[1].0 {
                return Err(FileMoveError::ConflictingDestination {
                    new_path: pair[0].0.clone(),
                    first_old_path: pair[0].1.clone(),
                    second_old_path: pair[1].1.clone(),
                });
            }
        }
        Ok(())
    }

    fn invalidates_replay_of(&self, previous: &Self) -> bool {
        self.moves.iter().any(|current| {
            previous.moves.iter().any(|previous| {
                paths_overlap(&current.old_path, &previous.new_path)
                    || paths_overlap(&current.new_path, &previous.old_path)
            })
        })
    }

    fn watcher_evidence(&self) -> RenameWatcherEvidence {
        let paths = self
            .moves
            .iter()
            .enumerate()
            .map(|(index, file_move)| RenameWatcherPath {
                move_id: FileMoveId(index),
                old_path: file_move.old_path.clone(),
                new_path: file_move.new_path.clone(),
                deleted: false,
                created: false,
            })
            .collect();
        RenameWatcherEvidence { paths, moves: self.clone() }
    }
}

fn paths_overlap(lhs: &Path, rhs: &Path) -> bool {
    lhs.starts_with(rhs) || rhs.starts_with(lhs)
}

impl FileOperationCoordinator {
    pub(crate) fn prepare_rename(&mut self, moves: FileMoveBatch) -> RenamePreparation {
        self.clear_cancelled_renames();
        let activation = Arc::new(AtomicU8::new(RENAME_PREPARING));
        self.renames.push(RenameTransaction {
            watcher: moves.watcher_evidence(),
            moves,
            activation: Some(activation.clone()),
            state: RenameTransactionState::Prepared,
        });
        RenamePreparation { activation, activated: false }
    }

    /// Returns whether this batch still needs to be applied.
    pub(crate) fn apply_rename(&mut self, moves: &FileMoveBatch) -> bool {
        self.clear_cancelled_renames();
        if let Some(index) = self.renames.iter().rposition(|transaction| {
            transaction.moves == *moves && transaction.state == RenameTransactionState::Prepared
        }) {
            self.claim_prepared_rename(index);
            self.invalidate_related_replay_guards(moves);
            return true;
        }
        if self.renames.iter().any(|transaction| {
            transaction.moves == *moves && transaction.state == RenameTransactionState::Applied
        }) {
            return false;
        }

        self.invalidate_related_replay_guards(moves);
        self.push_rename(RenameTransaction {
            watcher: moves.watcher_evidence(),
            moves: moves.clone(),
            activation: None,
            state: RenameTransactionState::Applied,
        });
        true
    }

    pub(crate) fn observe_watcher_event(
        &mut self,
        path: &Path,
        typ: FileChangeType,
    ) -> WatchedFileAction {
        self.clear_cancelled_renames();
        if self.observe_direct_event(path, typ) {
            return WatchedFileAction::Ignore;
        }
        let mut apply = Vec::new();
        let mut matched = false;
        self.renames.retain_mut(|transaction| {
            if transaction
                .activation
                .as_ref()
                .is_some_and(|activation| activation.load(AtomicOrdering::Acquire) != RENAME_ACTIVE)
            {
                return true;
            }
            if transaction.watcher.observe(path, typ) {
                matched = true;
                if transaction.state == RenameTransactionState::Prepared
                    && transaction.watcher.is_complete()
                {
                    apply.push(transaction.moves.clone());
                }
                return true;
            }
            transaction.state != RenameTransactionState::Applied
                || !transaction.watcher.is_opposite(path, typ)
        });
        if !apply.is_empty() {
            WatchedFileAction::ApplyRenames(apply)
        } else if matched {
            WatchedFileAction::Ignore
        } else {
            WatchedFileAction::Process
        }
    }

    pub(crate) fn claim_watched_rename(&mut self, moves: &FileMoveBatch) -> bool {
        self.clear_cancelled_renames();
        let Some(index) = self.renames.iter().rposition(|transaction| {
            transaction.moves == *moves
                && transaction.state == RenameTransactionState::Prepared
                && transaction.preparation_is_active()
                && transaction.watcher.is_complete()
        }) else {
            return false;
        };
        self.claim_prepared_rename(index);
        self.invalidate_related_replay_guards(moves);
        true
    }

    pub(crate) fn record_direct_events(
        &mut self,
        typ: FileChangeType,
        paths: impl IntoIterator<Item = PathBuf>,
    ) {
        debug_assert!(matches!(typ, FileChangeType::CREATED | FileChangeType::DELETED));
        let mut paths = paths.into_iter().map(|path| path.normalize()).collect::<Vec<_>>();
        paths.sort_unstable();
        paths.dedup();
        if paths.is_empty() {
            return;
        }

        self.renames.retain(|transaction| {
            transaction.state != RenameTransactionState::Applied
                || !paths.iter().any(|path| transaction.watcher.is_opposite(path, typ))
        });
        for transaction in &mut self.direct_events {
            if transaction.typ != typ {
                transaction.paths.retain(|path| paths.binary_search(path).is_err());
            }
        }
        for transaction in &mut self.watched_events {
            if transaction.typ != typ {
                transaction.paths.retain(|path| paths.binary_search(path).is_err());
            }
        }
        self.direct_events.retain(|transaction| !transaction.paths.is_empty());
        self.watched_events.retain(|transaction| !transaction.paths.is_empty());
        self.direct_events.push(DirectFileEventTransaction { typ, paths });
        if self.direct_events.len() > DIRECT_EVENT_HISTORY_LIMIT {
            self.direct_events.remove(0);
        }
    }

    pub(crate) fn record_watched_events(
        &mut self,
        typ: FileChangeType,
        paths: impl IntoIterator<Item = PathBuf>,
    ) {
        debug_assert!(matches!(typ, FileChangeType::CREATED | FileChangeType::DELETED));
        let mut paths = paths.into_iter().map(|path| path.normalize()).collect::<Vec<_>>();
        paths.sort_unstable();
        paths.dedup();
        if paths.is_empty() {
            return;
        }

        for transaction in &mut self.watched_events {
            if transaction.typ != typ {
                transaction.paths.retain(|path| paths.binary_search(path).is_err());
            }
        }
        self.watched_events.retain(|transaction| !transaction.paths.is_empty());
        if let Some(transaction) = self.watched_events.last_mut()
            && transaction.typ == typ
        {
            transaction.paths.extend(paths);
            transaction.paths.sort_unstable();
            transaction.paths.dedup();
            return;
        }
        self.watched_events.push(DirectFileEventTransaction { typ, paths });
        if self.watched_events.len() > DIRECT_EVENT_HISTORY_LIMIT {
            self.watched_events.remove(0);
        }
    }

    pub(crate) fn consume_watched_events(
        &mut self,
        typ: FileChangeType,
        paths: &[PathBuf],
    ) -> bool {
        let mut paths = paths.iter().map(|path| path.normalize()).collect::<Vec<_>>();
        paths.sort_unstable();
        paths.dedup();
        let all_observed = !paths.is_empty()
            && paths.iter().all(|path| {
                self.watched_events.iter().any(|transaction| {
                    transaction.typ == typ && transaction.paths.binary_search(path).is_ok()
                })
            });
        for transaction in &mut self.watched_events {
            if transaction.typ == typ {
                transaction.paths.retain(|path| paths.binary_search(path).is_err());
            }
        }
        self.watched_events.retain(|transaction| !transaction.paths.is_empty());
        all_observed
    }

    pub(crate) fn watched_event_paths_under(
        &self,
        typ: FileChangeType,
        roots: &[PathBuf],
    ) -> Vec<PathBuf> {
        let mut paths = self
            .watched_events
            .iter()
            .filter(|transaction| transaction.typ == typ)
            .flat_map(|transaction| &transaction.paths)
            .filter(|path| roots.iter().any(|root| path.starts_with(root)))
            .cloned()
            .collect::<Vec<_>>();
        paths.sort_unstable();
        paths.dedup();
        paths
    }

    fn observe_direct_event(&mut self, path: &Path, typ: FileChangeType) -> bool {
        let path = path.normalize();
        let mut matched = false;
        for transaction in &mut self.direct_events {
            let Ok(index) = transaction.paths.binary_search(&path) else { continue };
            if transaction.typ == typ {
                matched = true;
            } else {
                transaction.paths.remove(index);
            }
        }
        self.direct_events.retain(|transaction| !transaction.paths.is_empty());
        matched
    }

    fn clear_cancelled_renames(&mut self) {
        self.renames.retain(|transaction| {
            !transaction.activation.as_ref().is_some_and(|activation| {
                activation.load(AtomicOrdering::Acquire) == RENAME_CANCELLED
            })
        });

        let mut index = 0;
        while index < self.renames.len() {
            let transaction = &self.renames[index];
            let superseded = self.renames[index + 1..]
                .iter()
                .any(|newer| newer.moves == transaction.moves && newer.preparation_is_active());
            if superseded {
                self.renames.remove(index);
            } else {
                index += 1;
            }
        }
        self.trim_rename_history();
    }

    fn claim_prepared_rename(&mut self, index: usize) {
        let mut transaction = self.renames.remove(index);
        self.renames.retain(|other| other.moves != transaction.moves);
        transaction.activation = None;
        transaction.state = RenameTransactionState::Applied;
        self.renames.push(transaction);
        self.trim_rename_history();
    }

    fn invalidate_related_replay_guards(&mut self, moves: &FileMoveBatch) {
        self.renames.retain(|transaction| {
            transaction.state != RenameTransactionState::Applied
                || transaction.moves == *moves
                || !moves.invalidates_replay_of(&transaction.moves)
        });
    }

    fn push_rename(&mut self, transaction: RenameTransaction) {
        self.renames.push(transaction);
        self.trim_rename_history();
    }

    fn trim_rename_history(&mut self) {
        while self.renames.iter().filter(|transaction| transaction.occupies_history_slot()).count()
            > RENAME_HISTORY_LIMIT
        {
            let Some(index) =
                self.renames.iter().position(RenameTransaction::occupies_history_slot)
            else {
                break;
            };
            self.renames.remove(index);
        }
    }
}

impl RenameTransaction {
    fn preparation_is_active(&self) -> bool {
        self.activation
            .as_ref()
            .is_some_and(|activation| activation.load(AtomicOrdering::Acquire) == RENAME_ACTIVE)
    }

    fn occupies_history_slot(&self) -> bool {
        self.activation.is_none() || self.preparation_is_active()
    }
}

impl RenamePreparation {
    pub(crate) fn activate(mut self) {
        self.activation.store(RENAME_ACTIVE, AtomicOrdering::Release);
        self.activated = true;
    }
}

impl Drop for RenamePreparation {
    fn drop(&mut self) {
        if !self.activated {
            self.activation.store(RENAME_CANCELLED, AtomicOrdering::Release);
        }
    }
}

impl RenameWatcherEvidence {
    fn observe(&mut self, path: &Path, typ: FileChangeType) -> bool {
        let path = path.normalize();
        let mut matched = false;
        for watcher_path in &mut self.paths {
            if typ == FileChangeType::DELETED && path == watcher_path.old_path {
                watcher_path.deleted = true;
                matched = true;
            } else if typ == FileChangeType::CREATED && path == watcher_path.new_path {
                watcher_path.created = true;
                matched = true;
            }
        }
        if matched {
            return true;
        }

        let mapped = if typ == FileChangeType::DELETED {
            self.moves.map_path(&path).map(|(move_id, new_path)| (move_id, path, new_path))
        } else if typ == FileChangeType::CREATED {
            self.moves.reverse_map_path(&path).map(|(move_id, old_path)| (move_id, old_path, path))
        } else {
            None
        };
        let Some((move_id, old_path, new_path)) = mapped else { return false };
        self.paths.push(RenameWatcherPath {
            move_id,
            old_path,
            new_path,
            deleted: typ == FileChangeType::DELETED,
            created: typ == FileChangeType::CREATED,
        });
        true
    }

    fn is_complete(&self) -> bool {
        (0..self.moves.moves.len()).all(|index| {
            self.paths
                .iter()
                .any(|path| path.move_id == FileMoveId(index) && path.deleted && path.created)
        })
    }

    fn is_opposite(&self, path: &Path, typ: FileChangeType) -> bool {
        let path = path.normalize();
        self.paths.iter().any(|watcher_path| {
            (path == watcher_path.old_path && typ == FileChangeType::CREATED)
                || (path == watcher_path.new_path && typ == FileChangeType::DELETED)
                || (typ == FileChangeType::CHANGED
                    && (path == watcher_path.old_path || path == watcher_path.new_path))
        }) || (typ == FileChangeType::CREATED && self.moves.map_path(&path).is_some())
            || (typ == FileChangeType::DELETED && self.moves.reverse_map_path(&path).is_some())
            || (typ == FileChangeType::CHANGED
                && (self.moves.map_path(&path).is_some()
                    || self.moves.reverse_map_path(&path).is_some()))
    }
}

const RENAME_HISTORY_LIMIT: usize = 16;
const DIRECT_EVENT_HISTORY_LIMIT: usize = 16;
const RENAME_PREPARING: u8 = 0;
const RENAME_ACTIVE: u8 = 1;
const RENAME_CANCELLED: u8 = 2;

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
    fn exact_file_mapping_preserves_destination_paths() {
        let old = path("/workspace/src/Target.sol").normalize();
        let new = path("/workspace/moved/Renamed.sol").normalize();
        let batch = FileMoveBatch::new([(old.clone(), new.clone())]).unwrap();

        let mapped = batch.map_path(&old).unwrap().1;
        let reversed = batch.reverse_map_path(&new).unwrap().1;

        assert_eq!(mapped.as_os_str(), new.as_os_str());
        assert_eq!(reversed.as_os_str(), old.as_os_str());
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
    fn reverse_mapping_prefers_the_most_specific_destination() {
        let batch = FileMoveBatch::new([
            (path("/workspace/A"), path("/workspace/out")),
            (path("/workspace/B"), path("/workspace/out/nested")),
        ])
        .unwrap();

        assert_eq!(
            batch.reverse_map_path(Path::new("/workspace/out/nested/Test.sol")).unwrap().1,
            path("/workspace/B/Test.sol")
        );
        assert!(batch.reverse_map_path(Path::new("/workspace/output/Test.sol")).is_none());
    }

    #[test]
    fn new_prepare_allows_same_rename_payload_again() {
        let batch = FileMoveBatch::new([(path("/workspace/A"), path("/workspace/B"))]).unwrap();
        let mut coordinator = FileOperationCoordinator::default();

        coordinator.prepare_rename(batch.clone()).activate();
        assert!(coordinator.apply_rename(&batch));
        assert!(!coordinator.apply_rename(&batch));

        coordinator.prepare_rename(batch.clone()).activate();
        assert!(coordinator.apply_rename(&batch));
    }

    #[test]
    fn cancelled_same_payload_prepare_preserves_replay_guard() {
        let batch = FileMoveBatch::new([(path("/workspace/A"), path("/workspace/B"))]).unwrap();
        let mut coordinator = FileOperationCoordinator::default();

        assert!(coordinator.apply_rename(&batch));
        assert!(!coordinator.apply_rename(&batch));

        drop(coordinator.prepare_rename(batch.clone()));

        assert!(!coordinator.apply_rename(&batch));
    }

    #[test]
    fn cancelled_prepare_does_not_consume_replay_history() {
        let guarded = FileMoveBatch::new([(path("/workspace/A"), path("/workspace/B"))]).unwrap();
        let mut coordinator = FileOperationCoordinator::default();

        assert!(coordinator.apply_rename(&guarded));
        for index in 0..RENAME_HISTORY_LIMIT - 1 {
            let batch = FileMoveBatch::new([(
                path(&format!("/workspace/Old{index}")),
                path(&format!("/workspace/New{index}")),
            )])
            .unwrap();
            assert!(coordinator.apply_rename(&batch));
        }
        assert!(!coordinator.apply_rename(&guarded));

        drop(coordinator.prepare_rename(guarded.clone()));

        assert!(!coordinator.apply_rename(&guarded));
    }

    #[test]
    fn latest_activated_same_payload_prepare_replaces_earlier_lifecycle() {
        let batch = FileMoveBatch::new([(path("/workspace/A"), path("/workspace/B"))]).unwrap();
        let mut coordinator = FileOperationCoordinator::default();

        coordinator.prepare_rename(batch.clone()).activate();
        coordinator.prepare_rename(batch.clone()).activate();

        assert!(coordinator.apply_rename(&batch));
        assert!(!coordinator.apply_rename(&batch));
    }

    #[test]
    fn did_rename_claims_pending_preparation_before_cancellation() {
        let batch = FileMoveBatch::new([(path("/workspace/A"), path("/workspace/B"))]).unwrap();
        let mut coordinator = FileOperationCoordinator::default();

        let preparation = coordinator.prepare_rename(batch.clone());
        assert!(coordinator.apply_rename(&batch));
        drop(preparation);

        assert!(!coordinator.apply_rename(&batch));
    }

    #[test]
    fn did_reuses_prepared_lifecycle_without_external_evidence() {
        let prepared = FileMoveBatch::new([(path("/workspace/A"), path("/workspace/B"))]).unwrap();
        let did_only = FileMoveBatch::new([(path("/workspace/X"), path("/workspace/Y"))]).unwrap();
        let mut coordinator = FileOperationCoordinator::default();

        coordinator.prepare_rename(prepared.clone()).activate();
        assert!(coordinator.apply_rename(&prepared));
        assert!(!coordinator.apply_rename(&prepared));
        assert!(coordinator.apply_rename(&did_only));
    }

    #[test]
    fn did_rename_claims_one_of_multiple_pending_same_payload_preparations() {
        let batch = FileMoveBatch::new([(path("/workspace/A"), path("/workspace/B"))]).unwrap();
        let mut coordinator = FileOperationCoordinator::default();

        let earlier = coordinator.prepare_rename(batch.clone());
        let later = coordinator.prepare_rename(batch.clone());
        assert!(coordinator.apply_rename(&batch));
        earlier.activate();
        drop(later);

        assert!(!coordinator.apply_rename(&batch));
    }

    #[test]
    fn watcher_claims_one_of_multiple_pending_same_payload_preparations() {
        let old_path = path("/workspace/A.sol");
        let new_path = path("/workspace/B.sol");
        let batch = FileMoveBatch::new([(old_path.clone(), new_path.clone())]).unwrap();
        let mut coordinator = FileOperationCoordinator::default();

        coordinator.prepare_rename(batch.clone()).activate();
        let later = coordinator.prepare_rename(batch.clone());
        assert_eq!(
            coordinator.observe_watcher_event(&old_path, FileChangeType::DELETED),
            WatchedFileAction::Ignore
        );
        assert_eq!(
            coordinator.observe_watcher_event(&new_path, FileChangeType::CREATED),
            WatchedFileAction::ApplyRenames(vec![batch.clone()])
        );
        assert!(coordinator.claim_watched_rename(&batch));
        later.activate();

        assert!(!coordinator.apply_rename(&batch));
    }

    #[test]
    fn opposite_watcher_activity_ends_applied_rename_lifecycle() {
        let old_path = path("/workspace/A.sol");
        let new_path = path("/workspace/B.sol");
        let batch = FileMoveBatch::new([(old_path.clone(), new_path)]).unwrap();
        let mut coordinator = FileOperationCoordinator::default();

        assert!(coordinator.apply_rename(&batch));
        assert!(!coordinator.apply_rename(&batch));
        assert_eq!(
            coordinator.observe_watcher_event(&old_path, FileChangeType::CREATED),
            WatchedFileAction::Process
        );
        assert!(coordinator.apply_rename(&batch));
    }

    #[test]
    fn invalidated_applied_rename_does_not_swallow_destination_create() {
        let a = path("/workspace/A");
        let b = path("/workspace/B");
        let c = path("/workspace/C");
        let forward = FileMoveBatch::new([(a, b.clone())]).unwrap();
        let next = FileMoveBatch::new([(b.clone(), c)]).unwrap();
        let mut coordinator = FileOperationCoordinator::default();

        assert!(coordinator.apply_rename(&forward));
        assert!(coordinator.apply_rename(&next));

        assert_eq!(
            coordinator.observe_watcher_event(&b.join("New.sol"), FileChangeType::CREATED),
            WatchedFileAction::Process
        );
    }

    #[test]
    fn watched_parent_rename_invalidates_nested_applied_guard() {
        let a = path("/workspace/A");
        let b = path("/workspace/B");
        let c = path("/workspace/C");
        let nested = FileMoveBatch::new([(a.join("Sub"), b.join("Sub"))]).unwrap();
        let parent = FileMoveBatch::new([(b.clone(), c.clone())]).unwrap();
        let mut coordinator = FileOperationCoordinator::default();

        assert!(coordinator.apply_rename(&nested));
        coordinator.prepare_rename(parent.clone()).activate();
        assert_eq!(
            coordinator.observe_watcher_event(&b, FileChangeType::DELETED),
            WatchedFileAction::Ignore
        );
        assert_eq!(
            coordinator.observe_watcher_event(&c, FileChangeType::CREATED),
            WatchedFileAction::ApplyRenames(vec![parent.clone()])
        );
        assert!(coordinator.claim_watched_rename(&parent));

        assert_eq!(
            coordinator.observe_watcher_event(&b.join("Sub/New.sol"), FileChangeType::CREATED),
            WatchedFileAction::Process
        );
    }

    #[test]
    fn did_parent_rename_invalidates_nested_applied_guard() {
        let a = path("/workspace/A");
        let b = path("/workspace/B");
        let c = path("/workspace/C");
        let nested = FileMoveBatch::new([(a.join("Sub"), b.join("Sub"))]).unwrap();
        let parent = FileMoveBatch::new([(b.clone(), c)]).unwrap();
        let mut coordinator = FileOperationCoordinator::default();

        assert!(coordinator.apply_rename(&nested));
        coordinator.prepare_rename(parent.clone()).activate();
        assert!(coordinator.apply_rename(&parent));

        assert_eq!(
            coordinator.observe_watcher_event(&b.join("Sub/New.sol"), FileChangeType::CREATED),
            WatchedFileAction::Process
        );
    }

    #[test]
    fn reverse_did_only_rename_ends_applied_lifecycle() {
        let a = path("/workspace/A.sol");
        let b = path("/workspace/B.sol");
        let forward = FileMoveBatch::new([(a.clone(), b.clone())]).unwrap();
        let reverse = FileMoveBatch::new([(b, a)]).unwrap();
        let mut coordinator = FileOperationCoordinator::default();

        assert!(coordinator.apply_rename(&forward));
        assert!(!coordinator.apply_rename(&forward));
        assert!(coordinator.apply_rename(&reverse));
        assert!(coordinator.apply_rename(&forward));
        assert!(!coordinator.apply_rename(&forward));
    }

    #[test]
    fn independent_renames_retain_their_replay_guards() {
        let a = path("/workspace/A.sol");
        let b = path("/workspace/B.sol");
        let x = path("/workspace/X.sol");
        let y = path("/workspace/Y.sol");
        let first = FileMoveBatch::new([(a.clone(), b.clone())]).unwrap();
        let second = FileMoveBatch::new([(x, y)]).unwrap();
        let mut coordinator = FileOperationCoordinator::default();

        assert!(coordinator.apply_rename(&first));
        assert_eq!(
            coordinator.observe_watcher_event(&a, FileChangeType::DELETED),
            WatchedFileAction::Ignore
        );
        assert!(coordinator.apply_rename(&second));
        assert_eq!(
            coordinator.observe_watcher_event(&b, FileChangeType::CREATED),
            WatchedFileAction::Ignore
        );
        assert!(!coordinator.apply_rename(&first));
        assert!(!coordinator.apply_rename(&second));
    }

    #[test]
    fn direct_event_echoes_are_exact_and_expire_on_opposite_activity() {
        let a = path("/workspace/A.sol");
        let b = path("/workspace/B.sol");
        let unrelated = path("/workspace/Unrelated.sol");
        let mut coordinator = FileOperationCoordinator::default();

        coordinator.record_direct_events(FileChangeType::CREATED, [a.clone(), b.clone()]);
        assert_eq!(
            coordinator.observe_watcher_event(&a, FileChangeType::CREATED),
            WatchedFileAction::Ignore
        );
        assert_eq!(
            coordinator.observe_watcher_event(&a, FileChangeType::CREATED),
            WatchedFileAction::Ignore
        );
        assert_eq!(
            coordinator.observe_watcher_event(&unrelated, FileChangeType::CREATED),
            WatchedFileAction::Process
        );
        assert_eq!(
            coordinator.observe_watcher_event(&a, FileChangeType::CHANGED),
            WatchedFileAction::Process
        );
        assert_eq!(
            coordinator.observe_watcher_event(&a, FileChangeType::CREATED),
            WatchedFileAction::Process
        );
        assert_eq!(
            coordinator.observe_watcher_event(&b, FileChangeType::DELETED),
            WatchedFileAction::Process
        );
        assert_eq!(
            coordinator.observe_watcher_event(&b, FileChangeType::CREATED),
            WatchedFileAction::Process
        );
    }

    #[test]
    fn watched_event_history_requires_complete_batch_and_expires_on_direct_opposite() {
        let a = path("/workspace/A.sol");
        let b = path("/workspace/B.sol");
        let mut coordinator = FileOperationCoordinator::default();

        coordinator.record_watched_events(FileChangeType::CREATED, [a.clone()]);
        assert!(
            !coordinator.consume_watched_events(FileChangeType::CREATED, &[a.clone(), b.clone()])
        );

        coordinator.record_watched_events(FileChangeType::CREATED, [a.clone(), b.clone()]);
        assert!(
            coordinator.consume_watched_events(FileChangeType::CREATED, &[a.clone(), b.clone()])
        );
        assert!(!coordinator.consume_watched_events(FileChangeType::CREATED, &[a.clone(), b]));

        coordinator.record_watched_events(FileChangeType::CREATED, [a.clone()]);
        coordinator.record_direct_events(FileChangeType::DELETED, [a.clone()]);
        assert!(!coordinator.consume_watched_events(FileChangeType::CREATED, &[a]));
    }

    #[test]
    fn direct_opposite_event_ends_rename_replay_guard() {
        let a = path("/workspace/A.sol");
        let b = path("/workspace/B.sol");
        let batch = FileMoveBatch::new([(a.clone(), b)]).unwrap();
        let mut coordinator = FileOperationCoordinator::default();

        assert!(coordinator.apply_rename(&batch));
        assert!(!coordinator.apply_rename(&batch));
        coordinator.record_direct_events(FileChangeType::CREATED, [a]);
        assert!(coordinator.apply_rename(&batch));
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
