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

use crate::{
    FoundryWorkspaceConfig, FoundryWorkspaceConfigLoader, FoundryWorkspaceConfigSource,
    workspace::{
        foundry::FoundryDocument,
        index_policy::{IndexingCancellation, WorkspaceIndexMetrics, WorkspaceIndexPolicy},
    },
};
use normalize_path::NormalizePath;
use solar_config::CompileOpts;
use solar_interface::{
    data_structures::{map::FxHashMap, smallvec::SmallVec},
    source_map::SourceMap,
};
use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

mod edit_scope;
mod foundry;
pub(crate) mod index_policy;
pub(crate) mod manifest;

pub(crate) use edit_scope::{WorkspaceEditError, WorkspaceEditScope};

#[derive(Debug)]
pub(crate) struct FoundryConfigContext<'a> {
    selected_profile: Option<&'a str>,
    source: FoundryConfigSourceRef<'a>,
    loaded: FxHashMap<PathBuf, Result<FoundryWorkspaceConfig, String>>,
}

#[derive(Clone, Copy, Debug)]
enum FoundryConfigSourceRef<'a> {
    Static(&'a [FoundryWorkspaceConfig]),
    Loader(&'a FoundryWorkspaceConfigLoader),
}

impl<'a> FoundryConfigContext<'a> {
    pub(crate) fn new(
        selected_profile: Option<&'a str>,
        workspace_configs: &'a [FoundryWorkspaceConfig],
    ) -> Self {
        Self {
            selected_profile,
            source: FoundryConfigSourceRef::Static(workspace_configs),
            loaded: FxHashMap::default(),
        }
    }

    pub(crate) fn from_source(
        selected_profile: Option<&'a str>,
        source: &'a FoundryWorkspaceConfigSource,
    ) -> Self {
        let source = match source {
            FoundryWorkspaceConfigSource::Static(configs) => {
                FoundryConfigSourceRef::Static(configs)
            }
            FoundryWorkspaceConfigSource::Loader(loader) => FoundryConfigSourceRef::Loader(loader),
        };
        Self { selected_profile, source, loaded: FxHashMap::default() }
    }

    pub(crate) fn selected_profile(&self) -> Option<&'a str> {
        self.selected_profile
    }

    pub(crate) fn workspace_config(
        &mut self,
        root: &Path,
    ) -> Result<Option<&FoundryWorkspaceConfig>, String> {
        let root = root.normalize();
        match self.source {
            FoundryConfigSourceRef::Static(configs) => {
                Ok(configs.iter().find(|config| config.workspace_root() == root))
            }
            FoundryConfigSourceRef::Loader(loader) => {
                let loaded = self.loaded.entry(root).or_insert_with_key(|root| {
                    loader.load(root).and_then(|config| {
                        let config = config.try_into_normalized()?;
                        if config.workspace_root() == root {
                            Ok(config)
                        } else {
                            Err(format!(
                                "host returned Foundry configuration for `{}` while loading `{}`",
                                config.workspace_root().display(),
                                root.display()
                            ))
                        }
                    })
                });
                match loaded {
                    Ok(config) => Ok(Some(config)),
                    Err(error) => Err(error.clone()),
                }
            }
        }
    }
}

