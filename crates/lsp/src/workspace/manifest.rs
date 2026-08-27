use std::{
    fs::{ReadDir, read_dir},
    path::{Path, PathBuf},
};

use super::{
    FoundryConfigContext, SourceWatchRoot,
    index_policy::{IndexingCancellation, WorkspaceIndexMetrics, WorkspaceIndexPolicy},
    is_approved_index_root, is_import_only_path, load_foundry_document,
};
use normalize_path::NormalizePath;
use solar_interface::data_structures::map::rustc_hash::FxHashSet;
use tokio::io;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub(crate) enum ProjectManifest {
    // todo: guarantee this to be absolute
    Foundry(PathBuf),
}

type ManifestDiscoveryResult = (Vec<ProjectManifest>, Vec<SourceWatchRoot>, Vec<PathBuf>);

impl ProjectManifest {
    pub(crate) fn discover_in_parents(path: &Path) -> Option<Self> {
        find_in_parent_dirs(path, "foundry.toml").map(Self::Foundry)
    }

    fn discover(
        path: &Path,
        approved_roots: &[PathBuf],
        policy: &WorkspaceIndexPolicy,
        cancellation: &IndexingCancellation,
        metrics: &mut WorkspaceIndexMetrics,
        foundry_config: FoundryConfigContext<'_>,
    ) -> io::Result<Option<ManifestDiscoveryResult>> {
        // Keep naked roots shallow, but recurse once a Foundry project boundary is known.
        let mut manifests = Vec::new();
        let mut watch_roots = Vec::new();
        let mut marker_watch_roots = Vec::new();
        if let Some(manifest) = find_in_parent_dirs(path, "foundry.toml") {
            let workspace_root = manifest.parent().unwrap_or(path).to_path_buf();
            let (source_roots, import_only_roots) =
                foundry_index_roots(&manifest, approved_roots, foundry_config);
            manifests.push(manifest);
            if let Ok(entries) = read_dir(path)
                && matches!(
                    (ManifestDiscovery {
                        manifests: &mut manifests,
                        approved_roots,
                        watch_roots: &mut watch_roots,
                        marker_watch_roots: &mut marker_watch_roots,
                        policy,
                        cancellation,
                        metrics,
                        foundry_config,
                    })
                    .find_in_child_dirs(
                        entries,
                        ManifestTraversal {
                            within_project: true,
                            workspace_root: &workspace_root,
                            traversal_root: path,
                            watch_root: path,
                            source_roots: &source_roots,
                            import_only_roots: &import_only_roots,
                            corridor_only: false,
                        },
                    ),
                    ManifestTreeState::Cancelled
                )
            {
                return Ok(None);
            }
        } else {
            if matches!(
                (ManifestDiscovery {
                    manifests: &mut manifests,
                    approved_roots,
                    watch_roots: &mut watch_roots,
                    marker_watch_roots: &mut marker_watch_roots,
                    policy,
                    cancellation,
                    metrics,
                    foundry_config,
                })
                .find_in_child_dirs(
                    read_dir(path)?,
                    ManifestTraversal {
                        within_project: false,
                        workspace_root: path,
                        traversal_root: path,
                        watch_root: path,
                        source_roots: &[],
                        import_only_roots: &[],
                        corridor_only: false,
                    },
                ),
                ManifestTreeState::Cancelled
            ) {
                return Ok(None);
            }
        }
        Ok(Some((
            manifests.into_iter().map(ProjectManifest::Foundry).collect(),
            watch_roots,
            marker_watch_roots,
        )))
    }

    /// Discover all project manifests at the given paths.
    ///
    /// Returns a `Vec` of discovered [`ProjectManifest`]s, which is guaranteed to be unique and
    /// sorted.
    #[cfg(test)]
    pub(crate) fn discover_all(
        paths: &[PathBuf],
        policy: &WorkspaceIndexPolicy,
        cancellation: &IndexingCancellation,
        metrics: &mut WorkspaceIndexMetrics,
    ) -> Option<Vec<Self>> {
        Self::discover_all_with_watch_roots(
            paths,
            paths,
            policy,
            cancellation,
            metrics,
            FoundryConfigContext::default(),
        )
        .map(|(manifests, _, _)| manifests)
    }

