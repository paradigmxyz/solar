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
    /// Include roots approved for eager indexing and topology watching.
    ///
    /// `CompileOpts::include_paths` intentionally keeps every configured Foundry library root so
    /// imports and remappings can resolve external dependencies even when their files are outside
    /// the indexing boundary.
    index_import_only_roots: Vec<PathBuf>,
    source_roots: Vec<PathBuf>,
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
            kind: WorkspaceKind::Naked,
            compile_opts: CompileOpts { base_path: Some(root), ..Default::default() },
            index_import_only_roots: Vec::new(),
            flycheck_source_roots: source_roots.clone(),
            source_roots,
            source_watch_roots: Vec::new(),
            flycheck_watch_roots: Vec::new(),
            git_marker_watch_roots: Vec::new(),
            source_files: Vec::new(),
            source_files_complete: true,
            flycheck_source_files: Vec::new(),
        }
    }

    pub(crate) fn unconfigured() -> Self {
        Self {
            kind: WorkspaceKind::Naked,
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

    pub(crate) fn has_whole_root_foundry_source(&self) -> bool {
        self.kind == WorkspaceKind::Foundry
            && self
                .compile_opts
                .base_path
                .as_deref()
                .is_some_and(|base_path| self.source_roots.iter().any(|root| root == base_path))
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
        let Some((
            source_files,
            source_watch_roots,
            mut git_marker_watch_roots,
            source_files_complete,
        )) = self.collect_source_files(policy, cancellation, metrics, None)
        else {
            return false;
        };
        let Some((
            flycheck_source_files,
            flycheck_watch_roots,
            flycheck_marker_watch_roots,
            flycheck_source_files_complete,
        )) = self.collect_flycheck_source_files(&source_files, policy, cancellation, None)
        else {
            return false;
        };
        git_marker_watch_roots.extend(flycheck_marker_watch_roots);
        git_marker_watch_roots.sort_unstable();
        git_marker_watch_roots.dedup();
        self.source_files = source_files;
        self.source_watch_roots = source_watch_roots;
        self.flycheck_watch_roots = flycheck_watch_roots;
        self.git_marker_watch_roots = git_marker_watch_roots;
        self.flycheck_source_files = flycheck_source_files;
        self.source_files_complete = source_files_complete && flycheck_source_files_complete;
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
                let Some((
                    source_files,
                    source_watch_roots,
                    mut git_marker_watch_roots,
                    source_files_complete,
                )) = workspace.collect_source_files(
                    policy,
                    cancellation,
                    metrics,
                    Some((&index, idx)),
                )
                else {
                    return false;
                };
                let Some((
                    flycheck_source_files,
                    flycheck_watch_roots,
                    flycheck_marker_watch_roots,
                    flycheck_source_files_complete,
                )) = workspace.collect_flycheck_source_files(
                    &source_files,
                    policy,
                    cancellation,
                    Some((&index, idx)),
                )
                else {
                    return false;
                };
                git_marker_watch_roots.extend(flycheck_marker_watch_roots);
                git_marker_watch_roots.sort_unstable();
                git_marker_watch_roots.dedup();
                collected.push((
                    source_files,
                    source_watch_roots,
                    source_files_complete && flycheck_source_files_complete,
                    flycheck_source_files,
                    flycheck_watch_roots,
                    git_marker_watch_roots,
                ));
            }
        }
        for (
            workspace,
            (
                source_files,
                source_watch_roots,
                source_files_complete,
                flycheck_source_files,
                flycheck_watch_roots,
                git_marker_watch_roots,
            ),
        ) in workspaces.iter_mut().zip(collected)
        {
            workspace.source_files = source_files;
            workspace.source_watch_roots = source_watch_roots;
            workspace.flycheck_watch_roots = flycheck_watch_roots;
            workspace.git_marker_watch_roots = git_marker_watch_roots;
            workspace.source_files_complete = source_files_complete;
            workspace.flycheck_source_files = flycheck_source_files;
        }
        true
    }

    fn collect_source_files<'index, 'workspaces>(
        &self,
        policy: &WorkspaceIndexPolicy,
        cancellation: &IndexingCancellation,
        metrics: &mut WorkspaceIndexMetrics,
        ownership: Option<(&'index WorkspacePathIndex<'workspaces>, usize)>,
    ) -> Option<CollectedSourceFiles> {
        let mut source_files = Vec::new();
        let mut source_watch_roots = Vec::new();
        let mut git_marker_watch_roots = Vec::new();
        let mut source_files_complete = true;
        for root in &self.source_roots {
            let workspace_root = self.compile_opts.base_path.as_deref().unwrap_or(root);
            let watch_root_start = source_watch_roots.len();
            let mut collector = SourceFileCollector {
                workspace_root,
                source_root: root,
                source_roots: &self.source_roots,
                import_only_roots: self.index_import_only_roots(),
                policy,
                cancellation,
                metrics,
                files: &mut source_files,
                watch_roots: &mut source_watch_roots,
                marker_watch_roots: &mut git_marker_watch_roots,
                ownership,
                flycheck: false,
                source_files_complete: true,
            };
            let state = collector.collect(root, root == workspace_root);
            source_files_complete &= collector.source_files_complete;
            match state {
                SourceTreeState::Cancelled => return None,
                SourceTreeState::Pruned => continue,
                SourceTreeState::Clean | SourceTreeState::Partitioned => {}
            }
            if source_watch_roots.len() == watch_root_start {
                source_watch_roots.push(if root == workspace_root {
                    SourceWatchRoot::shallow(root)
                } else {
                    SourceWatchRoot::recursive(root)
                });
            }
        }
        source_files.sort_unstable();
        source_files.dedup();
        source_watch_roots.sort_unstable();
        source_watch_roots.dedup();
        git_marker_watch_roots.sort_unstable();
        git_marker_watch_roots.dedup();
        Some((source_files, source_watch_roots, git_marker_watch_roots, source_files_complete))
    }

    fn collect_flycheck_source_files<'index, 'workspaces>(
        &self,
        source_files: &[PathBuf],
        policy: &WorkspaceIndexPolicy,
        cancellation: &IndexingCancellation,
        ownership: Option<(&'index WorkspacePathIndex<'workspaces>, usize)>,
    ) -> Option<CollectedSourceFiles> {
        let mut files = source_files
            .iter()
            .filter(|path| {
                ownership.is_none_or(|(index, workspace_idx)| {
                    index.workspace_idx_for_flycheck_path(policy, path) == Some(workspace_idx)
                })
            })
            .cloned()
            .collect::<Vec<_>>();
        let mut watch_roots = Vec::new();
        let mut marker_watch_roots = Vec::new();
        let mut source_files_complete = true;
        for root in &self.flycheck_source_roots {
            if self.source_roots.contains(root) {
                continue;
            }
            if matches!(std::fs::symlink_metadata(root), Err(error) if error.kind() == io::ErrorKind::NotFound)
            {
                if let Some(ancestor) = root.ancestors().skip(1).find(|ancestor| {
                    std::fs::symlink_metadata(ancestor).is_ok_and(|metadata| metadata.is_dir())
                }) {
                    watch_roots.push(SourceWatchRoot::missing_ancestor(ancestor));
                }
                continue;
            }
            let workspace_root = self.compile_opts.base_path.as_deref().unwrap_or(root);
            let mut metrics = WorkspaceIndexMetrics::default();
            let watch_root_start = watch_roots.len();
            let mut collector = SourceFileCollector {
                workspace_root,
                source_root: root,
                source_roots: &self.flycheck_source_roots,
                import_only_roots: self.index_import_only_roots(),
                policy,
                cancellation,
                metrics: &mut metrics,
                files: &mut files,
                watch_roots: &mut watch_roots,
                marker_watch_roots: &mut marker_watch_roots,
                ownership,
                flycheck: true,
                source_files_complete: true,
            };
            let state = collector.collect(root, false);
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
        is_solidity_file(path)
            && !is_import_only_path(&self.source_roots, self.index_import_only_roots(), path)
            && self.source_roots.iter().any(|root| {
                let workspace_root = self.compile_opts.base_path.as_deref().unwrap_or(root);
                path.starts_with(root) && !policy.excludes_source_file(workspace_root, root, path)
            })
    }

    pub(crate) fn tracks_flycheck_file(&self, policy: &WorkspaceIndexPolicy, path: &Path) -> bool {
        is_solidity_file(path)
            && !is_import_only_path(
                &self.flycheck_source_roots,
                self.index_import_only_roots(),
                path,
            )
            && self.flycheck_source_roots.iter().any(|root| {
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
        Self::load_foundry_inner(path, None, None)
    }

    pub(crate) fn load_foundry_bounded(
        path: PathBuf,
        workspace_roots: &[PathBuf],
        selected_profile: Option<&str>,
    ) -> Result<Self, WorkspaceError> {
        Self::load_foundry_inner(path, Some(workspace_roots), selected_profile)
    }

    fn load_foundry_inner(
        path: PathBuf,
        workspace_roots: Option<&[PathBuf]>,
        selected_profile: Option<&str>,
    ) -> Result<Self, WorkspaceError> {
        let root = manifest_root(&path)?.normalize();
        let profile = load_foundry_document(&path)?.profile_for(selected_profile);
        let approved = |path: &Path| {
            workspace_roots
                .is_none_or(|workspace_roots| is_approved_index_root(path, &root, workspace_roots))
        };
        let source_roots = profile
            .source_roots(&root)
            .into_iter()
            .map(|path| path.normalize())
            .filter(|path| approved(path))
            .collect();
        let flycheck_source_roots = profile
            .flycheck_source_roots(&root)
            .into_iter()
            .map(|path| path.normalize())
            .filter(|path| approved(path))
            .collect();
        let include_paths = profile
            .include_paths(&root)
            .into_iter()
            .map(|path| path.normalize())
            .collect::<Vec<_>>();
        let index_import_only_roots =
            include_paths.iter().filter(|path| approved(path)).cloned().collect::<Vec<_>>();
        let import_remappings = profile.remappings_with_include_paths(&root, &include_paths);
        let compile_opts =
            compile_opts(root.clone(), include_paths, import_remappings, profile.evm_version());

        Ok(Self {
            kind: WorkspaceKind::Foundry,
            index_import_only_roots,
            source_roots,
            flycheck_source_roots,
            compile_opts,
            source_watch_roots: Vec::new(),
            flycheck_watch_roots: Vec::new(),
            git_marker_watch_roots: Vec::new(),
            source_files: Vec::new(),
            source_files_complete: true,
            flycheck_source_files: Vec::new(),
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
    import_entries: Vec<WorkspaceImportPathIndexEntry>,
}

pub(crate) struct WorkspacePathQuery<'a> {
    import_entries: &'a [WorkspaceImportPathIndexEntry],
    path: PathBuf,
}

struct WorkspaceImportPathIndexEntry {
    idx: usize,
    base_depth: usize,
    roots: Vec<WorkspaceImportRoot>,
}

struct WorkspaceImportRoot {
    path: PathBuf,
    depth: usize,
    kind: u8,
}

impl<'a> WorkspacePathIndex<'a> {
    pub(crate) fn new(workspaces: &'a [Workspace]) -> Self {
        let import_entries = workspaces
            .iter()
            .enumerate()
            .map(|(idx, workspace)| WorkspaceImportPathIndexEntry::new(idx, workspace))
            .collect();
        Self { workspaces, import_entries }
    }

    pub(crate) fn query(&self, path: &Path) -> WorkspacePathQuery<'_> {
        WorkspacePathQuery { import_entries: &self.import_entries, path: path.normalize() }
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
        let idx = self.workspace_idx_for_source_region(path)?;
        self.workspaces[idx].tracks_disk_file(policy, path).then_some(idx)
    }

    pub(crate) fn workspace_idx_for_flycheck_path(
        &self,
        policy: &WorkspaceIndexPolicy,
        path: &Path,
    ) -> Option<usize> {
        let idx = self.workspace_idx_for_flycheck_region(path)?;
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

    fn workspace_idx_for_source_region(&self, path: &Path) -> Option<usize> {
        self.workspace_idx_for_region(path, false)
    }

    fn workspace_idx_for_flycheck_region(&self, path: &Path) -> Option<usize> {
        self.workspace_idx_for_region(path, true)
    }

    fn workspace_idx_for_region(&self, path: &Path, flycheck: bool) -> Option<usize> {
        const SOURCE: u8 = 0;
        const BASE: u8 = 1;

        self.workspaces
            .iter()
            .enumerate()
            .filter_map(|(idx, workspace)| {
                let base_match = workspace
                    .compile_opts()
                    .base_path
                    .as_deref()
                    .filter(|base_path| path.starts_with(base_path))
                    .map(|base_path| (base_path.components().count(), BASE));
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
                let base_depth = workspace
                    .compile_opts()
                    .base_path
                    .as_deref()
                    .map_or(0, |base_path| base_path.components().count());
                Some((idx, root_depth, root_kind, base_depth))
            })
            .max_by_key(|&(idx, root_depth, root_kind, base_depth)| {
                (root_depth, root_kind, base_depth, idx)
            })
            .map(|(idx, _, _, _)| idx)
    }
}

impl WorkspacePathQuery<'_> {
    pub(crate) fn workspace_idx_for_path(&self) -> usize {
        self.import_path_matches()
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
        for (idx, root_depth, root_kind, _) in self.import_path_matches() {
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
        self.import_path_matches().map(|(idx, _, _, _)| idx)
    }

    fn import_path_matches(
        &self,
    ) -> impl DoubleEndedIterator<Item = (usize, usize, u8, usize)> + '_ {
        self.import_entries.iter().filter_map(move |entry| {
            let (root_depth, root_kind) = entry
                .roots
                .iter()
                .filter(|root| self.path.starts_with(&root.path))
                .map(|root| (root.depth, root.kind))
                .max()?;
            Some((entry.idx, root_depth, root_kind, entry.base_depth))
        })
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
        for remapping in &workspace.compile_opts().import_remappings {
            let target = Path::new(&remapping.path);
            let path = if target.is_absolute() {
                target.normalize()
            } else if let Some(base_path) = &base_path {
                base_path.join(target).normalize()
            } else {
                continue;
            };
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
        self.metrics.visited += 1;
        let owner = self.ownership.and_then(|(index, _)| {
            if self.flycheck {
                index.workspace_idx_for_flycheck_region(path)
            } else {
                index.workspace_idx_for_source_region(path)
            }
        });
        if let Some((_, workspace_idx)) = self.ownership
            && owner.is_some_and(|idx| idx != workspace_idx)
        {
            self.metrics.pruned += 1;
            return SourceTreeState::Pruned;
        }
        if is_import_only_path(self.source_roots, self.import_only_roots, path) {
            self.metrics.pruned += 1;
            self.source_files_complete = false;
            return SourceTreeState::Pruned;
        }
        let Ok(metadata) = std::fs::symlink_metadata(path) else {
            self.source_files_complete = false;
            return SourceTreeState::Partitioned;
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
            self.source_files_complete = false;
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
        path.starts_with(import_root)
            && !source_roots.iter().any(|source_root| {
                source_root.starts_with(import_root) && path.starts_with(source_root)
            })
    })
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
    fn foundry_workspace_loads_selected_profile_compile_config() {
        let project = TestProject::from_fixture(
            r#"
            //- /default-src/Main.sol
            contract DefaultMain {}

            //- /custom-src/Main.sol
            contract CustomMain {}

            //- /default-libs/pkg/src/Lib.sol
            contract Lib {}

            //- /custom-libs/pkg/src/CustomLib.sol
            contract CustomLib {}

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
            Some("custom"),
        )
        .unwrap();

        assert_eq!(workspace.source_roots(), &[project.path("/custom-src")]);
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
    fn bounded_foundry_workspace_keeps_external_library_compile_config() {
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

        let workspace = Workspace::load_foundry_bounded(
            project.path("/workspace/foundry.toml"),
            &[project.path("/workspace")],
            None,
        )
        .unwrap();
        let opts = workspace.compile_opts();
        let target = project.path("/external/lib/pkg/src/").to_string_lossy().replace('\\', "/");

        assert_eq!(opts.include_paths, [project.path("/external/lib")]);
        assert_eq!(workspace.import_only_roots(), [project.path("/external/lib")]);
        assert!(workspace.index_import_only_roots().is_empty());
        assert!(
            opts.import_remappings
                .iter()
                .any(|remapping| remapping.to_string() == "external/=../external/lib/pkg/src/")
        );
        assert!(
            opts.import_remappings
                .iter()
                .any(|remapping| { remapping.prefix == "pkg/" && remapping.path == target })
        );

        let workspaces = [workspace];
        assert_eq!(
            WorkspacePathIndex::new(&workspaces)
                .workspace_idx_for_import_path(&project.path("/external/lib/pkg/src/Target.sol")),
            Some(0)
        );
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
    fn foundry_flycheck_sources_respect_index_boundaries() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [profile.default]
            src = "src"
            test = "."
            script = "script"
            libs = ["lib"]

            //- /src/Main.sol
            contract Main {}

            //- /test/Tracked.t.sol
            contract TrackedTest {}

            //- /script/Deploy.s.sol
            contract Deploy {}

            //- /script/node_modules/Ignored.s.sol
            contract IgnoredScript {}

            //- /lib/Dependency.sol
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
        let policy = WorkspaceIndexPolicy::new(IndexingOptions {
            exclude: vec!["custom/**".into()],
            ..Default::default()
        });
        let mut workspace = Workspace::load_foundry(project.path("/foundry.toml")).unwrap();
        let mut metrics = WorkspaceIndexMetrics::default();

        assert!(workspace.refresh_source_files(
            &policy,
            &IndexingCancellation::default(),
            &mut metrics,
        ));

        assert_eq!(workspace.source_files(), &[project.path("/src/Main.sol")]);
        assert_eq!(
            workspace.flycheck_source_files(),
            &[
                project.path("/script/Deploy.s.sol"),
                project.path("/src/Main.sol"),
                project.path("/test/Tracked.t.sol"),
            ]
        );
        assert!(workspace.tracks_flycheck_file(&policy, &project.path("/test/Tracked.t.sol")));
        assert!(!workspace.tracks_flycheck_file(&policy, &project.path("/lib/Dependency.sol")));
        assert!(!workspace.tracks_flycheck_file(&policy, &project.path("/custom/Excluded.sol")));
        assert_eq!(metrics.eager, 1);
    }

    #[test]
    fn missing_foundry_flycheck_roots_are_complete_and_watch_their_parent() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [profile.default]
            src = "src"

            //- /src/Main.sol
            contract Main {}
            "#,
        );
        let mut workspace = Workspace::load_foundry(project.path("/foundry.toml")).unwrap();

        refresh_source_files(&mut workspace);

        assert!(workspace.source_files_complete());
        assert_eq!(workspace.source_files(), &[project.path("/src/Main.sol")]);
        assert!(
            workspace
                .flycheck_watch_roots()
                .contains(&SourceWatchRoot::missing_ancestor(&project.path("/")))
        );
    }

    #[test]
    fn foundry_workspace_auto_detects_remappings_from_absolute_library_roots() {
        let project = TestProject::new();
        project.write_file("/shared/lib/pkg/src/Target.sol", "contract Target {}");
        let library = project.path("/shared/lib").to_string_lossy().replace('\\', "/");
        project.write_file(
            "/workspace/foundry.toml",
            &format!("[profile.default]\nlibs = [\"{library}\"]\n"),
        );

        let workspace = Workspace::load_foundry(project.path("/workspace/foundry.toml")).unwrap();
        let remappings = workspace
            .compile_opts()
            .import_remappings
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let target = project.path("/shared/lib/pkg/src").to_string_lossy().replace('\\', "/");

        assert_eq!(remappings, [format!("pkg/={target}/")]);
    }

    #[test]
    fn workspace_path_index_uses_most_specific_base_path() {
        let project = TestProject::new();
        let nested = project.path("/nested");

        let outer = Workspace::naked(project.root().to_path_buf());
        let inner = Workspace::naked(nested);
        let workspaces = vec![outer, inner];
        let index = WorkspacePathIndex::new(&workspaces);

        assert_eq!(index.query(&project.path("/nested/A.sol")).workspace_idx_for_path(), 1);
        assert_eq!(index.query(&project.path("/B.sol")).workspace_idx_for_path(), 0);
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
        let workspaces = vec![
            Workspace::load_foundry(project.path("/first/foundry.toml")).unwrap(),
            Workspace::load_foundry(project.path("/nested/second/foundry.toml")).unwrap(),
            Workspace::load_foundry(project.path("/nested/third/foundry.toml")).unwrap(),
            Workspace::load_foundry(project.path("/source/foundry.toml")).unwrap(),
        ];
        let index = WorkspacePathIndex::new(&workspaces);

        assert_eq!(index.workspace_idx_for_import_path(&project.path("/first/Owned.sol")), Some(0));
        assert_eq!(
            index.workspace_idx_for_import_path(&project.path("/external/source/Owned.sol")),
            Some(3)
        );
        assert_eq!(
            index.workspace_idx_for_import_path(&project.path("/external/source/nested/Owned.sol")),
            Some(1)
        );
        assert_eq!(
            index.workspace_idx_for_import_path(&project.path("/external/tests/Owned.t.sol")),
            Some(1)
        );
        assert_eq!(
            index.workspace_idx_for_import_path(&project.path("/external/scripts/Owned.s.sol")),
            Some(1)
        );
        assert_eq!(
            index
                .workspace_idx_for_import_path(&project.path("/external/include/nested/Owned.sol")),
            Some(1)
        );
        assert_eq!(
            index.workspace_idx_for_import_path(&project.path("/external/priority/Owned.sol")),
            Some(0)
        );
        assert_eq!(
            index.workspace_idx_for_import_path(&project.path("/external/tie/Owned.sol")),
            None
        );
        assert_eq!(
            index.workspace_idx_for_import_path(&project.path("/external/stable/Owned.sol")),
            None
        );
        assert_eq!(
            index.workspace_idx_for_import_path(&project.path("/unowned/Overlay.sol")),
            None
        );
    }

    #[test]
    fn workspace_path_index_normalizes_import_base_paths() {
        let project = TestProject::from_fixture(
            r#"
            //- /container/.keep

            //- /project/foundry.toml
            "#,
        );
        let manifest = project.path("/container/../project/foundry.toml");
        let workspaces = vec![Workspace::load_foundry(manifest).unwrap()];
        let index = WorkspacePathIndex::new(&workspaces);

        assert_eq!(
            index.workspace_idx_for_import_path(&project.path("/project/test/Owned.t.sol")),
            Some(0)
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

            //- /node_modules/project/generated/node_modules/Dependency.sol
            contract Dependency {}
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
