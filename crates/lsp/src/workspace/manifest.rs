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
        foundry_config: &mut FoundryConfigContext<'_>,
    ) -> io::Result<Option<ManifestDiscoveryResult>> {
        // Keep naked roots shallow, but recurse once a Foundry project boundary is known.
        let manifest = find_in_parent_dirs(path, "foundry.toml");
        let (workspace_root, (source_roots, import_only_roots)) = match &manifest {
            Some(manifest) => (
                manifest.parent().unwrap_or(path).to_path_buf(),
                foundry_index_roots(manifest, approved_roots, foundry_config),
            ),
            None => (path.to_path_buf(), Default::default()),
        };
        let within_project = manifest.is_some();
        let entries = match read_dir(path) {
            Ok(entries) => Some(entries),
            Err(_) if within_project => None,
            Err(error) => return Err(error),
        };
        let mut manifests = Vec::from_iter(manifest);
        let mut watch_roots = Vec::new();
        let mut marker_watch_roots = Vec::new();
        if let Some(entries) = entries
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
                        within_project,
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
    pub(crate) fn discover_all_with_watch_roots(
        paths: &[PathBuf],
        approved_roots: &[PathBuf],
        policy: &WorkspaceIndexPolicy,
        cancellation: &IndexingCancellation,
        metrics: &mut WorkspaceIndexMetrics,
        foundry_config: &mut FoundryConfigContext<'_>,
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

    path.ancestors()
        .map(|path| path.join(target_file_name))
        .find(|candidate| std::fs::metadata(candidate).is_ok())
}

struct ManifestDiscovery<'a, 'config> {
    manifests: &'a mut Vec<PathBuf>,
    approved_roots: &'a [PathBuf],
    watch_roots: &'a mut Vec<SourceWatchRoot>,
    marker_watch_roots: &'a mut Vec<PathBuf>,
    policy: &'a WorkspaceIndexPolicy,
    cancellation: &'a IndexingCancellation,
    metrics: &'a mut WorkspaceIndexMetrics,
    foundry_config: &'a mut FoundryConfigContext<'config>,
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

impl ManifestDiscovery<'_, '_> {
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
                self.policy.excludes_source_directory(workspace_root, traversal_root, &path)
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
                let nested_index_roots;
                let nested = ManifestTraversal {
                    within_project: true,
                    watch_root: &path,
                    corridor_only: source_corridor,
                    ..traversal
                };
                let nested = if is_project {
                    nested_index_roots =
                        foundry_index_roots(&manifest, self.approved_roots, self.foundry_config);
                    ManifestTraversal {
                        workspace_root: &path,
                        traversal_root: &path,
                        source_roots: &nested_index_roots.0,
                        import_only_roots: &nested_index_roots.1,
                        ..nested
                    }
                } else {
                    nested
                };
                child_state = self.find_in_child_dirs(children, nested);
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
    foundry_config: &mut FoundryConfigContext<'_>,
) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let Some(root) = manifest.parent() else { return Default::default() };
    let Ok(host_config) = foundry_config.workspace_config(root) else {
        return Default::default();
    };
    let (source_roots, import_only_roots) = if let Some(config) = host_config {
        (config.source_roots().to_vec(), config.include_paths().to_vec())
    } else {
        let Ok(document) = load_foundry_document(manifest) else { return Default::default() };
        let profile = document.profile_for(foundry_config.selected_profile());
        let source_roots = profile.build_source_roots(root);
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

    fn discover(
        project: &TestProject,
        root: &str,
        options: IndexingOptions,
        profile: Option<&str>,
    ) -> Vec<ProjectManifest> {
        let paths = [project.path(root)];
        ProjectManifest::discover_all_with_watch_roots(
            &paths,
            &paths,
            &WorkspaceIndexPolicy::new(options),
            &IndexingCancellation::default(),
            &mut WorkspaceIndexMetrics::default(),
            &mut FoundryConfigContext::new(profile, &[]),
        )
        .unwrap()
        .0
    }

    fn manifests(project: &TestProject, paths: &[&str]) -> Vec<ProjectManifest> {
        paths.iter().map(|path| ProjectManifest::Foundry(project.path(path))).collect()
    }

    #[test]
    fn discovers_manifests_within_index_boundaries() {
        let nested_repository = r#"
            //- /foundry.toml

            //- /nested/.git
            gitdir: elsewhere

            //- /nested/foundry.toml
            "#;
        for (fixture, root, expected) in [
            // Naked roots stay shallow.
            (
                r#"
                //- /child/foundry.toml

                //- /container/deep/foundry.toml
                "#,
                "/",
                &["/child/foundry.toml"][..],
            ),
            // Root projects recurse but skip heavy directories.
            (
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
                "/",
                &[
                    "/foundry.toml",
                    "/packages/group/vault/foundry.toml",
                    "/packages/token/foundry.toml",
                ],
            ),
            // Nested repositories are boundaries unless they are the root.
            (nested_repository, "/", &["/foundry.toml"]),
            (nested_repository, "/nested", &["/nested/foundry.toml"]),
            // Parent discovery prefers the nearest manifest.
            (
                r#"
                //- /foundry.toml

                //- /child/foundry.toml
                "#,
                "/child",
                &["/child/foundry.toml"],
            ),
            // A source root inside a library only opens its own corridor.
            (
                r#"
                //- /foundry.toml
                [profile.default]
                src = "lib/contracts"

                //- /lib/contracts/nested/foundry.toml

                //- /lib/dependency/foundry.toml
                "#,
                "/",
                &["/foundry.toml", "/lib/contracts/nested/foundry.toml"],
            ),
            // Source roots still skip default-excluded descendants.
            (
                r#"
                //- /foundry.toml
                [profile.default]
                src = "src"

                //- /src/nested/foundry.toml

                //- /src/node_modules/dependency/foundry.toml
                "#,
                "/",
                &["/foundry.toml", "/src/nested/foundry.toml"],
            ),
            // An import-only corridor does not admit its ancestor manifest.
            (
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
                "/",
                &["/foundry.toml"],
            ),
            // A source corridor does not admit excluded sibling manifests.
            (
                r#"
                //- /foundry.toml
                [profile.default]
                src = ".hidden/contracts"

                //- /.hidden/contracts/nested/foundry.toml

                //- /.hidden/sibling/foundry.toml
                "#,
                "/",
                &["/.hidden/contracts/nested/foundry.toml", "/foundry.toml"],
            ),
        ] {
            let project = TestProject::from_fixture(fixture);
            assert_eq!(
                discover(&project, root, IndexingOptions::default(), None),
                manifests(&project, expected),
                "{root} in {fixture}"
            );
        }
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
        let options = IndexingOptions { use_default_excludes: false, ..Default::default() };

        assert_eq!(
            discover(&project, "/", options, None),
            manifests(&project, &["/foundry.toml", "/packages/app/foundry.toml"])
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
                &mut FoundryConfigContext::new(Some("custom"), &[]),
            )
            .0,
            [project.path("/.hidden/custom-src"), project.path("/test"), project.path("/script")]
        );
        assert_eq!(
            discover(&project, "/", IndexingOptions::default(), Some("custom")),
            manifests(
                &project,
                &[
                    "/.hidden/custom-src/nested/.hidden/custom-src/deep/foundry.toml",
                    "/.hidden/custom-src/nested/foundry.toml",
                    "/foundry.toml",
                ]
            )
        );
    }

    #[test]
    fn host_foundry_index_roots_keep_approved_boundaries() {
        let project = TestProject::new();
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
                &mut FoundryConfigContext::new(None, &configs),
            ),
            (vec![project.path("/workspace/host-src")], vec![project.path("/workspace/host-lib")],)
        );
    }
}