    pub(crate) fn discover_all_with_watch_roots(
        paths: &[PathBuf],
        approved_roots: &[PathBuf],
        policy: &WorkspaceIndexPolicy,
        cancellation: &IndexingCancellation,
        metrics: &mut WorkspaceIndexMetrics,
        foundry_config: FoundryConfigContext<'_>,
    ) -> Option<ManifestDiscoveryResult> {
        let mut discovered = FxHashSet::default();
        let mut watch_roots = Vec::new();
        let mut marker_watch_roots = Vec::new();
        for path in paths {
            if cancellation.is_cancelled() {
                return None;
            }
            if let Ok(result) =
                Self::discover(path, approved_roots, policy, cancellation, metrics, foundry_config)
            {
                let (manifests, mut roots, mut marker_roots) = result?;
                discovered.extend(manifests);
                watch_roots.append(&mut roots);
                marker_watch_roots.append(&mut marker_roots);
            }
        }
        let mut res = discovered.into_iter().collect::<Vec<_>>();
        res.sort();
        watch_roots.sort_unstable();
        watch_roots.dedup();
        marker_watch_roots.sort_unstable();
        marker_watch_roots.dedup();
        Some((res, watch_roots, marker_watch_roots))
    }

    /// Discovers project boundaries inside source regions already approved for recursive watching.
    ///
    /// This does not create workspaces for empty roots. It stops at each manifest so the caller can
    /// load that workspace, rebuild ownership and watch partitions, and then continue discovery.
    pub(crate) fn discover_in_source_watch_roots(
        roots: &[SourceWatchRoot],
        cancellation: &IndexingCancellation,
        metrics: &mut WorkspaceIndexMetrics,
    ) -> Option<Vec<Self>> {
        let mut discovered = FxHashSet::default();
        for root in roots {
            if cancellation.is_cancelled() {
                return None;
            }
            let manifest = root.path.join("foundry.toml");
            if manifest.is_file() {
                discovered.insert(Self::Foundry(manifest));
                continue;
            }
            if root.recursive
                && !find_in_recursive_watch_root(&root.path, &mut discovered, cancellation, metrics)
            {
                return None;
            }
        }
        let mut manifests = discovered.into_iter().collect::<Vec<_>>();
        manifests.sort_unstable();
        Some(manifests)
    }
}

fn find_in_recursive_watch_root(
    directory: &Path,
    manifests: &mut FxHashSet<ProjectManifest>,
    cancellation: &IndexingCancellation,
    metrics: &mut WorkspaceIndexMetrics,
) -> bool {
    let Ok(entries) = read_dir(directory) else { return true };
    for entry in entries.filter_map(Result::ok) {
        if cancellation.is_cancelled() {
            return false;
        }
        metrics.visited += 1;
        let Ok(file_type) = entry.file_type() else { continue };
        if !file_type.is_dir() {
            continue;
        }
        let path = entry.path();
        let manifest = path.join("foundry.toml");
        if manifest.is_file() {
            manifests.insert(ProjectManifest::Foundry(manifest));
        } else if !find_in_recursive_watch_root(&path, manifests, cancellation, metrics) {
            return false;
        }
    }
    true
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
    approved_roots: &'a [PathBuf],
    watch_roots: &'a mut Vec<SourceWatchRoot>,
    marker_watch_roots: &'a mut Vec<PathBuf>,
    policy: &'a WorkspaceIndexPolicy,
    cancellation: &'a IndexingCancellation,
    metrics: &'a mut WorkspaceIndexMetrics,
    foundry_config: FoundryConfigContext<'a>,
}

#[derive(Clone, Copy)]
struct ManifestTraversal<'a> {
    within_project: bool,
    workspace_root: &'a Path,
    traversal_root: &'a Path,
    watch_root: &'a Path,
    source_roots: &'a [PathBuf],
    import_only_roots: &'a [PathBuf],
    corridor_only: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ManifestTreeState {
    Clean,
    Partitioned,
    Cancelled,
}