impl Default for FoundryConfigContext<'_> {
    fn default() -> Self {
        Self::new(None, &[])
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Workspace {
    kind: WorkspaceKind,
    compile_opts: CompileOpts,
    /// Include roots approved for eager indexing and topology watching.
    ///
    /// `CompileOpts::include_paths` intentionally keeps every configured Foundry library root so
    /// imports and remappings can resolve external dependencies even when their files are outside
    /// the indexing boundary.
    index_import_only_roots: Vec<PathBuf>,
    source_roots: Vec<PathBuf>,
    /// Whether the project root supplements explicitly configured source roots.
    implicit_project_root: bool,
    source_watch_roots: Vec<SourceWatchRoot>,
    flycheck_watch_roots: Vec<SourceWatchRoot>,
    git_marker_watch_roots: Vec<PathBuf>,
    source_files: Vec<PathBuf>,
    /// Whether the latest source traversal saw every source path under its indexing boundary.
    source_files_complete: bool,
    flycheck_source_roots: Vec<PathBuf>,
    flycheck_source_files: Vec<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct SourceWatchRoot {
    pub(crate) path: PathBuf,
    pub(crate) recursive: bool,
    pub(crate) watch_contents: bool,
}

type CollectedSourceFiles = (Vec<PathBuf>, Vec<SourceWatchRoot>, Vec<PathBuf>, bool);

struct CollectedWorkspaceFiles {
    source_files: Vec<PathBuf>,
    source_watch_roots: Vec<SourceWatchRoot>,
    source_files_complete: bool,
    flycheck_source_files: Vec<PathBuf>,
    flycheck_watch_roots: Vec<SourceWatchRoot>,
    git_marker_watch_roots: Vec<PathBuf>,
}

impl SourceWatchRoot {
    fn shallow(path: &Path) -> Self {
        Self { path: path.to_path_buf(), recursive: false, watch_contents: true }
    }

    fn recursive(path: &Path) -> Self {
        Self { path: path.to_path_buf(), recursive: true, watch_contents: true }
    }

    fn missing_ancestor(path: &Path) -> Self {
        Self { path: path.to_path_buf(), recursive: false, watch_contents: false }
    }
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
            compile_opts: CompileOpts { base_path: Some(root), ..Default::default() },
            flycheck_source_roots: source_roots.clone(),
            source_roots,
            ..Self::unconfigured()
        }
    }

    pub(crate) fn unconfigured() -> Self {
        Self {
            kind: WorkspaceKind::Naked,
            implicit_project_root: false,
            compile_opts: CompileOpts::default(),
            index_import_only_roots: Vec::new(),
            source_roots: Vec::new(),
            source_watch_roots: Vec::new(),
            flycheck_watch_roots: Vec::new(),
            git_marker_watch_roots: Vec::new(),
            source_files: Vec::new(),
            source_files_complete: true,
            flycheck_source_roots: Vec::new(),
            flycheck_source_files: Vec::new(),
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

    pub(crate) fn source_watch_roots(&self) -> &[SourceWatchRoot] {
        &self.source_watch_roots
    }

    pub(crate) fn flycheck_watch_roots(&self) -> &[SourceWatchRoot] {
        &self.flycheck_watch_roots
    }

    pub(crate) fn git_marker_watch_roots(&self) -> &[PathBuf] {
        &self.git_marker_watch_roots
    }

    pub(crate) fn import_source_roots(&self) -> &[PathBuf] {
        &self.flycheck_source_roots
    }

    pub(crate) fn import_only_roots(&self) -> &[PathBuf] {
        &self.compile_opts.include_paths
    }

    pub(crate) fn import_remapping_paths(&self) -> impl Iterator<Item = PathBuf> + '_ {
        let remappings = self.compile_opts.import_remappings.iter();
        remappings.filter_map(|remapping| self.resolve_base_path(Path::new(&remapping.path)))
    }

    /// Normalizes `path`, resolving a relative path against the base path.
    pub(crate) fn resolve_base_path(&self, path: &Path) -> Option<PathBuf> {
        if path.is_absolute() {
            Some(path.normalize())
        } else {
            self.compile_opts.base_path.as_ref().map(|base| base.join(path).normalize())
        }
    }

    /// Returns include roots admitted to eager indexing and topology watching.
    pub(crate) fn index_import_only_roots(&self) -> &[PathBuf] {
        &self.index_import_only_roots
    }

    pub(crate) fn source_files(&self) -> &[PathBuf] {
        &self.source_files
    }

    pub(crate) fn source_files_complete(&self) -> bool {
        self.source_files_complete
    }

    pub(crate) fn flycheck_source_files(&self) -> &[PathBuf] {
        &self.flycheck_source_files
    }

    pub(crate) fn has_unindexed_flycheck_source_files(&self) -> bool {
        self.flycheck_source_files.iter().any(|path| self.source_files.binary_search(path).is_err())
    }

    pub(crate) fn is_import_only_path(&self, path: &Path) -> bool {
        is_import_only_path(&self.source_roots, self.import_only_roots(), path)
    }

    pub(crate) fn refresh_source_files(
        &mut self,
        policy: &WorkspaceIndexPolicy,
        cancellation: &IndexingCancellation,
        metrics: &mut WorkspaceIndexMetrics,
    ) -> bool {
        let Some(files) = self.collect_workspace_files(policy, cancellation, metrics, None) else {
            return false;
        };
        self.apply_collected_files(files);
        true
    }

    pub(crate) fn refresh_all_source_files(
        workspaces: &mut [Self],
        policy: &WorkspaceIndexPolicy,
        cancellation: &IndexingCancellation,
        metrics: &mut WorkspaceIndexMetrics,
    ) -> bool {
        if let [workspace] = workspaces {
            return workspace.refresh_source_files(policy, cancellation, metrics);
        }

        let mut collected = Vec::with_capacity(workspaces.len());
        {
            let index = WorkspacePathIndex::new(&*workspaces);
            for (idx, workspace) in workspaces.iter().enumerate() {
                let Some(files) = workspace.collect_workspace_files(
                    policy,
                    cancellation,
                    metrics,
                    Some((&index, idx)),
                ) else {
                    return false;
                };
                collected.push(files);
            }
        }
        for (workspace, files) in workspaces.iter_mut().zip(collected) {
            workspace.apply_collected_files(files);
        }
        true
    }

    fn collect_workspace_files(
        &self,
        policy: &WorkspaceIndexPolicy,
        cancellation: &IndexingCancellation,
        metrics: &mut WorkspaceIndexMetrics,
        ownership: Option<(&WorkspacePathIndex<'_>, usize)>,
    ) -> Option<CollectedWorkspaceFiles> {
        let (source_files, source_watch_roots, mut git_marker_watch_roots, source_files_complete) =
            self.collect_source_files(false, Vec::new(), policy, cancellation, metrics, ownership)?;
        let flycheck_files = source_files
            .iter()
            .filter(|path| {
                ownership.map_or_else(
                    || self.tracks_flycheck_file(policy, path),
                    |(index, workspace_idx)| {
                        index.workspace_idx_for_flycheck_path(policy, path) == Some(workspace_idx)
                    },
                )
            })
            .cloned()
            .collect();
        let (
            flycheck_source_files,
            flycheck_watch_roots,
            flycheck_marker_watch_roots,
            flycheck_source_files_complete,
        ) = self.collect_source_files(
            true,
            flycheck_files,
            policy,
            cancellation,
            &mut WorkspaceIndexMetrics::default(),
            ownership,
        )?;
        git_marker_watch_roots.extend(flycheck_marker_watch_roots);
        git_marker_watch_roots.sort_unstable();
        git_marker_watch_roots.dedup();
        Some(CollectedWorkspaceFiles {
            source_files,
            source_watch_roots,
            source_files_complete: source_files_complete && flycheck_source_files_complete,
            flycheck_source_files,
            flycheck_watch_roots,
            git_marker_watch_roots,
        })
    }

    fn apply_collected_files(&mut self, files: CollectedWorkspaceFiles) {
        self.source_files = files.source_files;
        self.source_watch_roots = files.source_watch_roots;
        self.source_files_complete = files.source_files_complete;
        self.flycheck_source_files = files.flycheck_source_files;
        self.flycheck_watch_roots = files.flycheck_watch_roots;
        self.git_marker_watch_roots = files.git_marker_watch_roots;
    }

    /// Collects the indexed source roots, or the flycheck roots not already indexed on top of
    /// `files`.
    fn collect_source_files(
        &self,
        flycheck: bool,
        mut files: Vec<PathBuf>,
        policy: &WorkspaceIndexPolicy,
        cancellation: &IndexingCancellation,
        metrics: &mut WorkspaceIndexMetrics,
        ownership: Option<(&WorkspacePathIndex<'_>, usize)>,
    ) -> Option<CollectedSourceFiles> {
        let source_roots = if flycheck { &self.flycheck_source_roots } else { &self.source_roots };
        let mut watch_roots = Vec::new();
        let mut marker_watch_roots = Vec::new();
        let mut source_files_complete = true;
        for root in source_roots {
            if flycheck && self.source_roots.contains(root) {
                continue;
            }
            let workspace_root = self.compile_opts.base_path.as_deref().unwrap_or(root);
            let partition_root = !flycheck && root == workspace_root;
            let watch_root_start = watch_roots.len();
            let mut collector = SourceFileCollector {
                workspace_root,
                source_root: root,
                implicit_project_root: self.implicit_project_root && partition_root,
                source_roots,
                import_only_roots: self.index_import_only_roots(),
                policy,
                cancellation,
                metrics,
                files: &mut files,
                watch_roots: &mut watch_roots,
                marker_watch_roots: &mut marker_watch_roots,
                ownership,
                flycheck,
                source_files_complete: true,
            };
            let state = collector.collect(root, partition_root);
            source_files_complete &= collector.source_files_complete;
            match state {
                SourceTreeState::Cancelled => return None,
                SourceTreeState::Pruned => continue,
                SourceTreeState::Clean | SourceTreeState::Partitioned => {}
            }
            if watch_roots.len() == watch_root_start {
                watch_roots.push(if root == workspace_root {
                    SourceWatchRoot::shallow(root)
                } else {
                    SourceWatchRoot::recursive(root)
                });
            }
        }
        files.sort_unstable();
        files.dedup();
        watch_roots.sort_unstable();
        watch_roots.dedup();
        marker_watch_roots.sort_unstable();
        marker_watch_roots.dedup();
        Some((files, watch_roots, marker_watch_roots, source_files_complete))
    }

    pub(crate) fn add_source_file(&mut self, policy: &WorkspaceIndexPolicy, path: PathBuf) {
        if std::fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.is_file())
            && self.tracks_disk_file(policy, &path)
        {
            insert_sorted(&mut self.source_files, path);
        }
    }

    pub(crate) fn remove_source_file(&mut self, path: &Path) {
        remove_sorted(&mut self.source_files, path);
    }

    pub(crate) fn add_flycheck_source_file(&mut self, policy: &WorkspaceIndexPolicy, path: &Path) {
        if std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_file())
            && self.tracks_flycheck_file(policy, path)
        {
            insert_sorted(&mut self.flycheck_source_files, path.to_path_buf());
        }
    }

    pub(crate) fn remove_flycheck_source_file(&mut self, path: &Path) {
        remove_sorted(&mut self.flycheck_source_files, path);
    }

    pub(crate) fn tracks_disk_file(&self, policy: &WorkspaceIndexPolicy, path: &Path) -> bool {
        self.tracks_file(&self.source_roots, policy, path)
    }

    pub(crate) fn tracks_flycheck_file(&self, policy: &WorkspaceIndexPolicy, path: &Path) -> bool {
        self.tracks_file(&self.flycheck_source_roots, policy, path)
    }

    fn tracks_file(&self, roots: &[PathBuf], policy: &WorkspaceIndexPolicy, path: &Path) -> bool {
        is_solidity_file(path)
            && !is_import_only_path(roots, self.index_import_only_roots(), path)
            && roots.iter().any(|root| {
                let workspace_root = self.compile_opts.base_path.as_deref().unwrap_or(root);
                path.starts_with(root) && !policy.excludes_source_file(workspace_root, root, path)
            })
    }

    pub(crate) fn excludes_source_directory(
        &self,
        policy: &WorkspaceIndexPolicy,
        source_root: &Path,
        path: &Path,
    ) -> bool {
        let workspace_root = self.compile_opts.base_path.as_deref().unwrap_or(source_root);
        policy.excludes_source_directory(workspace_root, source_root, path)
    }

    #[cfg(any(test, feature = "bench"))]
    pub(crate) fn load_foundry(path: PathBuf) -> Result<Self, WorkspaceError> {
        Self::load_foundry_inner(path, None, &mut FoundryConfigContext::default())
    }

    pub(crate) fn load_foundry_bounded(
        path: PathBuf,
        workspace_roots: &[PathBuf],
        foundry_config: &mut FoundryConfigContext<'_>,
    ) -> Result<Self, WorkspaceError> {
        Self::load_foundry_inner(path, Some(workspace_roots), foundry_config)
    }

    fn load_foundry_inner(
        path: PathBuf,
        workspace_roots: Option<&[PathBuf]>,
        foundry_config: &mut FoundryConfigContext<'_>,
    ) -> Result<Self, WorkspaceError> {
        let root = path
            .parent()
            .ok_or_else(|| WorkspaceError::MissingManifestParent(path.clone()))?
            .normalize();
        let approved = |path: &Path| {
            workspace_roots
                .is_none_or(|workspace_roots| is_approved_index_root(path, &root, workspace_roots))
        };
        let host_config = foundry_config
            .workspace_config(&root)
            .map_err(|error| WorkspaceError::HostConfig { root: root.clone(), error })?;
        let implicit_project_root = host_config.is_none();
        let (source_roots, flycheck_source_roots, include_paths, import_remappings, evm_version) =
            if let Some(config) = host_config {
                (
                    config.source_roots().to_vec(),
                    config.flycheck_source_roots().to_vec(),
                    config.include_paths().to_vec(),
                    config.import_remappings().to_vec(),
                    config.evm_version(),
                )
            } else {
                let profile =
                    load_foundry_document(&path)?.profile_for(foundry_config.selected_profile());
                let include_paths = profile
                    .include_paths(&root)
                    .into_iter()
                    .map(|path| path.normalize())
                    .collect::<Vec<_>>();
                let import_remappings =
                    profile.remappings_with_include_paths(&root, &include_paths);
                let flycheck_source_roots = profile.build_source_roots(&root);
                // Index the project independently of the build's entry-point directories.
                // Keep explicit roots for external sources and exclusion overrides.
                let mut source_roots = vec![root.clone()];
                source_roots
                    .extend(flycheck_source_roots.iter().filter(|path| **path != root).cloned());
                (
                    source_roots,
                    flycheck_source_roots,
                    include_paths,
                    import_remappings,
                    profile.evm_version(),
                )
            };
        let implicit_project_root = implicit_project_root && !flycheck_source_roots.contains(&root);
        let source_roots = source_roots.into_iter().filter(|path| approved(path)).collect();
        let flycheck_source_roots =
            flycheck_source_roots.into_iter().filter(|path| approved(path)).collect();
        let index_import_only_roots =
            include_paths.iter().filter(|path| approved(path)).cloned().collect::<Vec<_>>();
        let mut compile_opts = CompileOpts {
            base_path: Some(root),
            include_paths,
            import_remappings,
            ..Default::default()
        };
        if let Some(evm_version) = evm_version {
            compile_opts.evm_version = evm_version;
        }

        Ok(Self {
            kind: WorkspaceKind::Foundry,
            implicit_project_root,
            index_import_only_roots,
            source_roots,
            flycheck_source_roots,
            compile_opts,
            ..Self::unconfigured()
        })
    }
}

pub(crate) fn is_approved_index_root(
    path: &Path,
    manifest_root: &Path,
    workspace_roots: &[PathBuf],
) -> bool {
    let path = path.normalize();
    path.starts_with(manifest_root.normalize())
        || workspace_roots.iter().any(|root| path.starts_with(root.normalize()))
}

fn insert_sorted(files: &mut Vec<PathBuf>, path: PathBuf) {
    if let Err(pos) = files.binary_search(&path) {
        files.insert(pos, path);
    }
}

fn remove_sorted(files: &mut Vec<PathBuf>, path: &Path) {
    if let Ok(pos) = files.binary_search_by(|candidate| candidate.as_path().cmp(path)) {
        files.remove(pos);
    }
}

pub(crate) struct WorkspacePathIndex<'a> {
    workspaces: &'a [Workspace],
    import_entries: Arc<Vec<WorkspaceImportPathIndexEntry>>,
    root_index: Arc<FxHashMap<PathBuf, SmallVec<[WorkspacePathMatch; 4]>>>,
}

