//! Dependency validation for reusable per-batch analysis outputs.
//!
//! Record the loader's exact path resolutions and raw source reads, including failed resolution
//! probes. Reuse requires replaying every observation and checking the working directory. This
//! deliberately trades disk reads and retained source strings for avoiding repeated analysis;
//! timestamps and hashes are not correctness substitutes.

use crate::workspace::index_policy::IndexingCancellation;
use solar_interface::{
    data_structures::sync::Mutex,
    source_map::{FileLoader, RealFileLoader},
};
use std::{
    collections::HashMap,
    env, io,
    path::{Path, PathBuf},
    sync::Arc,
};

/// Filesystem observations shared by a batch's loader and cache entry.
#[derive(Clone)]
pub(super) struct DependencySnapshot {
    observations: Arc<Mutex<Option<Observations>>>,
}

struct Observations {
    cwd: PathBuf,
    entries: Vec<Observation>,
    /// Deduplicate repeated loader calls while retaining conflict detection.
    resolutions: HashMap<PathBuf, Resolution>,
    reads: HashMap<PathBuf, usize>,
}

enum Resolution {
    Canonical(PathBuf),
    Missing(Option<i32>),
}

enum Observation {
    Canonicalized { path: PathBuf, canonical: PathBuf },
    Missing { path: PathBuf, raw_os_error: Option<i32> },
    Read { path: PathBuf, source: String },
}

impl Default for DependencySnapshot {
    fn default() -> Self {
        let observations = env::current_dir().ok().map(|cwd| Observations {
            cwd,
            entries: Vec::new(),
            resolutions: HashMap::new(),
            reads: HashMap::new(),
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
            Err(_) => {
                self.invalidate();
                return;
            }
        };
        let mut conflict = false;
        let mut observations_guard = self.observations.lock();
        if let Some(observations) = observations_guard.as_mut() {
            let key = path.to_path_buf();
            match observations.resolutions.get(&key) {
                Some(Resolution::Canonical(previous)) if matches!(&resolution, Resolution::Canonical(current) if current == previous) =>
                    {}
                Some(Resolution::Missing(previous)) if matches!(&resolution, Resolution::Missing(current) if current == previous) =>
                    {}
                Some(_) => conflict = true,
                None => {
                    let entry = match &resolution {
                        Resolution::Canonical(canonical) => Observation::Canonicalized {
                            path: key.clone(),
                            canonical: canonical.clone(),
                        },
                        Resolution::Missing(raw_os_error) => {
                            Observation::Missing { path: key.clone(), raw_os_error: *raw_os_error }
                        }
                    };
                    observations.resolutions.insert(key, resolution);
                    observations.entries.push(entry);
                }
            }
        }
        drop(observations_guard);
        if conflict {
            self.invalidate();
        }
    }

    pub(super) fn record_read(&self, path: &Path, result: &io::Result<String>) {
        let Ok(source) = result else {
            self.invalidate();
            return;
        };
        let mut observations_guard = self.observations.lock();
        let mut conflict = false;
        if let Some(observations) = observations_guard.as_mut() {
            if let Some(&index) = observations.reads.get(path) {
                let previous = match &observations.entries[index] {
                    Observation::Read { source: previous, .. } => previous,
                    _ => unreachable!(),
                };
                conflict = previous != source;
            } else {
                let index = observations.entries.len();
                observations.reads.insert(path.to_path_buf(), index);
                observations
                    .entries
                    .push(Observation::Read { path: path.to_path_buf(), source: source.clone() });
            }
        }
        drop(observations_guard);
        if conflict {
            self.invalidate();
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
        for observation in &observations.entries {
            if cancellation.is_cancelled() {
                return false;
            }
            let matches = match observation {
                Observation::Canonicalized { path, canonical } => {
                    RealFileLoader.canonicalize_path(path).ok().as_ref() == Some(canonical)
                }
                Observation::Missing { path, raw_os_error } => {
                    RealFileLoader.canonicalize_path(path).is_err_and(|error| {
                        error.kind() == io::ErrorKind::NotFound
                            && error.raw_os_error() == *raw_os_error
                    })
                }
                Observation::Read { path, source } => {
                    RealFileLoader.load_file(path).ok().as_ref() == Some(source)
                }
            };
            if !matches {
                return false;
            }
        }
        !cancellation.is_cancelled()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::symlink;

    #[test]
    fn unchanged_resolutions_and_reads_are_reusable() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("source.sol");
        fs::write(&path, "contract Source {}\n").unwrap();
        let snapshot = DependencySnapshot::default();
        snapshot.record_canonicalize(&path, &RealFileLoader.canonicalize_path(&path));
        snapshot.record_read(&path, &RealFileLoader.load_file(&path));

        assert!(snapshot.unchanged(&IndexingCancellation::default()));
    }

    #[test]
    fn source_byte_changes_prevent_reuse() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("source.sol");
        fs::write(&path, "contract Before {}\n").unwrap();
        let snapshot = DependencySnapshot::default();
        snapshot.record_read(&path, &RealFileLoader.load_file(&path));
        fs::write(&path, "contract After_ {}\n").unwrap();

        assert!(!snapshot.unchanged(&IndexingCancellation::default()));
    }

    #[test]
    fn removing_a_byte_order_mark_prevents_reuse() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("source.sol");
        fs::write(&path, "\u{feff}contract Source {}\n").unwrap();
        let snapshot = DependencySnapshot::default();
        snapshot.record_read(&path, &RealFileLoader.load_file(&path));
        assert!(snapshot.unchanged(&IndexingCancellation::default()));
        fs::write(&path, "contract Source {}\n").unwrap();

        assert!(!snapshot.unchanged(&IndexingCancellation::default()));
    }

