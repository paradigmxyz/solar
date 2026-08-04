//! Workspace models.
//!
//! Solar LSP supports multiple workspace models that are configured in different ways.
//!
//! This module contains a generic workspace concept, as well as implementations of different
//! project models (e.g. Foundry projects), and a project discovery algorithm to try and determine
//! what kind of project the LSP is dealing with based on different heuristics.
//!
//! Once a project type is identified, the configuration for that project model is merged into the
//! overall LSP config.

use crate::workspace::{
    foundry::FoundryDocument,
    index_policy::{IndexingCancellation, WorkspaceIndexMetrics, WorkspaceIndexPolicy},
};
use normalize_path::NormalizePath;
use solar_config::{CompileOpts, EvmVersion, ImportRemapping};
use solar_interface::source_map::SourceMap;
use std::{
    io,
    path::{Path, PathBuf},
};

mod foundry;
pub(crate) mod index_policy;
pub(crate) mod manifest;

#[derive(Clone, Debug)]
pub(crate) struct Workspace {
    kind: WorkspaceKind,
    compile_opts: CompileOpts,
    source_roots: Vec<PathBuf>,
    source_files: Vec<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorkspaceKind {
    Foundry,
    /// A naked workspace is a workspace with no specific configuration.
    ///
    /// Naked workspaces have no remappings or toolchain-style dependencies, so all imports are
    /// assumed to be relative to the file being parsed.
    Naked,
}

impl Workspace {
    pub(crate) fn naked(root: PathBuf) -> Self {
        let source_roots = vec![root.clone()];
        Self {
            kind: WorkspaceKind::Naked,
            compile_opts: CompileOpts { base_path: Some(root), ..Default::default() },
            source_roots,
            source_files: Vec::new(),
        }
    }

    pub(crate) fn unconfigured() -> Self {
        Self {
            kind: WorkspaceKind::Naked,
            compile_opts: CompileOpts::default(),
            source_roots: Vec::new(),
            source_files: Vec::new(),
        }
    }

    pub(crate) fn kind(&self) -> WorkspaceKind {
        self.kind
    }

    pub(crate) fn compile_opts(&self) -> &CompileOpts {
        &self.compile_opts
    }

    pub(crate) fn source_roots(&self) -> &[PathBuf] {
        &self.source_roots
    }

    pub(crate) fn source_files(&self) -> &[PathBuf] {
        &self.source_files
    }

    pub(crate) fn import_only_roots(&self) -> &[PathBuf] {
        &self.compile_opts.include_paths
    }

    pub(crate) fn refresh_source_files(
        &mut self,
        policy: &WorkspaceIndexPolicy,
        cancellation: &IndexingCancellation,
        metrics: &mut WorkspaceIndexMetrics,
    ) -> bool {
        let mut source_files = Vec::new();
        for root in &self.source_roots {
            let workspace_root = self.compile_opts.base_path.as_deref().unwrap_or(root);
            if !(SourceFileCollector {
                workspace_root,
                source_root: root,
                import_only_roots: self.import_only_roots(),
                policy,
                cancellation,
                metrics,
                files: &mut source_files,
            })
            .collect(root)
            {
                return false;
            }
        }
        source_files.sort_unstable();
        source_files.dedup();
        self.source_files = source_files;
        true
    }

    pub(crate) fn add_source_file(&mut self, policy: &WorkspaceIndexPolicy, path: PathBuf) {
        if !self.tracks_disk_file(policy, &path) {
            return;
        }
        match self.source_files.binary_search(&path) {
            Ok(_) => {}
            Err(pos) => self.source_files.insert(pos, path),
        }
    }

    pub(crate) fn remove_source_file(&mut self, path: &Path) {
        if let Ok(pos) =
            self.source_files.binary_search_by(|candidate| candidate.as_path().cmp(path))
        {
            self.source_files.remove(pos);
        }
    }

    pub(crate) fn tracks_disk_file(&self, policy: &WorkspaceIndexPolicy, path: &Path) -> bool {
        is_solidity_file(path)
            && !self.import_only_roots().iter().any(|root| path.starts_with(root))
            && self.source_roots.iter().any(|root| {
                let workspace_root = self.compile_opts.base_path.as_deref().unwrap_or(root);
                path.starts_with(root) && !policy.excludes_file(workspace_root, root, path)
            })
    }