type WorkspacePathMatch = (usize, usize, u8, usize);

/// Immutable workspace path data shared by short-lived query indexes.
///
/// Workspace configuration recreates a borrowed index for each request. Keep the expensive root
/// map in an owned cache so those indexes only clone two `Arc`s while retaining the workspace
/// borrow needed for policy checks.
#[derive(Debug)]
pub(crate) struct WorkspacePathIndexCache {
    import_entries: Arc<Vec<WorkspaceImportPathIndexEntry>>,
    root_index: Arc<FxHashMap<PathBuf, SmallVec<[WorkspacePathMatch; 4]>>>,
}

pub(crate) struct WorkspacePathQuery {
    matches: SmallVec<[WorkspacePathMatch; 16]>,
}

#[derive(Clone, Debug)]
pub(crate) struct WorkspaceImportPathIndexEntry {
    idx: usize,
    base_depth: usize,
    roots: Vec<WorkspaceImportRoot>,
}

#[derive(Clone, Debug)]
struct WorkspaceImportRoot {
    path: PathBuf,
    depth: usize,
    kind: u8,
}

impl<'a> WorkspacePathIndex<'a> {
    pub(crate) fn new(workspaces: &'a [Workspace]) -> Self {
        let WorkspacePathIndexCache { import_entries, root_index } = Self::cache(workspaces);
        Self { workspaces, import_entries, root_index }
    }

