use std::{
    fs::{ReadDir, read_dir},
    path::{Path, PathBuf},
};

use solar_interface::data_structures::map::rustc_hash::FxHashSet;
use tokio::io;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub(crate) enum ProjectManifest {
    // todo: guarantee this to be absolute
    Foundry(PathBuf),
}

impl ProjectManifest {
    fn discover(path: &Path) -> io::Result<Vec<Self>> {
        return find_foundry_toml(path)
            .map(|paths| paths.into_iter().map(ProjectManifest::Foundry).collect());

        fn find_foundry_toml(path: &Path) -> io::Result<Vec<PathBuf>> {
            match find_in_parent_dirs(path, "foundry.toml") {
                Some(it) => Ok(vec![it]),
                None => Ok(find_foundry_toml_in_child_dir(read_dir(path)?)),
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

        fn find_foundry_toml_in_child_dir(entities: ReadDir) -> Vec<PathBuf> {
            entities
                .filter_map(Result::ok)
                .map(|it| it.path().join("foundry.toml"))
                .filter(|it| it.exists())
                .collect()
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn child_discovery_finds_foundry_manifest() {
        let root = TempDir::new().unwrap();
        let child = root.path().join("child");
        fs::create_dir(&child).unwrap();
        fs::write(child.join("foundry.toml"), "").unwrap();

        assert_eq!(
            ProjectManifest::discover_all(&[root.path().to_path_buf()]),
            vec![ProjectManifest::Foundry(child.join("foundry.toml"))],
        );
    }

    #[test]
    fn parent_discovery_prefers_nearest_foundry_manifest() {
        let root = TempDir::new().unwrap();
        let child = root.path().join("child");
        fs::create_dir(&child).unwrap();
        fs::write(root.path().join("foundry.toml"), "").unwrap();
        fs::write(child.join("foundry.toml"), "").unwrap();

        assert_eq!(
            ProjectManifest::discover_all(std::slice::from_ref(&child)),
            vec![ProjectManifest::Foundry(child.join("foundry.toml"))],
        );
    }
}