    pub(crate) fn load_foundry(path: PathBuf) -> Result<Self, WorkspaceError> {
        let root = manifest_root(&path)?;
        let profile = load_foundry_document(&path)?.default_profile();
        let source_roots =
            profile.source_roots(&root).into_iter().map(|path| path.normalize()).collect();
        let import_only_roots = profile
            .include_paths(&root)
            .into_iter()
            .map(|path| path.normalize())
            .collect::<Vec<_>>();
        let compile_opts = compile_opts(
            root.clone(),
            import_only_roots,
            profile.remappings(&root),
            profile.evm_version(),
        );

        Ok(Self {
            kind: WorkspaceKind::Foundry,
            source_roots,
            compile_opts,
            source_files: Vec::new(),
        })
    }
}

pub(crate) struct WorkspacePathIndex<'a> {
    workspaces: &'a [Workspace],
    entries: Vec<WorkspacePathIndexEntry<'a>>,
}

struct WorkspacePathIndexEntry<'a> {
    idx: usize,
    base_path: &'a Path,
    depth: usize,
}

impl<'a> WorkspacePathIndex<'a> {
    pub(crate) fn new(workspaces: &'a [Workspace]) -> Self {
        let mut entries = workspaces
            .iter()
            .enumerate()
            .filter_map(|(idx, workspace)| {
                let base_path = workspace.compile_opts().base_path.as_deref()?;
                Some(WorkspacePathIndexEntry {
                    idx,
                    base_path,
                    depth: base_path.components().count(),
                })
            })
            .collect::<Vec<_>>();
        entries.sort_by(|lhs, rhs| rhs.depth.cmp(&lhs.depth).then_with(|| rhs.idx.cmp(&lhs.idx)));
        Self { workspaces, entries }
    }

    pub(crate) fn workspace_idx_for_path(&self, path: &Path) -> usize {
        self.workspace_idx_containing_path(path).unwrap_or(0)
    }

    pub(crate) fn workspace_idx_containing_path(&self, path: &Path) -> Option<usize> {
        self.entries.iter().find(|entry| path.starts_with(entry.base_path)).map(|entry| entry.idx)
    }

    /// Returns the owning workspace when `path` is an active disk source under its policy.
    ///
    /// The most specific base path owns paths inside it even when its policy rejects them. Source
    /// roots are used as ownership boundaries only for paths outside every workspace base path.
    pub(crate) fn workspace_idx_for_source_path(
        &self,
        policy: &WorkspaceIndexPolicy,
        path: &Path,
    ) -> Option<usize> {
        let idx = self.workspace_idx_containing_path(path).or_else(|| {
            self.workspaces
                .iter()
                .enumerate()
                .filter_map(|(idx, workspace)| {
                    let source_depth = workspace
                        .source_roots()
                        .iter()
                        .filter(|root| path.starts_with(root))
                        .map(|root| root.components().count())
                        .max()?;
                    let base_depth = workspace
                        .compile_opts()
                        .base_path
                        .as_deref()
                        .map_or(0, |base_path| base_path.components().count());
                    Some((idx, source_depth, base_depth))
                })
                .max_by_key(|&(idx, source_depth, base_depth)| (source_depth, base_depth, idx))
                .map(|(idx, _, _)| idx)
        })?;
        self.workspaces[idx].tracks_disk_file(policy, path).then_some(idx)
    }

    pub(crate) fn reconcile_source_files(
        workspaces: &mut [Workspace],
        policy: &WorkspaceIndexPolicy,
        metrics: &mut WorkspaceIndexMetrics,
    ) {
        let mut candidates = workspaces
            .iter_mut()
            .flat_map(|workspace| std::mem::take(&mut workspace.source_files))
            .collect::<Vec<_>>();
        candidates.sort_unstable();
        candidates.dedup();

        let mut source_files = (0..workspaces.len()).map(|_| Vec::new()).collect::<Vec<_>>();
        {
            let index = WorkspacePathIndex::new(&*workspaces);
            for path in candidates {
                if let Some(idx) = index.workspace_idx_for_source_path(policy, &path) {
                    source_files[idx].push(path);
                }
            }
        }
        metrics.eager = source_files.iter().map(Vec::len).sum();
        for (workspace, source_files) in workspaces.iter_mut().zip(source_files) {
            workspace.source_files = source_files;
        }
    }
}

