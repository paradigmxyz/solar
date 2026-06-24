use std::{
    fs::{ReadDir, read_dir},
    path::{Path, PathBuf},
};

use solar_interface::data_structures::map::rustc_hash::FxHashSet;
use tokio::io;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub(crate) enum ProjectManifest {
    // todo: guarantee this to be absolute
    Solar(PathBuf),
    // todo: guarantee this to be absolute
    Foundry(PathBuf),
}

impl ProjectManifest {
    fn discover(path: &Path) -> io::Result<Vec<Self>> {
        if let Some(path) = find_in_parent_dirs(path, "solar.toml") {
            return Ok(vec![Self::Solar(path)]);
        }
        if let Some(path) = find_in_parent_dirs(path, "foundry.toml") {
            return Ok(vec![Self::Foundry(path)]);
        }

        let mut manifests = Vec::new();
        for path in find_in_child_dir(read_dir(path)?, "solar.toml") {
            manifests.push(Self::Solar(path));
        }
        for path in find_in_child_dir(read_dir(path)?, "foundry.toml") {
            manifests.push(Self::Foundry(path));
        }
        Ok(manifests)
    }

    /// Discover all project manifests at the given paths.
    ///
    /// Returns a `Vec` of discovered [`ProjectManifest`]s, which is guaranteed to be unique and
    /// sorted.
    pub(crate) fn discover_all(paths: &[PathBuf]) -> Vec<Self> {
        let mut res = paths
            .iter()
            .filter_map(|it| Self::discover(it.as_ref()).ok())
            .flatten()
            .collect::<FxHashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        res.sort();
        res
    }
}

fn find_in_parent_dirs(path: &Path, target_file_name: &str) -> Option<PathBuf> {
    if path.file_name().unwrap_or_default() == target_file_name {
        return Some(path.to_path_buf());
    }

    let mut curr = Some(path);

    while let Some(path) = curr {
        let candidate = path.join(target_file_name);
        if std::fs::metadata(&candidate).is_ok() {
            return Some(candidate);
        }

        curr = path.parent();
    }

    None
}

fn find_in_child_dir(entities: ReadDir, file_name: &str) -> Vec<PathBuf> {
    entities
        .filter_map(Result::ok)
        .map(|it| it.path().join(file_name))
        .filter(|it| it.exists())
        .collect()
}
