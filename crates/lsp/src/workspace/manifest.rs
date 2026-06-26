use std::{
    fs::read_dir,
    io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub(crate) struct FoundryManifest(PathBuf);

impl FoundryManifest {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self(path)
    }

    pub(crate) fn discover(path: &Path) -> Vec<Self> {
        let mut manifests = Self::try_discover(path).unwrap_or_default();
        manifests.sort();
        manifests.dedup();
        manifests
    }

    pub(crate) fn root(&self) -> Option<&Path> {
        self.0.parent()
    }

    pub(crate) fn into_path(self) -> PathBuf {
        self.0
    }

    fn try_discover(path: &Path) -> io::Result<Vec<Self>> {
        if let Some(manifest) = find_in_parent_dirs(path) {
            return Ok(vec![manifest]);
        }

        find_in_child_dirs(path)
    }
}

fn find_in_parent_dirs(path: &Path) -> Option<FoundryManifest> {
    if let Some("foundry.toml") = path.file_name().and_then(|name| name.to_str()) {
        return Some(FoundryManifest::new(path.to_path_buf()));
    }

    let mut curr = Some(path);

    while let Some(path) = curr {
        let foundry = path.join("foundry.toml");
        if std::fs::metadata(&foundry).is_ok() {
            return Some(FoundryManifest::new(foundry));
        }

        curr = path.parent();
    }

    None
}

fn find_in_child_dirs(path: &Path) -> io::Result<Vec<FoundryManifest>> {
    let mut manifests = Vec::new();
    for entry in read_dir(path)?.filter_map(Result::ok) {
        let path = entry.path();
        let foundry = path.join("foundry.toml");
        if foundry.exists() {
            manifests.push(FoundryManifest::new(foundry));
        }
    }
    Ok(manifests)
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
            FoundryManifest::discover(root.path()),
            vec![FoundryManifest::new(child.join("foundry.toml"))],
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
            FoundryManifest::discover(&child),
            vec![FoundryManifest::new(child.join("foundry.toml"))],
        );
    }
}
