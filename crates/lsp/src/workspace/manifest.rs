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
        if let Some(path) = find_in_parent_dirs(path, "solar.toml") {
            return Ok(vec![Self::Solar(path)]);
        }
        if let Some(path) = find_in_parent_dirs(path, "foundry.toml") {
            return Ok(vec![Self::Foundry(path)]);
        }

        find_in_child_dirs(path)
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
}
