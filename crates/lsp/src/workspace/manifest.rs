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
    pub(crate) fn discover_in_parents(path: &Path) -> Option<Self> {
        find_in_parent_dirs(path, "foundry.toml").map(Self::Foundry)
    }

    fn discover(path: &Path) -> io::Result<Vec<Self>> {
        // Keep naked roots shallow, but recurse once a Foundry project boundary is known.
        let mut manifests = Vec::new();
        if let Some(manifest) = find_in_parent_dirs(path, "foundry.toml") {
            manifests.push(manifest);
            if let Ok(entries) = read_dir(path) {
                find_foundry_toml_in_child_dirs(entries, &mut manifests, true);
            }
        } else {
            find_foundry_toml_in_child_dirs(read_dir(path)?, &mut manifests, false);
        }
        Ok(manifests.into_iter().map(ProjectManifest::Foundry).collect())
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

    let mut current = Some(path);
    while let Some(path) = current {
        let candidate = path.join(target_file_name);
        if std::fs::metadata(&candidate).is_ok() {
            return Some(candidate);
        }
        current = path.parent();
    }
    None
}

fn find_foundry_toml_in_child_dirs(
    entities: ReadDir,
    manifests: &mut Vec<PathBuf>,
    within_project: bool,
) {
    for entry in entities.filter_map(Result::ok) {
        let Ok(file_type) = entry.file_type() else { continue };
        let path = entry.path();
        if !file_type.is_dir() || is_heavy_dir(&path) {
            continue;
        }

        let manifest = path.join("foundry.toml");
        let is_project = manifest.is_file();
        if is_project {
            manifests.push(manifest);
        }
        if (within_project || is_project)
            && let Ok(children) = read_dir(path)
        {
            find_foundry_toml_in_child_dirs(children, manifests, true);
        }
    }
}

fn is_heavy_dir(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some(".git" | "cache" | "lib" | "node_modules" | "out")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestProject;

    #[test]
    fn naked_root_discovery_is_shallow() {
        let project = TestProject::from_fixture(
            r#"
            //- /child/foundry.toml

            //- /container/deep/foundry.toml
            "#,
        );

        assert_eq!(
            ProjectManifest::discover_all(&[project.root().to_path_buf()]),
            vec![ProjectManifest::Foundry(project.path("/child/foundry.toml"))],
        );
    }

    #[test]
    fn root_project_recursively_discovers_nested_projects_and_skips_heavy_dirs() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml

            //- /packages/token/foundry.toml

            //- /packages/group/vault/foundry.toml

            //- /.git/dependency/foundry.toml

            //- /cache/dependency/foundry.toml

            //- /lib/dependency/foundry.toml

            //- /node_modules/dependency/foundry.toml

            //- /out/dependency/foundry.toml
            "#,
        );

        assert_eq!(
            ProjectManifest::discover_all(&[project.root().to_path_buf()]),
            vec![
                ProjectManifest::Foundry(project.path("/foundry.toml")),
                ProjectManifest::Foundry(project.path("/packages/group/vault/foundry.toml")),
                ProjectManifest::Foundry(project.path("/packages/token/foundry.toml")),
            ],
        );
    }

    #[test]
    fn parent_discovery_prefers_nearest_foundry_manifest() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml

            //- /child/foundry.toml
            "#,
        );
        let child = project.path("/child");

        assert_eq!(
            ProjectManifest::discover_all(std::slice::from_ref(&child)),
            vec![ProjectManifest::Foundry(child.join("foundry.toml"))],
        );
    }
}