    pub(crate) fn cache(workspaces: &[Workspace]) -> WorkspacePathIndexCache {
        let import_entries = Arc::new(
            workspaces
                .iter()
                .enumerate()
                .map(|(idx, workspace)| WorkspaceImportPathIndexEntry::new(idx, workspace))
                .collect::<Vec<_>>(),
        );
        let root_index = Arc::new(Self::build_root_index(&import_entries));
        WorkspacePathIndexCache { import_entries, root_index }
    }

    pub(crate) fn with_cache(
        workspaces: &'a [Workspace],
        cache: Arc<WorkspacePathIndexCache>,
    ) -> Self {
        Self {
            workspaces,
            import_entries: Arc::clone(&cache.import_entries),
            root_index: Arc::clone(&cache.root_index),
        }
    }

    pub(crate) fn with_import_entries(
        workspaces: &'a [Workspace],
        import_entries: Arc<Vec<WorkspaceImportPathIndexEntry>>,
    ) -> Self {
        let root_index = Arc::new(Self::build_root_index(&import_entries));
        Self { workspaces, import_entries, root_index }
    }

    fn build_root_index(
        import_entries: &[WorkspaceImportPathIndexEntry],
    ) -> FxHashMap<PathBuf, SmallVec<[WorkspacePathMatch; 4]>> {
        let mut root_index = FxHashMap::default();
        for entry in import_entries {
            for root in &entry.roots {
                root_index.entry(root.path.clone()).or_insert_with(SmallVec::new).push((
                    entry.idx,
                    root.depth,
                    root.kind,
                    entry.base_depth,
                ));
            }
        }
        root_index
    }

    pub(crate) fn clone_import_entries(&self) -> Arc<Vec<WorkspaceImportPathIndexEntry>> {
        Arc::clone(&self.import_entries)
    }

    pub(crate) fn query(&self, path: &Path) -> WorkspacePathQuery {
        // Most callers need several ownership projections for the same path, so resolve roots
        // once and keep the matching workspace metadata in the query object.
        let matches = if path.is_normalized() {
            self.matching_entries(path)
        } else {
            let normalized = path.normalize();
            self.matching_entries(&normalized)
        };
        WorkspacePathQuery { matches }
    }

    fn matching_entries(&self, path: &Path) -> SmallVec<[WorkspacePathMatch; 16]> {
        if self.root_index.is_empty() {
            return SmallVec::new();
        }
        let mut matches = SmallVec::<[WorkspacePathMatch; 16]>::new();
        for ancestor in path.ancestors() {
            let Some(roots) = self.root_index.get(ancestor) else { continue };
            for &candidate @ (idx, root_depth, root_kind, _) in roots {
                if let Some(best) = matches.iter_mut().find(|best| best.0 == idx) {
                    if (root_depth, root_kind) > (best.1, best.2) {
                        *best = candidate;
                    }
                } else {
                    matches.push(candidate);
                }
            }
        }
        matches.sort_unstable_by_key(|&(idx, _, _, _)| idx);
        matches
    }

    pub(crate) fn workspace_idx_for_import_path(&self, path: &Path) -> Option<usize> {
        self.query(path).workspace_idx_for_import_path()
    }

    /// Returns the owning workspace when `path` is an active disk source under its policy.
    ///
    /// The most specific matching base path or explicit source root owns the path. At the same
    /// depth, base paths take precedence over source roots.
    pub(crate) fn workspace_idx_for_source_path(
        &self,
        policy: &WorkspaceIndexPolicy,
        path: &Path,
    ) -> Option<usize> {
        let idx = self.workspace_idx_for_region(path, false)?;
        self.workspaces[idx].tracks_disk_file(policy, path).then_some(idx)
    }

