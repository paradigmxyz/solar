//! Shared normalization and matching for workspace file moves.

use lsp_types::{FileChangeType, RenameFilesParams, Url};
use normalize_path::NormalizePath;
use std::{
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
    roots: Vec<PathBuf>,
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

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub(crate) enum FileMoveError {
    #[error(
        "rename source `{}` has conflicting destinations `{}` and `{}`",
        old_path.display(),
        first_new_path.display(),
        second_new_path.display()
    )]
    ConflictingSource { old_path: PathBuf, first_new_path: PathBuf, second_new_path: PathBuf },
    #[error(
        "rename destination `{}` has conflicting sources `{}` and `{}`",
        new_path.display(),
        first_old_path.display(),
        second_old_path.display()
    )]
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
            Some((FileMoveId(index), rebase(&path, &file_move.old_path, &file_move.new_path)?))
        })
    }

    pub(crate) fn reverse_map_path(&self, path: &Path) -> Option<(FileMoveId, PathBuf)> {
        let path = path.normalize();
        self.moves
            .iter()
            .enumerate()
            .filter_map(|(index, file_move)| {
                let old_path = rebase(&path, &file_move.new_path, &file_move.old_path)?;
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

/// Moves `path` from under `from` to under `to`, without appending a separator to exact matches.
fn rebase(path: &Path, from: &Path, to: &Path) -> Option<PathBuf> {
    let suffix = path.strip_prefix(from).ok()?;
    Some(if suffix.as_os_str().is_empty() { to.to_path_buf() } else { to.join(suffix) })
}

fn normalized_paths(paths: impl IntoIterator<Item = impl AsRef<Path>>) -> Vec<PathBuf> {
    let mut paths = paths.into_iter().map(|path| path.as_ref().normalize()).collect::<Vec<_>>();
    paths.sort_unstable();
    paths.dedup();
    paths
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
        self.renames.push(RenameTransaction {
            watcher: moves.watcher_evidence(),
            moves: moves.clone(),
            activation: None,
            state: RenameTransactionState::Applied,
        });
        self.trim_rename_history();
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
            if transaction.activation.is_some() && !transaction.preparation_is_active() {
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
        roots: impl IntoIterator<Item = PathBuf>,
    ) {
        debug_assert!(matches!(typ, FileChangeType::CREATED | FileChangeType::DELETED));
        let paths = normalized_paths(paths);
        let roots = normalized_paths(roots);
        if paths.is_empty() && roots.is_empty() {
            return;
        }

        self.renames.retain(|transaction| {
            transaction.state != RenameTransactionState::Applied
                || !paths
                    .iter()
                    .chain(&roots)
                    .any(|path| transaction.watcher.is_opposite(path, typ))
        });
        for transaction in self.direct_events.iter_mut().chain(&mut self.watched_events) {
            if transaction.typ != typ {
                transaction.retain_non_overlapping(&paths, &roots);
            }
        }
        self.direct_events.retain(|transaction| !transaction.is_empty());
        self.watched_events.retain(|transaction| !transaction.is_empty());
        self.direct_events.push(DirectFileEventTransaction { typ, paths, roots });
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
        let mut paths = normalized_paths(paths);
        if paths.is_empty() {
            return;
        }

        for transaction in &mut self.watched_events {
            if transaction.typ != typ {
                transaction.paths.retain(|path| paths.binary_search(path).is_err());
            }
        }
        self.watched_events.retain(|transaction| !transaction.paths.is_empty());
        paths.truncate(WATCHED_EVENT_PATH_LIMIT);
        if let Some(transaction) = self.watched_events.last_mut()
            && transaction.typ == typ
        {
            transaction.paths.extend(paths);
            transaction.paths.sort_unstable();
            transaction.paths.dedup();
        } else {
            self.watched_events.push(DirectFileEventTransaction { typ, paths, roots: Vec::new() });
            if self.watched_events.len() > DIRECT_EVENT_HISTORY_LIMIT {
                self.watched_events.remove(0);
            }
        }
        let mut excess = self
            .watched_events
            .iter()
            .map(|transaction| transaction.paths.len())
            .sum::<usize>()
            .saturating_sub(WATCHED_EVENT_PATH_LIMIT);
        for transaction in &mut self.watched_events {
            if excess == 0 {
                break;
            }
            let remove = excess.min(transaction.paths.len());
            transaction.paths.drain(..remove);
            excess -= remove;
        }
        self.watched_events.retain(|transaction| !transaction.paths.is_empty());
    }

    pub(crate) fn consume_watched_events(
        &mut self,
        typ: FileChangeType,
        paths: &[PathBuf],
    ) -> bool {
        let paths = normalized_paths(paths);
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
        self.watched_events
            .iter()
            .filter(|transaction| transaction.typ == typ)
            .flat_map(|transaction| &transaction.paths)
            .filter(|path| roots.iter().any(|root| path.starts_with(root)))
            .cloned()
            .collect()
    }

    fn observe_direct_event(&mut self, path: &Path, typ: FileChangeType) -> bool {
        let path = path.normalize();
        let mut matched = false;
        for transaction in &mut self.direct_events {
            if transaction.typ == typ {
                matched |= transaction.consume_match(&path);
            } else {
                transaction.remove_overlapping(&path);
            }
        }
        self.direct_events.retain(|transaction| !transaction.is_empty());
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

impl DirectFileEventTransaction {
    fn is_empty(&self) -> bool {
        self.paths.is_empty() && self.roots.is_empty()
    }

    fn consume_match(&mut self, path: &Path) -> bool {
        let exact = self.paths.binary_search_by(|candidate| candidate.as_path().cmp(path)).is_ok();
        let roots_len = self.roots.len();
        self.roots.retain(|root| !path.starts_with(root));
        exact || self.roots.len() != roots_len
    }

    fn remove_overlapping(&mut self, path: &Path) {
        self.paths.retain(|candidate| !paths_overlap(candidate, path));
        self.roots.retain(|root| !paths_overlap(root, path));
    }

    fn retain_non_overlapping(&mut self, paths: &[PathBuf], roots: &[PathBuf]) {
        let overlaps =
            |candidate: &Path| paths.iter().chain(roots).any(|path| paths_overlap(candidate, path));
        self.paths.retain(|path| !overlaps(path));
        self.roots.retain(|root| !overlaps(root));
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

    /// Returns whether the event undoes or modifies either side of the rename.
    fn is_opposite(&self, path: &Path, typ: FileChangeType) -> bool {
        let path = path.normalize();
        let old_side = matches!(typ, FileChangeType::CREATED | FileChangeType::CHANGED);
        let new_side = matches!(typ, FileChangeType::DELETED | FileChangeType::CHANGED);
        self.paths.iter().any(|watcher_path| {
            (old_side && path == watcher_path.old_path)
                || (new_side && path == watcher_path.new_path)
        }) || (old_side && self.moves.map_path(&path).is_some())
            || (new_side && self.moves.reverse_map_path(&path).is_some())
    }
}

const RENAME_HISTORY_LIMIT: usize = 16;
const DIRECT_EVENT_HISTORY_LIMIT: usize = 16;
const WATCHED_EVENT_PATH_LIMIT: usize = 4096;
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
    (uri.scheme() == "file")
        .then(|| crate::proto::normalize_file_uri(uri.clone()).to_file_path().ok())
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use WatchedFileAction::{Ignore, Process};

    const CHANGED: FileChangeType = FileChangeType::CHANGED;
    const CREATED: FileChangeType = FileChangeType::CREATED;
    const DELETED: FileChangeType = FileChangeType::DELETED;

    fn path(path: &str) -> PathBuf {
        PathBuf::from(path)
    }

    fn batch(moves: &[(&str, &str)]) -> FileMoveBatch {
        FileMoveBatch::new(moves.iter().map(|&(old, new)| (path(old), path(new)))).unwrap()
    }

    fn mapped(moves: &FileMoveBatch, old: &str) -> Option<PathBuf> {
        moves.map_path(Path::new(old)).map(|(_, new)| new)
    }

    fn reverse_mapped(moves: &FileMoveBatch, new: &str) -> Option<PathBuf> {
        moves.reverse_map_path(Path::new(new)).map(|(_, old)| old)
    }

    fn assert_observations(
        coordinator: &mut FileOperationCoordinator,
        observations: &[(&str, FileChangeType, WatchedFileAction)],
    ) {
        for (index, (path, typ, expected)) in observations.iter().enumerate() {
            let action = coordinator.observe_watcher_event(Path::new(path), *typ);
            assert_eq!(&action, expected, "observation {index}: {path}");
        }
    }

    #[test]
    fn file_move_batches_map_paths() {
        // Exact normalized duplicates collapse into one move.
        let duplicates = batch(&[
            ("/workspace/src/../src", "/workspace/moved/."),
            ("/workspace/src", "/workspace/moved"),
        ]);
        assert_eq!(duplicates.old_paths().count(), 1);
        assert_eq!(
            mapped(&duplicates, "/workspace/src/Test.sol"),
            Some(path("/workspace/moved/Test.sol"))
        );

        // Exact file matches keep the destination spelling.
        let old = path("/workspace/src/Target.sol").normalize();
        let new = path("/workspace/moved/Renamed.sol").normalize();
        let exact = FileMoveBatch::new([(old.clone(), new.clone())]).unwrap();
        assert_eq!(exact.map_path(&old).unwrap().1.as_os_str(), new.as_os_str());
        assert_eq!(exact.reverse_map_path(&new).unwrap().1.as_os_str(), old.as_os_str());

        // The most specific source wins, and prefix siblings do not match.
        let nested = batch(&[
            ("/workspace/pkg/nested", "/workspace/special"),
            ("/workspace/pkg", "/workspace/moved"),
        ]);
        assert_eq!(
            mapped(&nested, "/workspace/pkg/nested/Test.sol"),
            Some(path("/workspace/special/Test.sol"))
        );
        assert_eq!(mapped(&nested, "/workspace/pkg2/Test.sol"), None);

        // Mapping uses one snapshot without chaining moves.
        let chain = batch(&[("/workspace/A", "/workspace/B"), ("/workspace/B", "/workspace/C")]);
        assert_eq!(mapped(&chain, "/workspace/A/Test.sol"), Some(path("/workspace/B/Test.sol")));

        // Reverse mapping prefers the most specific destination.
        let reverse =
            batch(&[("/workspace/A", "/workspace/out"), ("/workspace/B", "/workspace/out/nested")]);
        assert_eq!(
            reverse_mapped(&reverse, "/workspace/out/nested/Test.sol"),
            Some(path("/workspace/B/Test.sol"))
        );
        assert_eq!(reverse_mapped(&reverse, "/workspace/output/Test.sol"), None);
    }

    #[test]
    fn conflicting_normalized_moves_are_rejected() {
        let error = FileMoveBatch::new([
            (path("/workspace/src/../src"), path("/workspace/first")),
            (path("/workspace/src"), path("/workspace/second")),
        ]);
        assert_eq!(
            error,
            Err(FileMoveError::ConflictingSource {
                old_path: path("/workspace/src"),
                first_new_path: path("/workspace/first"),
                second_new_path: path("/workspace/second"),
            })
        );

        let error = FileMoveBatch::new([
            (path("/workspace/first"), path("/workspace/out/../shared")),
            (path("/workspace/second"), path("/workspace/shared")),
        ]);
        assert_eq!(
            error,
            Err(FileMoveError::ConflictingDestination {
                new_path: path("/workspace/shared"),
                first_old_path: path("/workspace/first"),
                second_old_path: path("/workspace/second"),
            })
        );
    }

    #[test]
    fn new_prepare_allows_same_rename_payload_again() {
        let moves = batch(&[("/workspace/A", "/workspace/B")]);
        let mut coordinator = FileOperationCoordinator::default();

        coordinator.prepare_rename(moves.clone()).activate();
        assert!(coordinator.apply_rename(&moves));
        assert!(!coordinator.apply_rename(&moves));

        coordinator.prepare_rename(moves.clone()).activate();
        assert!(coordinator.apply_rename(&moves));
    }

    #[test]
    fn cancelled_same_payload_prepare_preserves_replay_guard() {
        let moves = batch(&[("/workspace/A", "/workspace/B")]);
        let mut coordinator = FileOperationCoordinator::default();

        assert!(coordinator.apply_rename(&moves));
        assert!(!coordinator.apply_rename(&moves));

        drop(coordinator.prepare_rename(moves.clone()));

        assert!(!coordinator.apply_rename(&moves));
    }

    #[test]
    fn cancelled_prepare_does_not_consume_replay_history() {
        let guarded = batch(&[("/workspace/A", "/workspace/B")]);
        let mut coordinator = FileOperationCoordinator::default();

        assert!(coordinator.apply_rename(&guarded));
        for index in 0..RENAME_HISTORY_LIMIT - 1 {
            let old = format!("/workspace/Old{index}");
            let new = format!("/workspace/New{index}");
            assert!(coordinator.apply_rename(&batch(&[(&old, &new)])));
        }
        assert!(!coordinator.apply_rename(&guarded));

        drop(coordinator.prepare_rename(guarded.clone()));

        assert!(!coordinator.apply_rename(&guarded));
    }

    #[test]
    fn latest_activated_same_payload_prepare_replaces_earlier_lifecycle() {
        let moves = batch(&[("/workspace/A", "/workspace/B")]);
        let mut coordinator = FileOperationCoordinator::default();

        coordinator.prepare_rename(moves.clone()).activate();
        coordinator.prepare_rename(moves.clone()).activate();

        assert!(coordinator.apply_rename(&moves));
        assert!(!coordinator.apply_rename(&moves));
    }

    #[test]
    fn did_rename_claims_pending_preparation_before_cancellation() {
        let moves = batch(&[("/workspace/A", "/workspace/B")]);
        let mut coordinator = FileOperationCoordinator::default();

        let preparation = coordinator.prepare_rename(moves.clone());
        assert!(coordinator.apply_rename(&moves));
        drop(preparation);

        assert!(!coordinator.apply_rename(&moves));
    }

    #[test]
    fn did_reuses_prepared_lifecycle_without_external_evidence() {
        let prepared = batch(&[("/workspace/A", "/workspace/B")]);
        let mut coordinator = FileOperationCoordinator::default();

        coordinator.prepare_rename(prepared.clone()).activate();
        assert!(coordinator.apply_rename(&prepared));
        assert!(!coordinator.apply_rename(&prepared));
        assert!(coordinator.apply_rename(&batch(&[("/workspace/X", "/workspace/Y")])));
    }

    #[test]
    fn did_rename_claims_one_of_multiple_pending_same_payload_preparations() {
        let moves = batch(&[("/workspace/A", "/workspace/B")]);
        let mut coordinator = FileOperationCoordinator::default();

        let earlier = coordinator.prepare_rename(moves.clone());
        let later = coordinator.prepare_rename(moves.clone());
        assert!(coordinator.apply_rename(&moves));
        earlier.activate();
        drop(later);

        assert!(!coordinator.apply_rename(&moves));
    }

    #[test]
    fn watcher_claims_one_of_multiple_pending_same_payload_preparations() {
        let moves = batch(&[("/workspace/A.sol", "/workspace/B.sol")]);
        let mut coordinator = FileOperationCoordinator::default();

        coordinator.prepare_rename(moves.clone()).activate();
        let later = coordinator.prepare_rename(moves.clone());
        assert_observations(
            &mut coordinator,
            &[
                ("/workspace/A.sol", DELETED, Ignore),
                ("/workspace/B.sol", CREATED, WatchedFileAction::ApplyRenames(vec![moves.clone()])),
            ],
        );
        assert!(coordinator.claim_watched_rename(&moves));
        later.activate();

        assert!(!coordinator.apply_rename(&moves));
    }

    #[test]
    fn opposite_watcher_activity_ends_applied_rename_lifecycle() {
        let moves = batch(&[("/workspace/A.sol", "/workspace/B.sol")]);
        let mut coordinator = FileOperationCoordinator::default();

        assert!(coordinator.apply_rename(&moves));
        assert!(!coordinator.apply_rename(&moves));
        assert_observations(&mut coordinator, &[("/workspace/A.sol", CREATED, Process)]);
        assert!(coordinator.apply_rename(&moves));
    }

    #[test]
    fn invalidated_applied_rename_does_not_swallow_destination_create() {
        let mut coordinator = FileOperationCoordinator::default();

        assert!(coordinator.apply_rename(&batch(&[("/workspace/A", "/workspace/B")])));
        assert!(coordinator.apply_rename(&batch(&[("/workspace/B", "/workspace/C")])));

        assert_observations(&mut coordinator, &[("/workspace/B/New.sol", CREATED, Process)]);
    }

    #[test]
    fn parent_rename_invalidates_nested_applied_guard() {
        // The parent rename is committed by the watcher or by the did notification.
        for watched in [true, false] {
            let parent = batch(&[("/workspace/B", "/workspace/C")]);
            let mut coordinator = FileOperationCoordinator::default();

            assert!(coordinator.apply_rename(&batch(&[("/workspace/A/Sub", "/workspace/B/Sub")])));
            coordinator.prepare_rename(parent.clone()).activate();
            if watched {
                assert_observations(
                    &mut coordinator,
                    &[
                        ("/workspace/B", DELETED, Ignore),
                        (
                            "/workspace/C",
                            CREATED,
                            WatchedFileAction::ApplyRenames(vec![parent.clone()]),
                        ),
                    ],
                );
                assert!(coordinator.claim_watched_rename(&parent));
            } else {
                assert!(coordinator.apply_rename(&parent));
            }

            assert_observations(
                &mut coordinator,
                &[("/workspace/B/Sub/New.sol", CREATED, Process)],
            );
        }
    }

    #[test]
    fn reverse_did_only_rename_ends_applied_lifecycle() {
        let forward = batch(&[("/workspace/A.sol", "/workspace/B.sol")]);
        let mut coordinator = FileOperationCoordinator::default();

        assert!(coordinator.apply_rename(&forward));
        assert!(!coordinator.apply_rename(&forward));
        assert!(coordinator.apply_rename(&batch(&[("/workspace/B.sol", "/workspace/A.sol")])));
        assert!(coordinator.apply_rename(&forward));
        assert!(!coordinator.apply_rename(&forward));
    }

    #[test]
    fn independent_renames_retain_their_replay_guards() {
        let first = batch(&[("/workspace/A.sol", "/workspace/B.sol")]);
        let second = batch(&[("/workspace/X.sol", "/workspace/Y.sol")]);
        let mut coordinator = FileOperationCoordinator::default();

        assert!(coordinator.apply_rename(&first));
        assert_observations(&mut coordinator, &[("/workspace/A.sol", DELETED, Ignore)]);
        assert!(coordinator.apply_rename(&second));
        assert_observations(&mut coordinator, &[("/workspace/B.sol", CREATED, Ignore)]);
        assert!(!coordinator.apply_rename(&first));
        assert!(!coordinator.apply_rename(&second));
    }

    #[test]
    fn direct_event_echoes_are_exact_and_expire_on_opposite_activity() {
        let mut coordinator = FileOperationCoordinator::default();

        coordinator.record_direct_events(CREATED, [], []);
        assert_observations(&mut coordinator, &[("/workspace/Unrelated.sol", CREATED, Process)]);

        coordinator.record_direct_events(
            CREATED,
            [path("/workspace/A.sol"), path("/workspace/B.sol")],
            [],
        );
        assert_observations(
            &mut coordinator,
            &[
                ("/workspace/A.sol", CREATED, Ignore),
                ("/workspace/A.sol", CREATED, Ignore),
                ("/workspace/Unrelated.sol", CREATED, Process),
                ("/workspace/A.sol", CHANGED, Process),
                ("/workspace/A.sol", CREATED, Process),
                ("/workspace/B.sol", DELETED, Process),
                ("/workspace/B.sol", CREATED, Process),
            ],
        );
    }

    #[test]
    fn direct_delete_directory_echoes_match_descendants_and_expire_on_opposite_activity() {
        let child = "/workspace/deleted/nested/Target.sol";
        let mut coordinator = FileOperationCoordinator::default();

        coordinator.record_direct_events(DELETED, [], [path("/workspace/deleted")]);

        assert_observations(
            &mut coordinator,
            &[(child, DELETED, Ignore), (child, CREATED, Process), (child, DELETED, Process)],
        );
    }

    #[test]
    fn watched_event_history_requires_complete_batch_and_expires_on_direct_opposite() {
        let (a, b) = (path("/workspace/A.sol"), path("/workspace/B.sol"));
        let both = [a.clone(), b];
        let mut coordinator = FileOperationCoordinator::default();

        coordinator.record_watched_events(CREATED, [a.clone()]);
        assert!(!coordinator.consume_watched_events(CREATED, &both));

        coordinator.record_watched_events(CREATED, both.clone());
        assert!(coordinator.consume_watched_events(CREATED, &both));
        assert!(!coordinator.consume_watched_events(CREATED, &both));

        coordinator.record_watched_events(CREATED, [a.clone()]);
        coordinator.record_direct_events(DELETED, [a.clone()], []);
        assert!(!coordinator.consume_watched_events(CREATED, &[a]));
    }

    #[test]
    fn watched_event_history_caps_retained_paths() {
        let root = path("/workspace");
        let paths = (0..=WATCHED_EVENT_PATH_LIMIT).map(|idx| root.join(format!("{idx}.sol")));
        let mut coordinator = FileOperationCoordinator::default();

        coordinator.record_watched_events(CREATED, paths);

        assert_eq!(
            coordinator.watched_event_paths_under(CREATED, &[root]).len(),
            WATCHED_EVENT_PATH_LIMIT
        );
    }

    #[test]
    fn watched_event_cap_still_expires_opposite_paths() {
        let root = path("/workspace");
        let old = root.join("zzzz.sol");
        let deleted = (0..WATCHED_EVENT_PATH_LIMIT)
            .map(|idx| root.join(format!("{idx:04}.sol")))
            .chain([old.clone()]);
        let mut coordinator = FileOperationCoordinator::default();
        coordinator.record_watched_events(CREATED, [old.clone()]);

        coordinator.record_watched_events(DELETED, deleted);

        assert!(!coordinator.consume_watched_events(CREATED, &[old]));
    }

    #[test]
    fn direct_opposite_event_ends_rename_replay_guard() {
        let moves = batch(&[("/workspace/A.sol", "/workspace/B.sol")]);
        let mut coordinator = FileOperationCoordinator::default();

        assert!(coordinator.apply_rename(&moves));
        assert!(!coordinator.apply_rename(&moves));
        coordinator.record_direct_events(CREATED, [path("/workspace/A.sol")], []);
        assert!(coordinator.apply_rename(&moves));
    }

    #[test]
    fn file_uris_are_normalized_and_other_schemes_are_ignored() {
        let path = std::env::temp_dir().join("Contract.sol");
        let uri = Url::from_file_path(&path).unwrap();
        let equivalent =
            Url::parse(&uri.as_str().replacen("Contract.sol", "missing%2F..%2FContract.sol", 1))
                .unwrap();
        assert_eq!(file_path_from_url(&equivalent), Some(path));

        let untitled = uri.as_str().replacen("file:", "untitled:", 1);
        let new_uri = Url::from_file_path(std::env::temp_dir().join("New.sol")).unwrap();
        let moves = FileMoveBatch::try_from(RenameFilesParams {
            files: vec![lsp_types::FileRename { old_uri: untitled, new_uri: new_uri.to_string() }],
        })
        .unwrap();
        assert!(moves.is_empty());
    }
}