impl ManifestDiscovery<'_> {
    fn find_in_child_dirs(
        &mut self,
        entities: ReadDir,
        traversal: ManifestTraversal<'_>,
    ) -> ManifestTreeState {
        let ManifestTraversal {
            within_project,
            workspace_root,
            traversal_root,
            watch_root,
            source_roots,
            import_only_roots,
            corridor_only,
        } = traversal;
        let mut partitioned = false;
        for entry in entities.filter_map(Result::ok) {
            if self.cancellation.is_cancelled() {
                return ManifestTreeState::Cancelled;
            }
            self.metrics.visited += 1;
            let Ok(file_type) = entry.file_type() else { continue };
            let path = entry.path();
            if !file_type.is_dir() {
                continue;
            }
            let source_root = source_roots
                .iter()
                .find(|source_root| path.starts_with(source_root))
                .map(PathBuf::as_path);
            let import_only = is_import_only_path(source_roots, import_only_roots, &path);
            let source_corridor = source_root.is_none()
                && source_roots.iter().any(|source_root| source_root.starts_with(&path));
            let policy_pruned = if source_corridor {
                // A synthetic corridor is open only for custom exclusion checks. Built-in,
                // hidden, and nested-repository rules still apply to its siblings below.
                self.policy.should_prune_directory(workspace_root, &path, &path)
            } else if let Some(source_root) = source_root {
                self.policy.should_prune_source_directory(workspace_root, source_root, &path)
            } else {
                self.policy.excludes_directory(workspace_root, traversal_root, &path)
            };
            if corridor_only && !source_corridor && source_root.is_none()
                || import_only && !source_corridor
                || policy_pruned
            {
                if policy_pruned
                    && let Some(root) = self.policy.nested_repository_marker_root(&path)
                {
                    self.marker_watch_roots.push(root);
                }
                self.metrics.pruned += 1;
                partitioned = true;
                continue;
            }

            let manifest = path.join("foundry.toml");
            let is_project = !source_corridor && !import_only && manifest.is_file();
            if is_project {
                self.manifests.push(manifest.clone());
            }
            let mut child_state = ManifestTreeState::Clean;
            if (within_project || is_project)
                && let Ok(children) = read_dir(&path)
            {
                let nested_index_roots = if is_project {
                    foundry_index_roots(&manifest, self.approved_roots, self.foundry_config)
                } else {
                    Default::default()
                };
                let (
                    nested_workspace_root,
                    nested_traversal_root,
                    nested_source_roots,
                    nested_import_only_roots,
                ) = if is_project {
                    (
                        path.as_path(),
                        path.as_path(),
                        nested_index_roots.0.as_slice(),
                        nested_index_roots.1.as_slice(),
                    )
                } else {
                    (workspace_root, traversal_root, source_roots, import_only_roots)
                };
                child_state = self.find_in_child_dirs(
                    children,
                    ManifestTraversal {
                        within_project: true,
                        workspace_root: nested_workspace_root,
                        traversal_root: nested_traversal_root,
                        watch_root: &path,
                        source_roots: nested_source_roots,
                        import_only_roots: nested_import_only_roots,
                        corridor_only: source_corridor,
                    },
                );
            } else if within_project || is_project {
                self.watch_roots.push(SourceWatchRoot::shallow(path.as_path()));
                child_state = ManifestTreeState::Partitioned;
            } else if !source_corridor {
                // Naked roots intentionally stop at the first directory layer.
                child_state = ManifestTreeState::Partitioned;
            }
            match child_state {
                ManifestTreeState::Clean => {}
                ManifestTreeState::Partitioned => partitioned = true,
                ManifestTreeState::Cancelled => return ManifestTreeState::Cancelled,
            }
        }
        if corridor_only {
            return ManifestTreeState::Partitioned;
        }
        let root = if partitioned {
            SourceWatchRoot::shallow(watch_root)
        } else {
            SourceWatchRoot::recursive(watch_root)
        };
        self.watch_roots.push(root);
        if partitioned { ManifestTreeState::Partitioned } else { ManifestTreeState::Clean }
    }
}

