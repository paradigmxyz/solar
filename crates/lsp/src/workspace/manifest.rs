use std::{
    fs::read_dir,
    io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub(crate) enum ProjectManifest {
    // todo: guarantee this to be absolute
    Solar(PathBuf),
    // todo: guarantee this to be absolute
    Foundry(PathBuf),
}

impl ProjectManifest {
    pub(crate) fn discover(path: &Path) -> Vec<Self> {
        let mut manifests = Self::try_discover(path).unwrap_or_default();
        manifests.sort();
        manifests.dedup();
        manifests
    }

    pub(crate) fn root(&self) -> Option<&Path> {
        match self {
            Self::Solar(path) | Self::Foundry(path) => path.parent(),
        }
    }

    fn try_discover(path: &Path) -> io::Result<Vec<Self>> {
        if let Some(manifest) = find_in_parent_dirs(path) {
            return Ok(vec![manifest]);
        }

        find_in_child_dirs(path)
    }
}

fn find_in_parent_dirs(path: &Path) -> Option<ProjectManifest> {
    match path.file_name().and_then(|name| name.to_str()) {
        Some("solar.toml") => return Some(ProjectManifest::Solar(path.to_path_buf())),
        Some("foundry.toml") => return Some(ProjectManifest::Foundry(path.to_path_buf())),
        _ => {}
    }

    let mut curr = Some(path);

    while let Some(path) = curr {
        let solar = path.join("solar.toml");
        if std::fs::metadata(&solar).is_ok() {
            return Some(ProjectManifest::Solar(solar));
        }
        let foundry = path.join("foundry.toml");
        if std::fs::metadata(&foundry).is_ok() {
            return Some(ProjectManifest::Foundry(foundry));
        }

        curr = path.parent();
    }

    None
}

fn find_in_child_dirs(path: &Path) -> io::Result<Vec<ProjectManifest>> {
    let mut manifests = Vec::new();
    for entry in read_dir(path)?.filter_map(Result::ok) {
        let path = entry.path();
        let solar = path.join("solar.toml");
        if solar.exists() {
            manifests.push(ProjectManifest::Solar(solar));
            continue;
        }
        let foundry = path.join("foundry.toml");
        if foundry.exists() {
            manifests.push(ProjectManifest::Foundry(foundry));
        }
    }
    Ok(manifests)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn child_discovery_prefers_solar_manifest_over_foundry_manifest() {
        let root = TempDir::new().unwrap();
        let child = root.path().join("child");
        fs::create_dir(&child).unwrap();
        fs::write(child.join("solar.toml"), "").unwrap();
        fs::write(child.join("foundry.toml"), "").unwrap();

        assert_eq!(
            ProjectManifest::discover(root.path()),
            vec![ProjectManifest::Solar(child.join("solar.toml"))],
        );
    }

    #[test]
    fn parent_discovery_prefers_nearest_manifest_before_manifest_kind() {
        let root = TempDir::new().unwrap();
        let child = root.path().join("child");
        fs::create_dir(&child).unwrap();
        fs::write(root.path().join("solar.toml"), "").unwrap();
        fs::write(child.join("foundry.toml"), "").unwrap();

        assert_eq!(
            ProjectManifest::discover(&child),
            vec![ProjectManifest::Foundry(child.join("foundry.toml"))],
        );
    }
}
