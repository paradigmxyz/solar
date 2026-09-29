//! Dependency validation for reusable per-batch analysis outputs.
//!
//! Record the loader's exact path resolutions and raw source reads, including failed resolution
//! probes. Reuse requires replaying every observation and checking the working directory. This
//! deliberately trades disk reads and retained source strings for avoiding repeated analysis;
//! timestamps and hashes are not correctness substitutes.

use crate::workspace::index_policy::IndexingCancellation;
use solar_interface::{
    data_structures::{map::FxHashMap, sync::Mutex},
    source_map::{FileLoader, RealFileLoader},
};
use std::{
    collections::hash_map::Entry,
    env, io,
    path::{Path, PathBuf},
    sync::Arc,
};

/// Filesystem observations shared by a batch's loader and cache entry.
#[derive(Clone)]
pub(super) struct DependencySnapshot {
    observations: Arc<Mutex<Option<Observations>>>,
}

/// Replay order is irrelevant: reuse requires every observation to match.
struct Observations {
    cwd: PathBuf,
    resolutions: FxHashMap<PathBuf, Resolution>,
    reads: FxHashMap<PathBuf, String>,
}

#[derive(PartialEq)]
enum Resolution {
    Canonical(PathBuf),
    Missing(Option<i32>),
}

impl Default for DependencySnapshot {
    fn default() -> Self {
        let observations = env::current_dir().ok().map(|cwd| Observations {
            cwd,
            resolutions: FxHashMap::default(),
            reads: FxHashMap::default(),
        });
        Self { observations: Arc::new(Mutex::new(observations)) }
    }
}

impl DependencySnapshot {
    pub(super) fn record_canonicalize(&self, path: &Path, result: &io::Result<PathBuf>) {
        let resolution = match result {
            Ok(canonical) => Resolution::Canonical(canonical.clone()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                Resolution::Missing(error.raw_os_error())
            }
            Err(_) => return self.invalidate(),
        };
        let mut observations = self.observations.lock();
        let Some(recorded) = observations.as_mut() else { return };
        let conflict = match recorded.resolutions.entry(path.to_path_buf()) {
            Entry::Occupied(previous) => *previous.get() != resolution,
            Entry::Vacant(entry) => {
                entry.insert(resolution);
                false
            }
        };
        if conflict {
            *observations = None;
        }
    }

    pub(super) fn record_read(&self, path: &Path, result: &io::Result<String>) {
        let Ok(source) = result else { return self.invalidate() };
        let mut observations = self.observations.lock();
        let Some(recorded) = observations.as_mut() else { return };
        let conflict = match recorded.reads.entry(path.to_path_buf()) {
            Entry::Occupied(previous) => previous.get() != source,
            Entry::Vacant(entry) => {
                entry.insert(source.clone());
                false
            }
        };
        if conflict {
            *observations = None;
        }
    }

    pub(super) fn invalidate(&self) {
        *self.observations.lock() = None;
    }

    pub(super) fn unchanged(&self, cancellation: &IndexingCancellation) -> bool {
        if cancellation.is_cancelled() {
            return false;
        }
        let observations = self.observations.lock();
        let Some(observations) = observations.as_ref() else {
            return false;
        };
        if env::current_dir().ok().as_ref() != Some(&observations.cwd) {
            return false;
        }
        let resolutions_match = observations.resolutions.iter().all(|(path, resolution)| {
            !cancellation.is_cancelled()
                && match (RealFileLoader.canonicalize_path(path), resolution) {
                    (Ok(current), Resolution::Canonical(canonical)) => current == *canonical,
                    (Err(error), Resolution::Missing(raw_os_error)) => {
                        error.kind() == io::ErrorKind::NotFound
                            && error.raw_os_error() == *raw_os_error
                    }
                    _ => false,
                }
        });
        resolutions_match
            && observations.reads.iter().all(|(path, source)| {
                !cancellation.is_cancelled()
                    && RealFileLoader.load_file(path).ok().as_ref() == Some(source)
            })
            && !cancellation.is_cancelled()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::symlink;

    fn unchanged(snapshot: &DependencySnapshot) -> bool {
        snapshot.unchanged(&IndexingCancellation::default())
    }

    fn record(snapshot: &DependencySnapshot, path: &Path) {
        snapshot.record_canonicalize(path, &RealFileLoader.canonicalize_path(path));
        snapshot.record_read(path, &RealFileLoader.load_file(path));
    }

    #[test]
    fn observations_are_reusable_until_source_bytes_change() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("source.sol");
        fs::write(&path, "\u{feff}contract Source {}\n").unwrap();
        let snapshot = DependencySnapshot::default();
        record(&snapshot, &path);
        record(&snapshot, &path);
        assert!(unchanged(&snapshot));

        fs::write(&path, "contract Source {}\n").unwrap();
        assert!(!unchanged(&snapshot));
    }

    #[test]
    fn creating_a_missing_resolution_candidate_prevents_reuse() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("missing.sol");
        let snapshot = DependencySnapshot::default();
        let resolution = RealFileLoader.canonicalize_path(&path);
        assert_eq!(resolution.as_ref().unwrap_err().kind(), io::ErrorKind::NotFound);
        snapshot.record_canonicalize(&path, &resolution);
        assert!(unchanged(&snapshot));
        fs::write(&path, "contract Created {}\n").unwrap();

        assert!(!unchanged(&snapshot));
    }

    #[test]
    #[cfg(unix)]
    fn retargeting_a_symlink_with_identical_contents_prevents_reuse() {
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("first.sol");
        let second = directory.path().join("second.sol");
        let link = directory.path().join("link.sol");
        fs::write(&first, "contract Source {}\n").unwrap();
        fs::write(&second, "contract Source {}\n").unwrap();
        symlink(&first, &link).unwrap();
        let snapshot = DependencySnapshot::default();
        record(&snapshot, &link);
        assert!(unchanged(&snapshot));
        fs::remove_file(&link).unwrap();
        symlink(&second, &link).unwrap();

        assert!(!unchanged(&snapshot));
    }

    #[test]
    fn conflicts_errors_cancellation_and_invalidation_prevent_reuse() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("source.sol");
        fs::write(&path, "contract Before {}\n").unwrap();
        let conflicting_reads = DependencySnapshot::default();
        conflicting_reads.record_read(&path, &RealFileLoader.load_file(&path));
        fs::write(&path, "contract After_ {}\n").unwrap();
        conflicting_reads.record_read(&path, &RealFileLoader.load_file(&path));
        assert!(!unchanged(&conflicting_reads));

        let canonicalize_error = DependencySnapshot::default();
        canonicalize_error.record_canonicalize(
            Path::new("source.sol"),
            &Err(io::Error::from(io::ErrorKind::PermissionDenied)),
        );
        assert!(!unchanged(&canonicalize_error));

        let read_error = DependencySnapshot::default();
        read_error
            .record_read(Path::new("missing.sol"), &Err(io::Error::from(io::ErrorKind::NotFound)));
        assert!(!unchanged(&read_error));

        let cancellation = IndexingCancellation::default();
        cancellation.cancel();
        assert!(!DependencySnapshot::default().unchanged(&cancellation));

        let invalidated = DependencySnapshot::default();
        let loader_snapshot = invalidated.clone();
        loader_snapshot.invalidate();
        loader_snapshot.record_canonicalize(
            directory.path(),
            &RealFileLoader.canonicalize_path(directory.path()),
        );
        assert!(!unchanged(&invalidated));
    }
}