fn foundry_index_roots(
    manifest: &Path,
    approved_roots: &[PathBuf],
    foundry_config: FoundryConfigContext<'_>,
) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let Some(root) = manifest.parent() else { return Default::default() };
    if foundry_config.workspace_config_error(root).is_some() {
        return Default::default();
    }
    let (source_roots, import_only_roots) =
        if let Some(config) = foundry_config.workspace_config(root) {
            (config.source_roots().to_vec(), config.include_paths().to_vec())
        } else {
            let Ok(document) = load_foundry_document(manifest) else { return Default::default() };
            let profile = document.profile_for(foundry_config.selected_profile());
            let source_roots = profile.source_roots(root);
            let import_only_roots =
                profile.include_paths(root).into_iter().map(|path| path.normalize()).collect();
            (source_roots, import_only_roots)
        };
    (
        source_roots
            .into_iter()
            .filter(|path| is_approved_index_root(path, root, approved_roots))
            .collect(),
        import_only_roots
            .into_iter()
            .filter(|path| is_approved_index_root(path, root, approved_roots))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        FoundryWorkspaceConfig, test_support::TestProject, workspace::index_policy::IndexingOptions,
    };

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

    #[test]
    fn source_root_inside_library_only_opens_its_manifest_corridor() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [profile.default]
            src = "lib/contracts"

            //- /lib/contracts/nested/foundry.toml

            //- /lib/dependency/foundry.toml
            "#,
        );

        assert_eq!(
            discover_all(&[project.root().to_path_buf()]),
            vec![
                ProjectManifest::Foundry(project.path("/foundry.toml")),
                ProjectManifest::Foundry(project.path("/lib/contracts/nested/foundry.toml")),
            ]
        );
    }

    #[test]
    fn source_root_discovery_skips_default_excluded_descendants() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [profile.default]
            src = "src"

            //- /src/nested/foundry.toml

            //- /src/node_modules/dependency/foundry.toml
            "#,
        );

        assert_eq!(
            discover_all(&[project.root().to_path_buf()]),
            vec![
                ProjectManifest::Foundry(project.path("/foundry.toml")),
                ProjectManifest::Foundry(project.path("/src/nested/foundry.toml")),
            ]
        );
    }

    #[test]
    fn selected_profile_source_root_controls_nested_manifest_discovery() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [profile.default]
            src = ".hidden/default-src"

            [profile.custom]
            src = ".hidden/custom-src"

            //- /.hidden/default-src/nested/foundry.toml

            //- /.hidden/custom-src/nested/foundry.toml
            [profile.default]
            src = ".hidden/default-src"

            [profile.custom]
            src = ".hidden/custom-src"

            //- /.hidden/custom-src/nested/.hidden/default-src/deep/foundry.toml

            //- /.hidden/custom-src/nested/.hidden/custom-src/deep/foundry.toml
            "#,
        );

        assert_eq!(
            foundry_index_roots(
                &project.path("/foundry.toml"),
                &[project.root().to_path_buf()],
                FoundryConfigContext::new(Some("custom"), &[]),
            )
            .0,
            [project.path("/.hidden/custom-src")]
        );

        let nested_custom_manifest =
            project.path("/.hidden/custom-src/nested/.hidden/custom-src/deep/foundry.toml");
        let paths = [project.root().to_path_buf()];
        let discovered = ProjectManifest::discover_all_with_watch_roots(
            &paths,
            &paths,
            &WorkspaceIndexPolicy::default(),
            &IndexingCancellation::default(),
            &mut WorkspaceIndexMetrics::default(),
            FoundryConfigContext::new(Some("custom"), &[]),
        )
        .unwrap()
        .0;
        assert_eq!(
            discovered,
            vec![
                ProjectManifest::Foundry(nested_custom_manifest),
                ProjectManifest::Foundry(project.path("/.hidden/custom-src/nested/foundry.toml")),
                ProjectManifest::Foundry(project.path("/foundry.toml")),
            ]
        );
    }

    #[test]
    fn host_foundry_index_roots_keep_approved_boundaries() {
        let project = TestProject::from_fixture(
            r#"
            //- /workspace/foundry.toml

            //- /workspace/host-src/Inside.sol
            contract Inside {}

            //- /workspace/host-lib/Dependency.sol
            contract HostDependency {}

            //- /external/src/Outside.sol
            contract Outside {}

            //- /external/lib/Dependency.sol
            contract Dependency {}
            "#,
        );
        let config = FoundryWorkspaceConfig::new(project.path("/workspace"))
            .with_source_roots([project.path("/workspace/host-src"), project.path("/external/src")])
            .with_include_paths([
                project.path("/workspace/host-lib"),
                project.path("/external/lib"),
            ]);
        let configs = [config];

        assert_eq!(
            foundry_index_roots(
                &project.path("/workspace/foundry.toml"),
                &[project.path("/workspace")],
                FoundryConfigContext::new(None, &configs),
            ),
            (vec![project.path("/workspace/host-src")], vec![project.path("/workspace/host-lib")],)
        );
    }

    #[test]
    fn import_only_source_corridor_does_not_admit_ancestor_manifest() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [profile.default]
            src = "lib/contracts"

            //- /lib/foundry.toml
            [profile.default]
            src = "other"

            //- /lib/contracts/Main.sol
            contract Main {}
            "#,
        );

        assert_eq!(
            discover_all(&[project.root().to_path_buf()]),
            vec![ProjectManifest::Foundry(project.path("/foundry.toml"))]
        );
    }

    #[test]
    fn source_corridor_does_not_admit_excluded_sibling_manifests() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [profile.default]
            src = ".hidden/contracts"

            //- /.hidden/contracts/nested/foundry.toml

            //- /.hidden/sibling/foundry.toml
            "#,
        );

        assert_eq!(
            discover_all(&[project.root().to_path_buf()]),
            vec![
                ProjectManifest::Foundry(project.path("/.hidden/contracts/nested/foundry.toml")),
                ProjectManifest::Foundry(project.path("/foundry.toml")),
            ]
        );
    }
}