    pub(crate) fn workspace_idx_for_flycheck_path(
        &self,
        policy: &WorkspaceIndexPolicy,
        path: &Path,
    ) -> Option<usize> {
        let idx = self.workspace_idx_for_region(path, true)?;
        self.workspaces[idx].tracks_flycheck_file(policy, path).then_some(idx)
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

    fn workspace_idx_for_region(&self, path: &Path, flycheck: bool) -> Option<usize> {
        const SOURCE: u8 = 0;
        const BASE: u8 = 1;

        self.workspaces
            .iter()
            .enumerate()
            .filter_map(|(idx, workspace)| {
                let base_path = workspace.compile_opts().base_path.as_deref();
                let base_depth = base_path.map_or(0, |base_path| base_path.components().count());
                let base_match = base_path
                    .filter(|base_path| path.starts_with(base_path))
                    .map(|_| (base_depth, BASE));
                let roots = if flycheck {
                    workspace.import_source_roots()
                } else {
                    workspace.source_roots()
                };
                let source_match = roots
                    .iter()
                    .filter(|root| path.starts_with(root))
                    .map(|root| (root.components().count(), SOURCE))
                    .max();
                let (root_depth, root_kind) = base_match.into_iter().chain(source_match).max()?;
                Some((idx, root_depth, root_kind, base_depth))
            })
            .max_by_key(|&(idx, root_depth, root_kind, base_depth)| {
                (root_depth, root_kind, base_depth, idx)
            })
            .map(|(idx, _, _, _)| idx)
    }
}

impl WorkspacePathQuery {
    pub(crate) fn workspace_idx_for_path(&self) -> usize {
        self.matches
            .iter()
            .copied()
            .max_by_key(|&(idx, root_depth, root_kind, base_depth)| {
                (root_depth, root_kind, base_depth, idx)
            })
            .map_or(0, |(idx, _, _, _)| idx)
    }

    /// Returns the workspace whose import configuration owns `path`.
    ///
    /// The deepest matching root wins. At the same depth, base paths take precedence over
    /// external source roots, which take precedence over import-only roots. A tie across
    /// workspaces at both levels has no unique owner.
    pub(crate) fn workspace_idx_for_import_path(&self) -> Option<usize> {
        let mut best = None;
        for (idx, root_depth, root_kind, _) in self.matches.iter().copied() {
            let score = (root_depth, root_kind);
            match best.as_mut() {
                Some((best_score, _, _)) if score < *best_score => {}
                Some((best_score, _, ambiguous)) if score == *best_score => *ambiguous = true,
                _ => best = Some((score, idx, false)),
            }
        }
        best.and_then(|(_, owner, ambiguous)| (!ambiguous).then_some(owner))
    }

    /// Returns every workspace whose import configuration can resolve `path`.
    pub(crate) fn workspace_idxs_for_import_path(
        &self,
    ) -> impl DoubleEndedIterator<Item = usize> + '_ {
        self.matches.iter().map(|(idx, _, _, _)| *idx)
    }
}

pub(crate) fn workspace_idx_containing_path(
    workspaces: &[Workspace],
    path: &Path,
) -> Option<usize> {
    workspaces
        .iter()
        .enumerate()
        .filter_map(|(idx, workspace)| {
            let base_path = workspace.compile_opts().base_path.as_deref()?;
            path.starts_with(base_path).then(|| (idx, base_path.components().count()))
        })
        .max_by_key(|&(idx, depth)| (depth, idx))
        .map(|(idx, _)| idx)
}

impl WorkspaceImportPathIndexEntry {
    fn new(idx: usize, workspace: &Workspace) -> Self {
        const IMPORT_ONLY: u8 = 0;
        const SOURCE: u8 = 1;
        const BASE: u8 = 2;

        let base_path = workspace.compile_opts().base_path.as_deref().map(Path::normalize);
        let base_depth = base_path.as_ref().map_or(0, |path| path.components().count());
        let mut roots = Vec::new();
        if let Some(path) = &base_path {
            roots.push(WorkspaceImportRoot::new(path.clone(), BASE));
        }
        roots.extend(
            workspace
                .import_source_roots()
                .iter()
                .map(|path| WorkspaceImportRoot::new(path.normalize(), SOURCE)),
        );
        roots.extend(
            workspace
                .import_only_roots()
                .iter()
                .map(|path| WorkspaceImportRoot::new(path.normalize(), IMPORT_ONLY)),
        );
        for path in workspace.import_remapping_paths() {
            roots.push(WorkspaceImportRoot::new(path, IMPORT_ONLY));
        }
        Self { idx, base_depth, roots }
    }
}

impl WorkspaceImportRoot {
    fn new(path: PathBuf, kind: u8) -> Self {
        let depth = path.components().count();
        Self { path, depth, kind }
    }
}

struct SourceFileCollector<'a, 'index, 'workspaces> {
    workspace_root: &'a Path,
    source_root: &'a Path,
    implicit_project_root: bool,
    source_roots: &'a [PathBuf],
    import_only_roots: &'a [PathBuf],
    policy: &'a WorkspaceIndexPolicy,
    cancellation: &'a IndexingCancellation,
    metrics: &'a mut WorkspaceIndexMetrics,
    files: &'a mut Vec<PathBuf>,
    watch_roots: &'a mut Vec<SourceWatchRoot>,
    marker_watch_roots: &'a mut Vec<PathBuf>,
    ownership: Option<(&'index WorkspacePathIndex<'workspaces>, usize)>,
    flycheck: bool,
    source_files_complete: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceTreeState {
    Clean,
    Partitioned,
    Pruned,
    Cancelled,
}