struct SourceFileCollector<'a> {
    workspace_root: &'a Path,
    source_root: &'a Path,
    import_only_roots: &'a [PathBuf],
    policy: &'a WorkspaceIndexPolicy,
    cancellation: &'a IndexingCancellation,
    metrics: &'a mut WorkspaceIndexMetrics,
    files: &'a mut Vec<PathBuf>,
}

impl SourceFileCollector<'_> {
    fn collect(&mut self, path: &Path) -> bool {
        if self.cancellation.is_cancelled() {
            return false;
        }
        self.metrics.visited += 1;
        if self.import_only_roots.iter().any(|root| path.starts_with(root)) {
            self.metrics.pruned += 1;
            return true;
        }
        let Ok(metadata) = std::fs::symlink_metadata(path) else {
            return true;
        };
        if metadata.is_file() {
            if !is_solidity_file(path) {
                return true;
            }
            if self.policy.excludes_file(self.workspace_root, self.source_root, path) {
                self.metrics.pruned += 1;
            } else {
                self.files.push(path.to_path_buf());
                self.metrics.eager += 1;
            }
            return true;
        }
        if metadata.is_dir() {
            if self.policy.should_prune_directory(self.workspace_root, self.source_root, path) {
                self.metrics.pruned += 1;
                return true;
            }
            let Ok(entries) = std::fs::read_dir(path) else {
                return true;
            };
            for entry in entries.filter_map(Result::ok) {
                if !self.collect(&entry.path()) {
                    return false;
                }
            }
        }
        true
    }
}

fn is_solidity_file(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension == "sol")
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum WorkspaceError {
    #[error("workspace manifest `{}` has no parent directory", .0.display())]
    MissingManifestParent(PathBuf),
    #[error("failed to read workspace manifest `{}`: {source}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse workspace manifest `{}`: {source}", path.display())]
    ParseToml {
        path: PathBuf,
        #[source]
        source: toml_edit::de::Error,
    },
}

fn manifest_root(path: &Path) -> Result<PathBuf, WorkspaceError> {
    path.parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| WorkspaceError::MissingManifestParent(path.to_path_buf()))
}

fn compile_opts(
    base_path: PathBuf,
    include_paths: Vec<PathBuf>,
    import_remappings: Vec<ImportRemapping>,
    evm_version: Option<EvmVersion>,
) -> CompileOpts {
    let mut opts = CompileOpts {
        base_path: Some(base_path),
        include_paths,
        import_remappings,
        ..Default::default()
    };
    if let Some(evm_version) = evm_version {
        opts.evm_version = evm_version;
    }
    opts
}

