use std::{
    fs::{ReadDir, read_dir},
    path::{Path, PathBuf},
};

use super::{
    index_policy::{IndexingCancellation, WorkspaceIndexMetrics, WorkspaceIndexPolicy},
    load_foundry_document,
};
use normalize_path::NormalizePath;
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

    fn discover(
        path: &Path,
        policy: &WorkspaceIndexPolicy,
        cancellation: &IndexingCancellation,
        metrics: &mut WorkspaceIndexMetrics,
    ) -> io::Result<Option<Vec<Self>>> {
        // Keep naked roots shallow, but recurse once a Foundry project boundary is known.
        let mut manifests = Vec::new();
        if let Some(manifest) = find_in_parent_dirs(path, "foundry.toml") {
            let workspace_root = manifest.parent().unwrap_or(path).to_path_buf();
            let import_only_roots = foundry_import_only_roots(&manifest);
            manifests.push(manifest);
            if let Ok(entries) = read_dir(path)
                && !(ManifestDiscovery { manifests: &mut manifests, policy, cancellation, metrics })
                    .find_in_child_dirs(entries, true, &workspace_root, path, &import_only_roots)
            {
                return Ok(None);
            }
        } else {
            if !(ManifestDiscovery { manifests: &mut manifests, policy, cancellation, metrics })
                .find_in_child_dirs(read_dir(path)?, false, path, path, &[])
            {
                return Ok(None);
            }
        }
        Ok(Some(manifests.into_iter().map(ProjectManifest::Foundry).collect()))
    }

    /// Discover all project manifests at the given paths.
    ///
    /// Returns a `Vec` of discovered [`ProjectManifest`]s, which is guaranteed to be unique and
    /// sorted.
    pub(crate) fn discover_all(
        paths: &[PathBuf],
        policy: &WorkspaceIndexPolicy,
        cancellation: &IndexingCancellation,
        metrics: &mut WorkspaceIndexMetrics,
    ) -> Option<Vec<Self>> {
        let mut discovered = FxHashSet::default();
        for path in paths {
            if cancellation.is_cancelled() {
                return None;
            }
            if let Ok(result) = Self::discover(path, policy, cancellation, metrics) {
                discovered.extend(result?);
            }
        }
        let mut res = discovered.into_iter().collect::<Vec<_>>();
        res.sort();
        Some(res)
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

struct ManifestDiscovery<'a> {
    manifests: &'a mut Vec<PathBuf>,
    policy: &'a WorkspaceIndexPolicy,
    cancellation: &'a IndexingCancellation,
    metrics: &'a mut WorkspaceIndexMetrics,
}

impl ManifestDiscovery<'_> {
    fn find_in_child_dirs(
        &mut self,
        entities: ReadDir,
        within_project: bool,
        workspace_root: &Path,
        traversal_root: &Path,
        import_only_roots: &[PathBuf],
    ) -> bool {
        for entry in entities.filter_map(Result::ok) {
            if self.cancellation.is_cancelled() {
                return false;
            }
            self.metrics.visited += 1;
            let Ok(file_type) = entry.file_type() else { continue };
            let path = entry.path();
            if !file_type.is_dir() {
                continue;
            }
            if import_only_roots.iter().any(|root| path.starts_with(root))
                || self.policy.should_prune_directory(workspace_root, traversal_root, &path)
            {
                self.metrics.pruned += 1;
                continue;
            }

            let manifest = path.join("foundry.toml");
            let is_project = manifest.is_file();
            if is_project {
                self.manifests.push(manifest.clone());
            }
            if (within_project || is_project)
                && let Ok(children) = read_dir(&path)
            {
                let nested_import_only_roots =
                    if is_project { foundry_import_only_roots(&manifest) } else { Vec::new() };
                let (nested_workspace_root, nested_traversal_root, nested_import_only_roots) =
                    if is_project {
                        (path.as_path(), path.as_path(), nested_import_only_roots.as_slice())
                    } else {
                        (workspace_root, traversal_root, import_only_roots)
                    };
                if !self.find_in_child_dirs(
                    children,
                    true,
                    nested_workspace_root,
                    nested_traversal_root,
                    nested_import_only_roots,
                ) {
                    return false;
                }
            }
        }
        true
    }
}

fn foundry_import_only_roots(manifest: &Path) -> Vec<PathBuf> {
    let Some(root) = manifest.parent() else { return Vec::new() };
    load_foundry_document(manifest)
        .map(|document| {
            document.include_paths(root).into_iter().map(|path| path.normalize()).collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{test_support::TestProject, workspace::index_policy::IndexingOptions};

    fn discover_all(paths: &[PathBuf]) -> Vec<ProjectManifest> {
        ProjectManifest::discover_all(
            paths,
            &WorkspaceIndexPolicy::default(),
            &IndexingCancellation::default(),
            &mut WorkspaceIndexMetrics::default(),
        )
        .unwrap()
    }

    #[test]
    fn naked_root_discovery_is_shallow() {
        let project = TestProject::from_fixture(
            r#"
            //- /child/foundry.toml

            //- /container/deep/foundry.toml
            "#,
        );

        assert_eq!(
            discover_all(&[project.root().to_path_buf()]),
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
            discover_all(&[project.root().to_path_buf()]),
            vec![
                ProjectManifest::Foundry(project.path("/foundry.toml")),
                ProjectManifest::Foundry(project.path("/packages/group/vault/foundry.toml")),
                ProjectManifest::Foundry(project.path("/packages/token/foundry.toml")),
            ],
        );
    }

    #[test]
    fn root_project_skips_nested_repository_boundaries() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml

            //- /nested/.git
            gitdir: elsewhere

            //- /nested/foundry.toml
            "#,
        );

        assert_eq!(
            discover_all(&[project.root().to_path_buf()]),
            vec![ProjectManifest::Foundry(project.path("/foundry.toml"))],
        );
        assert_eq!(
            discover_all(&[project.path("/nested")]),
            vec![ProjectManifest::Foundry(project.path("/nested/foundry.toml"))],
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
            discover_all(std::slice::from_ref(&child)),
            vec![ProjectManifest::Foundry(child.join("foundry.toml"))],
        );
    }

    #[test]
    fn configured_libraries_remain_import_only_when_default_excludes_are_disabled() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [profile.default]
            libs = ["vendor"]

            //- /vendor/dependency/foundry.toml

            //- /packages/app/foundry.toml
            "#,
        );
        let policy = WorkspaceIndexPolicy::new(IndexingOptions {
            use_default_excludes: false,
            ..Default::default()
        });
        let discovered = ProjectManifest::discover_all(
            &[project.root().to_path_buf()],
            &policy,
            &IndexingCancellation::default(),
            &mut WorkspaceIndexMetrics::default(),
        )
        .unwrap();

        assert_eq!(
            discovered,
            vec![
                ProjectManifest::Foundry(project.path("/foundry.toml")),
                ProjectManifest::Foundry(project.path("/packages/app/foundry.toml")),
            ]
        );
    }
}