impl SourceFileCollector<'_, '_, '_> {
    fn collect(&mut self, path: &Path, partition_root: bool) -> SourceTreeState {
        if self.cancellation.is_cancelled() {
            return SourceTreeState::Cancelled;
        }
        // A nested source root is collected separately with its own exclusion boundary.
        if path != self.source_root && self.source_roots.iter().any(|root| root == path) {
            return SourceTreeState::Pruned;
        }
        self.metrics.visited += 1;
        if let Some((index, workspace_idx)) = self.ownership
            && index
                .workspace_idx_for_region(path, self.flycheck)
                .is_some_and(|idx| idx != workspace_idx)
        {
            self.metrics.pruned += 1;
            return SourceTreeState::Pruned;
        }
        if is_import_only_path(self.source_roots, self.import_only_roots, path) {
            self.metrics.pruned += 1;
            // Dependency trees are outside whole-project discovery. Pruning inside an
            // explicit source root can still omit project importers.
            self.source_files_complete &= self.implicit_project_root;
            return SourceTreeState::Pruned;
        }
        let metadata = match std::fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound && path == self.source_root => {
                // Keep recursive watches for files created with the root, and watch its parent
                // for creation of the root itself. Missing roots are known to contain no sources.
                self.watch_roots.push(SourceWatchRoot::recursive(path));
                if let Some(ancestor) = path.ancestors().skip(1).find(|ancestor| {
                    std::fs::symlink_metadata(ancestor).is_ok_and(|metadata| metadata.is_dir())
                }) {
                    self.watch_roots.push(SourceWatchRoot::missing_ancestor(ancestor));
                }
                return SourceTreeState::Clean;
            }
            Err(_) => {
                self.source_files_complete = false;
                return SourceTreeState::Partitioned;
            }
        };
        if metadata.is_file() {
            if !is_solidity_file(path) {
                return SourceTreeState::Clean;
            }
            if self.policy.excludes_source_file(self.workspace_root, self.source_root, path) {
                self.metrics.pruned += 1;
                self.source_files_complete = false;
            } else {
                self.files.push(path.to_path_buf());
                self.metrics.eager += 1;
            }
            return SourceTreeState::Clean;
        }
        if !metadata.is_dir() {
            self.source_files_complete = false;
            return SourceTreeState::Partitioned;
        }
        if self.policy.should_prune_source_directory(self.workspace_root, self.source_root, path) {
            if let Some(root) = self.policy.nested_repository_marker_root(path) {
                self.marker_watch_roots.push(root);
            }
            self.metrics.pruned += 1;
            // Built-in exclusions define whole-project indexing boundaries. Custom
            // exclusions and pruning inside explicit source roots may hide importers.
            self.source_files_complete &= self.implicit_project_root
                && !self.policy.excludes_relative_path(self.workspace_root, path, true);
            return SourceTreeState::Pruned;
        }

        let watch_root_start = self.watch_roots.len();
        let mut partitioned = partition_root;
        let Ok(entries) = std::fs::read_dir(path) else {
            self.source_files_complete = false;
            self.watch_roots.push(SourceWatchRoot::shallow(path));
            return SourceTreeState::Partitioned;
        };
        for entry in entries {
            let Ok(entry) = entry else {
                self.source_files_complete = false;
                partitioned = true;
                continue;
            };
            match self.collect(&entry.path(), false) {
                SourceTreeState::Clean => {}
                SourceTreeState::Partitioned | SourceTreeState::Pruned => partitioned = true,
                SourceTreeState::Cancelled => return SourceTreeState::Cancelled,
            }
        }

        // Recursive watchers are safe only for subtrees with no pruned descendants.
        if partitioned {
            self.watch_roots.push(SourceWatchRoot::shallow(path));
            SourceTreeState::Partitioned
        } else {
            self.watch_roots.truncate(watch_root_start);
            self.watch_roots.push(SourceWatchRoot::recursive(path));
            SourceTreeState::Clean
        }
    }
}

pub(crate) fn is_import_only_path(
    source_roots: &[PathBuf],
    import_only_roots: &[PathBuf],
    path: &Path,
) -> bool {
    import_only_roots.iter().any(|import_root| {
        is_import_only_path_in_root(
            path,
            import_root,
            source_roots.iter().filter(|source_root| source_root.starts_with(import_root)),
        )
    })
}