fn load_foundry_document(path: &Path) -> Result<FoundryDocument, WorkspaceError> {
    let source_map = SourceMap::empty();
    let contents = source_map
        .file_loader()
        .load_file(path)
        .map_err(|source| WorkspaceError::Read { path: path.to_path_buf(), source })?;
    toml_edit::de::from_str(&contents)
        .map_err(|source| WorkspaceError::ParseToml { path: path.to_path_buf(), source })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{test_support::TestProject, workspace::index_policy::IndexingOptions};
    use solar_config::EvmVersion;

    fn refresh_source_files(workspace: &mut Workspace) {
        let mut metrics = WorkspaceIndexMetrics::default();
        assert!(workspace.refresh_source_files(
            &WorkspaceIndexPolicy::default(),
            &IndexingCancellation::default(),
            &mut metrics,
        ));
    }

    #[test]
    fn foundry_workspace_loads_manifest_compile_config() {
        let project = TestProject::from_fixture(
            r#"
            //- /lib/forge-std/src/Test.sol
            contract Test {}

            //- /vendor/ds-test/src/Test.sol
            contract Test {}

            //- /remappings.txt
            solmate/=lib/solmate/src/

            //- /foundry.toml
            [profile.default]
            src = "contracts"
            libs = ["lib", "vendor"]
            evm_version = "cancun"
            remappings = [
                "@oz=lib/openzeppelin-contracts/contracts/",
                "ds-test=lib/ds-test/src/",
            ]
            "#,
        );

        let workspace = Workspace::load_foundry(project.path("/foundry.toml")).unwrap();
        let opts = workspace.compile_opts();

        assert_eq!(opts.base_path.as_deref(), Some(project.root()));
        assert_eq!(opts.include_paths, vec![project.path("/lib"), project.path("/vendor")]);
        assert_eq!(opts.evm_version, EvmVersion::Cancun);
        assert_eq!(
            opts.import_remappings.iter().map(ToString::to_string).collect::<Vec<_>>(),
            vec![
                "ds-test/=vendor/ds-test/src/",
                "forge-std/=lib/forge-std/src/",
                "solmate/=lib/solmate/src/",
                "@oz=lib/openzeppelin-contracts/contracts/",
                "ds-test=lib/ds-test/src/",
            ]
        );
        assert_eq!(workspace.source_roots(), &[project.path("/contracts")]);
    }

    #[test]
    fn foundry_workspace_respects_disabled_auto_detect_remappings() {
        let project = TestProject::from_fixture(
            r#"
            //- /lib/forge-std/src/Test.sol
            contract Test {}

            //- /remappings.txt
            solmate/=lib/solmate/src/

            //- /foundry.toml
            [profile.default]
            auto_detect_remappings = false
            remappings = ["@oz=lib/openzeppelin-contracts/contracts/"]
            "#,
        );

        let workspace = Workspace::load_foundry(project.path("/foundry.toml")).unwrap();
        let opts = workspace.compile_opts();

        assert_eq!(
            opts.import_remappings.iter().map(ToString::to_string).collect::<Vec<_>>(),
            vec!["solmate/=lib/solmate/src/", "@oz=lib/openzeppelin-contracts/contracts/"]
        );
    }

    #[test]
    fn foundry_root_source_does_not_eagerly_index_configured_libraries() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [profile.default]
            src = "."
            libs = ["vendor"]

            //- /Main.sol
            import "vendor/Dependency.sol";
            contract Main is Dependency {}

            //- /vendor/Dependency.sol
            contract Dependency {}
            "#,
        );

        let mut workspace = Workspace::load_foundry(project.path("/foundry.toml")).unwrap();
        refresh_source_files(&mut workspace);

        assert_eq!(workspace.source_files(), &[project.path("/Main.sol")]);
    }

    #[test]
    fn workspace_path_index_uses_most_specific_base_path() {
        let project = TestProject::new();
        let nested = project.path("/nested");

        let outer = Workspace::naked(project.root().to_path_buf());
        let inner = Workspace::naked(nested);
        let workspaces = vec![outer, inner];
        let index = WorkspacePathIndex::new(&workspaces);

        assert_eq!(index.workspace_idx_for_path(&project.path("/nested/A.sol")), 1);
        assert_eq!(index.workspace_idx_for_path(&project.path("/B.sol")), 0);
    }

    #[test]
    fn workspace_path_index_finds_external_source_roots() {
        let project = TestProject::from_fixture(
            r#"
            //- /project/foundry.toml
            [profile.default]
            src = "../shared"

            //- /shared/External.sol
            contract External {}
            "#,
        );
        let workspaces = vec![
            Workspace::naked(project.path("/unrelated")),
            Workspace::load_foundry(project.path("/project/foundry.toml")).unwrap(),
        ];
        let index = WorkspacePathIndex::new(&workspaces);

        assert_eq!(
            index.workspace_idx_for_source_path(
                &WorkspaceIndexPolicy::default(),
                &project.path("/shared/External.sol"),
            ),
            Some(1)
        );
    }

    #[test]
    fn workspace_path_index_does_not_bypass_nested_workspace_policy() {
        let project = TestProject::from_fixture(
            r#"
            //- /nested/foundry.toml
            [profile.default]
            src = "."
            libs = ["vendor"]

            //- /nested/Included.sol
            contract Included {}

            //- /nested/generated/Excluded.sol
            contract Excluded {}

            //- /nested/vendor/Dependency.sol
            contract Dependency {}
            "#,
        );
        let workspaces = vec![
            Workspace::naked(project.root().to_path_buf()),
            Workspace::load_foundry(project.path("/nested/foundry.toml")).unwrap(),
        ];
        let index = WorkspacePathIndex::new(&workspaces);
        let policy = WorkspaceIndexPolicy::new(IndexingOptions {
            exclude: vec!["generated/**".into()],
            ..Default::default()
        });

        assert_eq!(
            index.workspace_idx_for_source_path(&policy, &project.path("/nested/Included.sol")),
            Some(1)
        );
        assert_eq!(
            index.workspace_idx_for_source_path(
                &policy,
                &project.path("/nested/generated/Excluded.sol"),
            ),
            None
        );
        assert_eq!(
            index.workspace_idx_for_source_path(
                &policy,
                &project.path("/nested/vendor/Dependency.sol"),
            ),
            None
        );
    }

    #[test]
    fn workspace_path_index_reconciles_cached_sources_with_deepest_workspace() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [profile.default]
            src = "."

            //- /Outer.sol
            contract Outer {}

            //- /nested/foundry.toml
            [profile.default]
            src = "src"

            //- /nested/src/Owned.sol
            contract Owned {}

            //- /nested/Rejected.sol
            contract Rejected {}
            "#,
        );
        let mut workspaces = vec![
            Workspace::load_foundry(project.path("/foundry.toml")).unwrap(),
            Workspace::load_foundry(project.path("/nested/foundry.toml")).unwrap(),
        ];
        let policy = WorkspaceIndexPolicy::default();
        let mut metrics = WorkspaceIndexMetrics::default();
        assert!(workspaces[0].refresh_source_files(
            &policy,
            &IndexingCancellation::default(),
            &mut metrics,
        ));

        WorkspacePathIndex::reconcile_source_files(&mut workspaces, &policy, &mut metrics);

        assert_eq!(workspaces[0].source_files(), &[project.path("/Outer.sol")]);
        assert_eq!(workspaces[1].source_files(), &[project.path("/nested/src/Owned.sol")]);
        assert_eq!(metrics.eager, 2);
    }

    #[test]
    fn naked_workspace_collects_disk_source_files_and_skips_heavy_dirs() {
        let project = TestProject::new();
        project.write_file("/src/A.sol", "contract A {}");
        for dir in [".git", "cache", "lib", "node_modules", "out", "target"] {
            project.write_file(&format!("/{dir}/Ignored.sol"), "contract Ignored {}");
        }
        project.write_file("/nested/.git", "gitdir: elsewhere");
        project.write_file("/nested/Ignored.sol", "contract Ignored {}");

        let mut workspace = Workspace::naked(project.root().to_path_buf());
        refresh_source_files(&mut workspace);

        assert_eq!(workspace.source_files(), &[project.path("/src/A.sol")]);
    }

    #[test]
    fn naked_workspace_adds_created_disk_source_files_outside_heavy_dirs() {
        let project = TestProject::from_fixture(
            r#"
            //- /src/A.sol
            contract A {}

            //- /node_modules/Ignored.sol
            contract Ignored {}

            //- /nested/.git
            gitdir: elsewhere

            //- /nested/Ignored.sol
            contract Ignored {}
            "#,
        );

        let mut workspace = Workspace::naked(project.root().to_path_buf());
        let policy = WorkspaceIndexPolicy::default();
        workspace.add_source_file(&policy, project.path("/src/A.sol"));
        workspace.add_source_file(&policy, project.path("/node_modules/Ignored.sol"));
        workspace.add_source_file(&policy, project.path("/nested/Ignored.sol"));

        assert_eq!(workspace.source_files(), &[project.path("/src/A.sol")]);
    }

    #[test]
    fn source_traversal_honors_switches_custom_globs_and_nested_repositories() {
        let project = TestProject::from_fixture(
            r#"
            //- /src/Included.sol
            contract Included {}

            //- /build/IncludedWhenDefaultsDisabled.sol
            contract IncludedWhenDefaultsDisabled {}

            //- /.hidden/IncludedWhenHiddenDisabled.sol
            contract IncludedWhenHiddenDisabled {}

            //- /generated/Excluded.sol
            contract Excluded {}

            //- /nested/.git
            gitdir: elsewhere

            //- /nested/ExcludedRepository.sol
            contract ExcludedRepository {}
            "#,
        );
        let policy = WorkspaceIndexPolicy::new(IndexingOptions {
            exclude: vec!["generated/**".into()],
            use_default_excludes: false,
            exclude_hidden_directories: false,
            ..Default::default()
        });
        let mut workspace = Workspace::naked(project.root().to_path_buf());
        let mut metrics = WorkspaceIndexMetrics::default();

        assert!(workspace.refresh_source_files(
            &policy,
            &IndexingCancellation::default(),
            &mut metrics,
        ));

        assert_eq!(
            workspace.source_files(),
            &[
                project.path("/.hidden/IncludedWhenHiddenDisabled.sol"),
                project.path("/build/IncludedWhenDefaultsDisabled.sol"),
                project.path("/src/Included.sol"),
            ]
        );
        assert_eq!(metrics.eager, 3);
        assert_eq!(metrics.pruned, 2);
    }

    #[test]
    fn source_refresh_honors_file_exclude_globs() {
        let project = TestProject::from_fixture(
            r#"
            //- /src/Included.sol
            contract Included {}

            //- /src/Only.generated.sol
            contract Only {}
            "#,
        );
        let policy = WorkspaceIndexPolicy::new(IndexingOptions {
            exclude: vec!["**/*.generated.sol".into()],
            ..Default::default()
        });
        let mut workspace = Workspace::naked(project.root().to_path_buf());
        let mut metrics = WorkspaceIndexMetrics::default();

        assert!(workspace.refresh_source_files(
            &policy,
            &IndexingCancellation::default(),
            &mut metrics,
        ));

        assert_eq!(workspace.source_files(), &[project.path("/src/Included.sol")]);
        assert_eq!(metrics.eager, 1);
        assert_eq!(metrics.pruned, 1);
    }

    #[test]
    fn built_in_rules_exempt_explicit_source_roots_but_custom_rules_do_not() {
        let project = TestProject::from_fixture(
            r#"
            //- /node_modules/project/foundry.toml
            [profile.default]
            src = "generated"

            //- /node_modules/project/generated/Main.sol
            contract Main {}
            "#,
        );
        let mut workspace =
            Workspace::load_foundry(project.path("/node_modules/project/foundry.toml")).unwrap();
        let mut metrics = WorkspaceIndexMetrics::default();
        assert!(workspace.refresh_source_files(
            &WorkspaceIndexPolicy::default(),
            &IndexingCancellation::default(),
            &mut metrics,
        ));
        assert_eq!(
            workspace.source_files(),
            &[project.path("/node_modules/project/generated/Main.sol")]
        );

        let policy = WorkspaceIndexPolicy::new(IndexingOptions {
            exclude: vec!["generated/**".into()],
            ..Default::default()
        });
        assert!(workspace.refresh_source_files(
            &policy,
            &IndexingCancellation::default(),
            &mut WorkspaceIndexMetrics::default(),
        ));
        assert!(workspace.source_files().is_empty());
    }

    #[test]
    fn cancelled_source_refresh_does_not_commit_partial_results() {
        let project = TestProject::from_fixture(
            r#"
            //- /src/Before.sol
            contract Before {}
            "#,
        );
        let mut workspace = Workspace::naked(project.root().to_path_buf());
        refresh_source_files(&mut workspace);
        project.write_file("/src/After.sol", "contract After {}");
        let cancellation = IndexingCancellation::default();
        cancellation.cancel();

        assert!(!workspace.refresh_source_files(
            &WorkspaceIndexPolicy::default(),
            &cancellation,
            &mut WorkspaceIndexMetrics::default(),
        ));
        assert_eq!(workspace.source_files(), &[project.path("/src/Before.sol")]);
    }
}