    #[test]
    fn creating_a_missing_resolution_candidate_prevents_reuse() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("missing.sol");
        let snapshot = DependencySnapshot::default();
        let resolution = RealFileLoader.canonicalize_path(&path);
        assert_eq!(resolution.as_ref().unwrap_err().kind(), io::ErrorKind::NotFound);
        snapshot.record_canonicalize(&path, &resolution);
        assert!(snapshot.unchanged(&IndexingCancellation::default()));
        fs::write(&path, "contract Created {}\n").unwrap();

        assert!(!snapshot.unchanged(&IndexingCancellation::default()));
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
        snapshot.record_canonicalize(&link, &RealFileLoader.canonicalize_path(&link));
        snapshot.record_read(&link, &RealFileLoader.load_file(&link));
        assert!(snapshot.unchanged(&IndexingCancellation::default()));
        fs::remove_file(&link).unwrap();
        symlink(&second, &link).unwrap();

        assert!(!snapshot.unchanged(&IndexingCancellation::default()));
    }

    #[test]
    fn unexpected_canonicalization_errors_prevent_reuse() {
        let snapshot = DependencySnapshot::default();
        snapshot.record_canonicalize(
            Path::new("source.sol"),
            &Err(io::Error::from(io::ErrorKind::PermissionDenied)),
        );

        assert!(!snapshot.unchanged(&IndexingCancellation::default()));
    }

    #[test]
    fn read_errors_prevent_reuse() {
        let snapshot = DependencySnapshot::default();
        snapshot
            .record_read(Path::new("missing.sol"), &Err(io::Error::from(io::ErrorKind::NotFound)));

        assert!(!snapshot.unchanged(&IndexingCancellation::default()));
    }

    #[test]
    fn conflicting_reads_are_not_overwritten() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("source.sol");
        fs::write(&path, "contract Before {}\n").unwrap();
        let snapshot = DependencySnapshot::default();
        snapshot.record_read(&path, &RealFileLoader.load_file(&path));
        fs::write(&path, "contract After_ {}\n").unwrap();
        snapshot.record_read(&path, &RealFileLoader.load_file(&path));

        assert!(!snapshot.unchanged(&IndexingCancellation::default()));
    }

    #[test]
    fn cancellation_prevents_reuse() {
        let snapshot = DependencySnapshot::default();
        let cancellation = IndexingCancellation::default();
        cancellation.cancel();

        assert!(!snapshot.unchanged(&cancellation));
    }

    #[test]
    fn invalidation_is_shared_and_cannot_be_reversed_by_recording() {
        let directory = tempfile::tempdir().unwrap();
        let snapshot = DependencySnapshot::default();
        let loader_snapshot = snapshot.clone();
        loader_snapshot.invalidate();
        loader_snapshot.record_canonicalize(
            directory.path(),
            &RealFileLoader.canonicalize_path(directory.path()),
        );

        assert!(!snapshot.unchanged(&IndexingCancellation::default()));
    }
}