/// Checks an import root against source exceptions selected before any filesystem resolution.
fn is_import_only_path_in_root<'a>(
    path: &Path,
    import_root: &Path,
    mut source_roots: impl Iterator<Item = &'a PathBuf>,
) -> bool {
    path.starts_with(import_root) && !source_roots.any(|source_root| path.starts_with(source_root))
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
    #[error("failed to load host configuration for workspace `{}`: {error}", root.display())]
    HostConfig { root: PathBuf, error: String },
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

    fn foundry(project: &TestProject, manifest: &str) -> Workspace {
        Workspace::load_foundry(project.path(manifest)).unwrap()
    }

    fn refresh(workspace: &mut Workspace, policy: &WorkspaceIndexPolicy) -> WorkspaceIndexMetrics {
        let mut metrics = WorkspaceIndexMetrics::default();
        assert!(workspace.refresh_source_files(
            policy,
            &IndexingCancellation::default(),
            &mut metrics
        ));
        metrics
    }

    fn exclude(globs: &[&str]) -> WorkspaceIndexPolicy {
        WorkspaceIndexPolicy::new(IndexingOptions {
            exclude: globs.iter().map(|glob| glob.to_string()).collect(),
            ..Default::default()
        })
    }

    fn remappings(workspace: &Workspace) -> Vec<String> {
        workspace.compile_opts().import_remappings.iter().map(ToString::to_string).collect()
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

        let mut workspace = foundry(&project, "/foundry.toml");
        let opts = workspace.compile_opts();

        assert_eq!(opts.base_path.as_deref(), Some(project.root()));
        assert_eq!(opts.include_paths, vec![project.path("/lib"), project.path("/vendor")]);
        assert_eq!(opts.evm_version, EvmVersion::Cancun);
        assert_eq!(
            remappings(&workspace),
            [
                "ds-test/=vendor/ds-test/src/",
                "forge-std/=lib/forge-std/src/",
                "solmate/=lib/solmate/src/",
                "@oz=lib/openzeppelin-contracts/contracts/",
                "ds-test=lib/ds-test/src/",
            ]
        );
        assert_eq!(
            workspace.source_roots(),
            &[
                project.path("/"),
                project.path("/contracts"),
                project.path("/test"),
                project.path("/script")
            ]
        );

        // Missing source roots keep the traversal complete and watch their existing parent.
        refresh(&mut workspace, &WorkspaceIndexPolicy::default());
        assert!(workspace.source_files_complete());
        assert_eq!(workspace.source_files(), workspace.flycheck_source_files());
        assert!(
            workspace
                .source_watch_roots()
                .contains(&SourceWatchRoot::missing_ancestor(&project.path("/")))
        );
    }

    #[test]
    fn foundry_workspace_loads_selected_profile_compile_config() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [profile.default]
            src = "default-src"
            test = "default-test"
            script = "default-script"
            libs = ["default-libs"]
            evm_version = "paris"

            [profile.custom]
            src = "custom-src"
            libs = ["custom-libs"]
            evm_version = "cancun"
            "#,
        );

        let workspace = Workspace::load_foundry_bounded(
            project.path("/foundry.toml"),
            &[project.root().to_path_buf()],
            &mut FoundryConfigContext::new(Some("custom"), &[]),
        )
        .unwrap();

        assert_eq!(
            workspace.source_roots(),
            &[
                project.path("/"),
                project.path("/custom-src"),
                project.path("/default-test"),
                project.path("/default-script")
            ]
        );
        assert_eq!(
            workspace.import_source_roots(),
            &[
                project.path("/custom-src"),
                project.path("/default-test"),
                project.path("/default-script"),
            ]
        );
        assert_eq!(workspace.compile_opts().include_paths, [project.path("/custom-libs")]);
        assert_eq!(workspace.compile_opts().evm_version, EvmVersion::Cancun);
    }

    #[test]
    fn foundry_workspace_config_matches_only_the_exact_normalized_root() {
        let project = TestProject::new();
        let outer = FoundryWorkspaceConfig::new(project.path("/outer"));
        let nested = FoundryWorkspaceConfig::new(project.path("/outer/nested"));
        let configs = [outer, nested];
        let mut foundry_config = FoundryConfigContext::new(None, &configs);

        let mut root_matches = |root: &Path, expected: &Path| {
            foundry_config
                .workspace_config(root)
                .unwrap()
                .is_some_and(|config| config.workspace_root() == expected)
        };
        assert!(root_matches(&project.path("/outer"), &project.path("/outer")));
        assert!(root_matches(
            &project.path("/outer/./nested/../nested"),
            &project.path("/outer/nested")
        ));
        assert!(foundry_config.workspace_config(&project.path("/outer/other")).unwrap().is_none());
    }

    #[test]
    fn bounded_foundry_workspace_keeps_external_compile_config_out_of_the_index() {
        let project = TestProject::from_fixture(
            r#"
            //- /workspace/foundry.toml
            [profile.default]
            libs = ["../external/lib"]
            remappings = ["external/=../external/lib/pkg/src/"]

            //- /external/lib/pkg/src/Target.sol
            contract Target {}
            "#,
        );
        let load = |configs| {
            Workspace::load_foundry_bounded(
                project.path("/workspace/foundry.toml"),
                &[project.path("/workspace")],
                &mut FoundryConfigContext::new(None, configs),
            )
            .unwrap()
        };

        let workspace = load(&[]);
        let target = project.path("/external/lib/pkg/src").to_string_lossy().replace('\\', "/");
        assert_eq!(workspace.compile_opts().include_paths, [project.path("/external/lib")]);
        assert_eq!(workspace.import_only_roots(), [project.path("/external/lib")]);
        assert!(workspace.index_import_only_roots().is_empty());
        assert_eq!(
            remappings(&workspace),
            [format!("pkg/={target}/"), "external/=../external/lib/pkg/src/".into()]
        );
        let workspaces = [workspace];
        assert_eq!(
            WorkspacePathIndex::new(&workspaces)
                .workspace_idx_for_import_path(&project.path("/external/lib/pkg/src/Target.sol")),
            Some(0)
        );

        let external = project.path("/external/src");
        let host = FoundryWorkspaceConfig::new(project.path("/workspace"))
            .with_source_roots([external.clone()])
            .with_flycheck_source_roots([external.clone()])
            .with_include_paths([external.clone()]);
        let workspace = load(&[host]);
        assert!(workspace.source_roots().is_empty());
        assert!(workspace.import_source_roots().is_empty());
        assert_eq!(workspace.compile_opts().include_paths, [external]);
        assert!(workspace.index_import_only_roots().is_empty());
    }

    #[test]
    fn foundry_workspace_remapping_detection_honors_config_and_absolute_libraries() {
        let project = TestProject::from_fixture(
            r#"
            //- /disabled/lib/forge-std/src/Test.sol
            contract Test {}

            //- /disabled/remappings.txt
            solmate/=lib/solmate/src/

            //- /disabled/foundry.toml
            [profile.default]
            auto_detect_remappings = false
            remappings = ["@oz=lib/openzeppelin-contracts/contracts/"]

            //- /shared/lib/pkg/src/Target.sol
            contract Target {}
            "#,
        );
        let library = project.path("/shared/lib").to_string_lossy().replace('\\', "/");
        project.write_file(
            "/workspace/foundry.toml",
            &format!("[profile.default]\nlibs = [\"{library}\"]\n"),
        );

        assert_eq!(
            remappings(&foundry(&project, "/disabled/foundry.toml")),
            ["solmate/=lib/solmate/src/", "@oz=lib/openzeppelin-contracts/contracts/"]
        );
        assert_eq!(
            remappings(&foundry(&project, "/workspace/foundry.toml")),
            [format!("pkg/={library}/pkg/src/")]
        );
    }

    #[test]
    fn foundry_flycheck_sources_respect_index_boundaries() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [profile.default]
            src = "src"
            test = "."
            script = "script"
            libs = ["vendor"]

            //- /src/Main.sol
            contract Main {}

            //- /test/Tracked.t.sol
            contract TrackedTest {}

            //- /script/Deploy.s.sol
            contract Deploy {}

            //- /script/node_modules/Ignored.s.sol
            contract IgnoredScript {}

            //- /vendor/Dependency.sol
            contract Dependency {}

            //- /out/Generated.sol
            contract Generated {}

            //- /custom/Excluded.sol
            contract Excluded {}

            //- /.hidden/Hidden.sol
            contract Hidden {}

            //- /nested/.git
            gitdir: elsewhere

            //- /nested/Ignored.sol
            contract Ignored {}
            "#,
        );
        let policy = exclude(&["custom/**"]);
        let mut workspace = foundry(&project, "/foundry.toml");

        let metrics = refresh(&mut workspace, &policy);

        assert_eq!(workspace.source_files(), workspace.flycheck_source_files());
        assert_eq!(
            workspace.flycheck_source_files(),
            &[
                project.path("/script/Deploy.s.sol"),
                project.path("/src/Main.sol"),
                project.path("/test/Tracked.t.sol"),
            ]
        );
        assert!(workspace.tracks_flycheck_file(&policy, &project.path("/test/Tracked.t.sol")));
        assert!(!workspace.tracks_flycheck_file(&policy, &project.path("/vendor/Dependency.sol")));
        assert!(!workspace.tracks_flycheck_file(&policy, &project.path("/custom/Excluded.sol")));
        assert_eq!(metrics.eager, 3);
    }

    #[test]
    fn workspace_path_index_selects_import_owners_by_root_kind_and_specificity() {
        let project = TestProject::from_fixture(
            r#"
            //- /first/foundry.toml
            [profile.default]
            src = "../external/priority"
            libs = ["../external/include", "../external/tie"]
            auto_detect_remappings = false

            //- /nested/second/foundry.toml
            [profile.default]
            src = "../../external/source/nested"
            test = "../../external/tests"
            script = "../../external/scripts"
            libs = [
                "../../external/include/nested",
                "../../external/priority",
                "../../external/tie",
                "../../external/stable",
            ]
            auto_detect_remappings = false

            //- /nested/third/foundry.toml
            [profile.default]
            libs = ["../../external/stable"]
            auto_detect_remappings = false

            //- /source/foundry.toml
            [profile.default]
            src = "../external/source"
            auto_detect_remappings = false
            "#,
        );
        let workspaces = [
            foundry(&project, "/first/foundry.toml"),
            foundry(&project, "/nested/second/foundry.toml"),
            foundry(&project, "/nested/third/foundry.toml"),
            foundry(&project, "/first/../source/foundry.toml"),
        ];
        let index = WorkspacePathIndex::new(&workspaces);

        for (path, expected) in [
            ("/first/Owned.sol", Some(0)),
            ("/external/source/Owned.sol", Some(3)),
            // Manifest roots and query paths are normalized.
            ("/external/source/../source/Owned.sol", Some(3)),
            ("/external/source/nested/Owned.sol", Some(1)),
            ("/external/tests/Owned.t.sol", Some(1)),
            ("/external/scripts/Owned.s.sol", Some(1)),
            ("/external/include/nested/Owned.sol", Some(1)),
            ("/external/priority/Owned.sol", Some(0)),
            ("/external/tie/Owned.sol", None),
            ("/external/stable/Owned.sol", None),
            ("/unowned/Overlay.sol", None),
        ] {
            assert_eq!(
                index.workspace_idx_for_import_path(&project.path(path)),
                expected,
                "{path}"
            );
        }
    }

    #[test]
    fn workspace_path_index_selects_source_owners_under_their_policy() {
        let project = TestProject::from_fixture(
            r#"
            //- /nested/foundry.toml
            [profile.default]
            src = "."
            libs = ["vendor"]

            //- /project/foundry.toml
            [profile.default]
            src = "../shared"
            "#,
        );
        let workspaces = [
            Workspace::naked(project.root().to_path_buf()),
            foundry(&project, "/nested/foundry.toml"),
            foundry(&project, "/project/foundry.toml"),
        ];
        let index = WorkspacePathIndex::new(&workspaces);
        let policy = exclude(&["generated/**"]);

        // The most specific base path owns a path.
        let query = index.query(&project.path("/nested/A.sol"));
        assert_eq!(query.workspace_idx_for_path(), 1);
        assert_eq!(query.workspace_idxs_for_import_path().collect::<Vec<_>>(), [0, 1]);
        assert_eq!(index.query(&project.path("/B.sol")).workspace_idx_for_path(), 0);

        for (path, expected) in [
            ("/nested/Included.sol", Some(1)),
            ("/nested/generated/Excluded.sol", None),
            ("/nested/vendor/Dependency.sol", None),
            ("/shared/External.sol", Some(2)),
        ] {
            assert_eq!(
                index.workspace_idx_for_source_path(&policy, &project.path(path)),
                expected,
                "{path}"
            );
        }
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

            //- /nested/Outside.sol
            contract Outside {}
            "#,
        );
        let mut workspaces =
            [foundry(&project, "/foundry.toml"), foundry(&project, "/nested/foundry.toml")];
        let policy = WorkspaceIndexPolicy::default();
        let mut metrics = refresh(&mut workspaces[0], &policy);

        WorkspacePathIndex::reconcile_source_files(&mut workspaces, &policy, &mut metrics);

        assert_eq!(workspaces[0].source_files(), &[project.path("/Outer.sol")]);
        assert_eq!(
            workspaces[1].source_files(),
            &[project.path("/nested/Outside.sol"), project.path("/nested/src/Owned.sol")]
        );
        assert_eq!(metrics.eager, 3);
    }

    #[test]
    fn naked_workspace_source_files_skip_heavy_dirs_and_cancelled_refreshes() {
        let project = TestProject::new();
        project.write_file("/src/A.sol", "contract A {}");
        for dir in [".git", "cache", "lib", "node_modules", "out", "target"] {
            project.write_file(&format!("/{dir}/Ignored.sol"), "contract Ignored {}");
        }
        project.write_file("/nested/.git", "gitdir: elsewhere");
        project.write_file("/nested/Ignored.sol", "contract Ignored {}");
        let policy = WorkspaceIndexPolicy::default();
        let mut workspace = Workspace::naked(project.root().to_path_buf());

        refresh(&mut workspace, &policy);
        assert_eq!(workspace.source_files(), &[project.path("/src/A.sol")]);

        workspace.remove_source_file(&project.path("/src/A.sol"));
        assert!(workspace.source_files().is_empty());
        for path in ["/src/A.sol", "/node_modules/Ignored.sol", "/nested/Ignored.sol"] {
            workspace.add_source_file(&policy, project.path(path));
        }
        assert_eq!(workspace.source_files(), &[project.path("/src/A.sol")]);

        project.write_file("/src/After.sol", "contract After {}");
        let cancellation = IndexingCancellation::default();
        cancellation.cancel();
        assert!(!workspace.refresh_source_files(
            &policy,
            &cancellation,
            &mut WorkspaceIndexMetrics::default()
        ));
        assert_eq!(workspace.source_files(), &[project.path("/src/A.sol")]);
    }

    #[test]
    fn source_traversal_honors_switches_custom_globs_and_nested_repositories() {
        let project = TestProject::from_fixture(
            r#"
            //- /src/Included.sol
            contract Included {}

            //- /src/Only.generated.sol
            contract Only {}

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
            exclude: vec!["generated/**".into(), "**/*.generated.sol".into()],
            use_default_excludes: false,
            exclude_hidden_directories: false,
            ..Default::default()
        });
        let mut workspace = Workspace::naked(project.root().to_path_buf());

        let metrics = refresh(&mut workspace, &policy);

        assert_eq!(
            workspace.source_files(),
            &[
                project.path("/.hidden/IncludedWhenHiddenDisabled.sol"),
                project.path("/build/IncludedWhenDefaultsDisabled.sol"),
                project.path("/src/Included.sol"),
            ]
        );
        assert_eq!(metrics.eager, 3);
        assert_eq!(metrics.pruned, 3);
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

            //- /node_modules/project/generated/node_modules/Dependency.sol
            contract Dependency {}
            "#,
        );
        let mut workspace = foundry(&project, "/node_modules/project/foundry.toml");
        refresh(&mut workspace, &WorkspaceIndexPolicy::default());
        assert_eq!(
            workspace.source_files(),
            &[project.path("/node_modules/project/generated/Main.sol")]
        );

        refresh(&mut workspace, &exclude(&["generated/**"]));
        assert!(workspace.source_files().is_empty());
    }
}
