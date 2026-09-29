use crate::{
    FoundryWorkspaceConfigSource, LaunchConfig, commands,
    diagnostics::DiagnosticOwner,
    file_operations::FileMoveBatch,
    flycheck::{FlycheckConfig, FlycheckInitializationOptions},
    import_resolution::ImportResolutionContext,
    proto,
    workspace::{
        FoundryConfigContext, SourceWatchRoot, Workspace, WorkspaceEditScope, WorkspaceError,
        WorkspaceKind, WorkspacePathIndex,
        index_policy::{
            IndexingCancellation, IndexingOptions, WorkspaceIndexMetrics, WorkspaceIndexPolicy,
        },
        is_approved_index_root,
        manifest::ProjectManifest,
        workspace_idx_containing_path,
    },
};
use lsp_types::{
    CallHierarchyServerCapability, CodeActionKind, CodeActionOptions, CodeActionProviderCapability,
    CodeLensOptions as CodeLensServerOptions, CompletionOptions, DeclarationCapability, Diagnostic,
    DiagnosticOptions, DiagnosticServerCapabilities, DiagnosticTag, DocumentLinkOptions,
    ExecuteCommandOptions, FileOperationFilter, FileOperationPattern, FileOperationPatternKind,
    FileOperationRegistrationOptions, FoldingRangeProviderCapability, HoverProviderCapability,
    ImplementationProviderCapability, InitializeParams, MarkupKind, OneOf, RenameOptions,
    SaveOptions, SelectionRangeProviderCapability, ServerCapabilities, SignatureHelpOptions,
    TextDocumentSyncCapability, TextDocumentSyncKind, TextDocumentSyncOptions,
    TextDocumentSyncSaveOptions, TypeDefinitionProviderCapability, Url, WatchKind,
    WorkDoneProgressOptions, WorkspaceFileOperationsServerCapabilities, WorkspaceFolder,
    WorkspaceFoldersServerCapabilities, WorkspaceServerCapabilities,
};
use normalize_path::NormalizePath;
use serde::Deserialize;
use solar_interface::data_structures::map::FxHashSet;
use std::{
    collections::BTreeSet,
    env,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
    time::Duration,
};
use tracing::{info, warn};

const DEFAULT_SOURCE_CHANGE_DEBOUNCE: Duration = Duration::from_millis(150);

/// The LSP config.
///
/// This struct is internal only and should not be serialized or deserialized. Instead, values in
/// this struct are the full view of all merged config sources, such as `initialization_opts`,
/// on-disk config files (e.g. `foundry.toml`).
#[derive(Clone, Debug)]
pub(crate) struct Config {
    workspace_roots: Vec<PathBuf>,
    forge_path: PathBuf,
    selected_profile: Option<String>,
    foundry_workspace_config_source: FoundryWorkspaceConfigSource,
    workspaces: Vec<Workspace>,
    workspace_path_cache: Arc<OnceLock<Arc<crate::workspace::WorkspacePathIndexCache>>>,
    workspace_edit_scope: Arc<OnceLock<WorkspaceEditScope>>,
    manifest_watch_roots: Vec<SourceWatchRoot>,
    git_marker_watch_roots: Vec<PathBuf>,
    index_policy: WorkspaceIndexPolicy,
    index_metrics: WorkspaceIndexMetrics,
    analysis_source_files_complete: bool,
    flycheck_options: FlycheckInitializationOptions,
    flychecks: Vec<FlycheckConfig>,
    watched_file_dynamic_registration: bool,
    watched_file_relative_pattern_support: bool,
    workspace_edit_document_changes: bool,
    code_action_literals: bool,
    code_action_is_preferred: bool,
    diagnostic_delivery: DiagnosticDelivery,
    publish_diagnostics_related_information: bool,
    publish_diagnostics_tags: Vec<DiagnosticTag>,
    publish_diagnostics_data: bool,
    pull_diagnostics_data: bool,
    code_lens_refresh_support: bool,
    diagnostic_refresh_support: bool,
    inlay_hint_refresh_support: bool,
    work_done_progress: bool,
    hierarchical_document_symbol_support: bool,
    completion: CompletionClientOptions,
    signature_help: SignatureHelpClientOptions,
    source_change_debounce: Duration,
    progress_delay: Duration,
    progress_create_timeout: Duration,
    formatter_timeout: Duration,
    flycheck_timeout: Duration,
    code_lens: CodeLensConfig,
}

pub(crate) struct WorkspaceDiscoveryResult {
    pub(crate) workspaces: Vec<Workspace>,
    pub(crate) manifest_watch_roots: Vec<SourceWatchRoot>,
    pub(crate) git_marker_watch_roots: Vec<PathBuf>,
    pub(crate) metrics: WorkspaceIndexMetrics,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct WatchedFileSpec {
    pub(crate) base: PathBuf,
    pub(crate) pattern: &'static str,
    pub(crate) kind: WatchKind,
}

impl WatchedFileSpec {
    pub(crate) fn new(base: PathBuf, pattern: &'static str) -> Self {
        Self::with_kind(base, pattern, WatchKind::Create | WatchKind::Change | WatchKind::Delete)
    }

    fn with_kind(base: PathBuf, pattern: &'static str, kind: WatchKind) -> Self {
        Self { base, pattern, kind }
    }

    fn create_delete(base: PathBuf, pattern: &'static str) -> Self {
        Self::with_kind(base, pattern, WatchKind::Create | WatchKind::Delete)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum DiagnosticDelivery {
    #[default]
    Push,
    Pull,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            workspace_roots: Vec::new(),
            forge_path: PathBuf::from("forge"),
            selected_profile: None,
            foundry_workspace_config_source: FoundryWorkspaceConfigSource::default(),
            workspaces: Vec::new(),
            workspace_path_cache: Arc::new(OnceLock::new()),
            workspace_edit_scope: Arc::new(OnceLock::new()),
            manifest_watch_roots: Vec::new(),
            git_marker_watch_roots: Vec::new(),
            index_policy: WorkspaceIndexPolicy::default(),
            index_metrics: WorkspaceIndexMetrics::default(),
            analysis_source_files_complete: true,
            flycheck_options: FlycheckInitializationOptions::default(),
            flychecks: Vec::new(),
            watched_file_dynamic_registration: false,
            watched_file_relative_pattern_support: false,
            workspace_edit_document_changes: false,
            code_action_literals: false,
            code_action_is_preferred: false,
            diagnostic_delivery: DiagnosticDelivery::Push,
            publish_diagnostics_related_information: false,
            publish_diagnostics_tags: Vec::new(),
            publish_diagnostics_data: false,
            pull_diagnostics_data: false,
            code_lens_refresh_support: false,
            diagnostic_refresh_support: false,
            inlay_hint_refresh_support: false,
            work_done_progress: false,
            hierarchical_document_symbol_support: false,
            completion: CompletionClientOptions::default(),
            signature_help: SignatureHelpClientOptions::default(),
            source_change_debounce: DEFAULT_SOURCE_CHANGE_DEBOUNCE,
            progress_delay: Duration::from_millis(250),
            progress_create_timeout: Duration::from_secs(1),
            formatter_timeout: Duration::from_secs(30),
            flycheck_timeout: Duration::from_secs(30),
            code_lens: CodeLensConfig::default(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct CompletionClientOptions {
    pub(crate) snippet_support: bool,
    pub(crate) markdown_documentation: bool,
    pub(crate) resolve_documentation: bool,
}

impl Default for CompletionClientOptions {
    fn default() -> Self {
        Self { snippet_support: false, markdown_documentation: false, resolve_documentation: true }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SignatureHelpClientOptions {
    pub(crate) label_offsets: bool,
    pub(crate) markdown_documentation: bool,
    pub(crate) signature_active_parameter: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct CodeLensConfig {
    pub(crate) enable: bool,
    pub(crate) selectors: bool,
    pub(crate) references: bool,
    pub(crate) inheritance: bool,
    pub(crate) client_commands: bool,
}

impl Default for CodeLensConfig {
    fn default() -> Self {
        Self {
            enable: true,
            selectors: true,
            references: true,
            inheritance: true,
            client_commands: false,
        }
    }
}

impl CodeLensConfig {
    pub(crate) fn is_active(self) -> bool {
        self.enable && (self.selectors || self.references || self.inheritance)
    }
}

impl Config {
    pub(crate) fn supports_watched_file_dynamic_registration(&self) -> bool {
        self.watched_file_dynamic_registration
    }

    pub(crate) fn supports_watched_file_relative_patterns(&self) -> bool {
        self.watched_file_relative_pattern_support
    }

    pub(crate) fn supports_workspace_edit_document_changes(&self) -> bool {
        self.workspace_edit_document_changes
    }

    pub(crate) fn supports_code_action_literals(&self) -> bool {
        self.code_action_literals
    }

    pub(crate) fn supports_code_action_is_preferred(&self) -> bool {
        self.code_action_is_preferred
    }

    pub(crate) fn uses_push_diagnostics(&self) -> bool {
        self.diagnostic_delivery == DiagnosticDelivery::Push
    }

    pub(crate) fn uses_pull_diagnostics(&self) -> bool {
        self.diagnostic_delivery == DiagnosticDelivery::Pull
    }

    pub(crate) fn supports_publish_diagnostics_data(&self) -> bool {
        self.uses_push_diagnostics() && self.publish_diagnostics_data
    }

    /// Adapts an outgoing diagnostic without changing the server's cached diagnostic or fix data.
    pub(crate) fn prepare_publish_diagnostic(&self, diagnostic: &mut Diagnostic) {
        if !self.publish_diagnostics_related_information
            && let Some(related) = diagnostic.related_information.take()
        {
            for information in related {
                crate::diagnostics::presentation::append_message(
                    &mut diagnostic.message,
                    &information.message,
                );
            }
        }
        if let Some(tags) = &mut diagnostic.tags {
            tags.retain(|tag| self.publish_diagnostics_tags.contains(tag));
            if tags.is_empty() {
                diagnostic.tags = None;
            }
        }
    }

    pub(crate) fn supports_pull_diagnostics_data(&self) -> bool {
        self.uses_pull_diagnostics() && self.pull_diagnostics_data
    }

    pub(crate) fn supports_code_action_diagnostic_data(&self) -> bool {
        self.supports_publish_diagnostics_data() || self.supports_pull_diagnostics_data()
    }

    pub(crate) fn supports_code_lens_refresh(&self) -> bool {
        self.code_lens_refresh_support
    }

    pub(crate) fn supports_diagnostic_refresh(&self) -> bool {
        self.diagnostic_refresh_support
    }

    pub(crate) fn supports_inlay_hint_refresh(&self) -> bool {
        self.inlay_hint_refresh_support
    }

    pub(crate) fn supports_work_done_progress(&self) -> bool {
        self.work_done_progress
    }

    pub(crate) fn supports_hierarchical_document_symbols(&self) -> bool {
        self.hierarchical_document_symbol_support
    }

    pub(crate) fn source_change_debounce(&self) -> Duration {
        self.source_change_debounce
    }

    pub(crate) fn progress_delay(&self) -> Duration {
        self.progress_delay
    }

    pub(crate) fn progress_create_timeout(&self) -> Duration {
        self.progress_create_timeout
    }

    pub(crate) fn formatter_timeout(&self) -> Duration {
        self.formatter_timeout
    }

    pub(crate) fn flycheck_timeout(&self) -> Duration {
        self.flycheck_timeout
    }

    pub(crate) fn completion_options(&self) -> CompletionClientOptions {
        self.completion
    }

    #[cfg(test)]
    pub(crate) fn enable_completion_snippets(&mut self) {
        self.completion.snippet_support = true;
    }

    pub(crate) fn signature_help_options(&self) -> SignatureHelpClientOptions {
        self.signature_help
    }

    pub(crate) fn code_lens_options(&self) -> CodeLensConfig {
        self.code_lens
    }

    #[cfg(test)]
    pub(crate) fn enable_signature_help_label_offsets(&mut self) {
        self.signature_help.label_offsets = true;
    }

    #[cfg(test)]
    pub(crate) fn enable_code_lens_client_commands(&mut self) {
        self.code_lens.client_commands = true;
    }

    pub(crate) fn workspaces(&self) -> &[Workspace] {
        &self.workspaces
    }

    pub(crate) fn import_resolution_context(
        &self,
        path: &Path,
    ) -> Option<ImportResolutionContext<'_>> {
        let entries = self.workspace_path_index().clone_import_entries();
        ImportResolutionContext::for_workspaces_with_index(&self.workspaces, path, entries)
    }

    fn invalidate_workspace_path_cache(&mut self) {
        self.workspace_path_cache = Arc::new(OnceLock::new());
        self.workspace_edit_scope = Arc::new(OnceLock::new());
    }

    /// Returns cached lexical ownership; filesystem resolution remains request-local.
    pub(crate) fn workspace_edit_scope(&self) -> &WorkspaceEditScope {
        self.workspace_edit_scope
            .get_or_init(|| WorkspaceEditScope::new(&self.workspace_roots, &self.workspaces))
    }

    pub(crate) fn workspace_path_index(&self) -> WorkspacePathIndex<'_> {
        let cache = Arc::clone(
            self.workspace_path_cache
                .get_or_init(|| Arc::new(WorkspacePathIndex::cache(&self.workspaces))),
        );
        WorkspacePathIndex::with_cache(&self.workspaces, cache)
    }

    pub(crate) fn is_index_import_only_path(&self, path: &Path) -> bool {
        let path = path.normalize();
        self.workspaces.iter().any(|workspace| {
            workspace
                .index_import_only_roots()
                .iter()
                .any(|root| path.starts_with(root.normalize()))
        })
    }

    pub(crate) fn workspace_roots(&self) -> &[PathBuf] {
        &self.workspace_roots
    }

    pub(crate) fn watched_file_specs(&self) -> Vec<WatchedFileSpec> {
        let mut specs = BTreeSet::new();
        let watch_nested_repositories = self.watches_nested_repository_markers();
        for root in &self.workspace_roots {
            for pattern in ["foundry.toml", "remappings.txt"] {
                specs.insert(WatchedFileSpec::new(root.clone(), pattern));
            }
            specs.extend(
                root.ancestors()
                    .skip(1)
                    .map(|ancestor| WatchedFileSpec::new(ancestor.to_path_buf(), "foundry.toml")),
            );
        }

        for root in &self.manifest_watch_roots {
            if self
                .workspace_roots
                .iter()
                .any(|workspace_root| root.path.starts_with(workspace_root))
            {
                insert_watch_root_specs(&mut specs, root, false, watch_nested_repositories);
            }
        }

        for workspace in &self.workspaces {
            let Some(base_path) = workspace.compile_opts().base_path.as_deref() else {
                continue;
            };
            if !self
                .workspace_roots
                .iter()
                .any(|root| root.starts_with(base_path) || base_path.starts_with(root))
            {
                continue;
            }
            specs.insert(WatchedFileSpec::new(base_path.to_path_buf(), "foundry.toml"));
            specs.insert(WatchedFileSpec::new(base_path.to_path_buf(), "remappings.txt"));

            for root in workspace.index_import_only_roots() {
                if is_approved_index_root(root, base_path, &self.workspace_roots) {
                    specs.insert(WatchedFileSpec::create_delete(root.clone(), "*"));
                }
            }

            for root in
                workspace.source_watch_roots().iter().chain(workspace.flycheck_watch_roots())
            {
                if is_approved_index_root(&root.path, base_path, &self.workspace_roots) {
                    insert_watch_root_specs(&mut specs, root, true, watch_nested_repositories);
                }
            }
        }
        if watch_nested_repositories {
            for root in &self.git_marker_watch_roots {
                if self.is_approved_watch_root(root) {
                    specs.insert(WatchedFileSpec::create_delete(root.clone(), ".git"));
                }
            }
        }
        specs.into_iter().collect()
    }

    pub(crate) fn tracks_source_file(&self, path: &Path) -> bool {
        self.workspace_path_index()
            .workspace_idx_for_source_path(&self.index_policy, path)
            .is_some()
    }

    pub(crate) fn may_omit_source_files(&self) -> bool {
        !self.analysis_source_files_complete
            || self.workspaces.iter().any(|workspace| {
                !workspace.source_files_complete()
                    || workspace.has_unindexed_flycheck_source_files()
            })
    }

    pub(crate) fn mark_analysis_source_files_incomplete(&mut self) {
        self.analysis_source_files_complete = false;
    }

    pub(crate) fn tracks_flycheck_file(&self, path: &Path) -> bool {
        self.workspace_path_index()
            .workspace_idx_for_flycheck_path(&self.index_policy, path)
            .is_some()
    }

    pub(crate) fn tracks_watched_source_file(&self, path: &Path) -> bool {
        self.tracks_source_file(path) || self.tracks_flycheck_file(path)
    }

    fn watch_roots(&self) -> impl Iterator<Item = &SourceWatchRoot> {
        self.manifest_watch_roots.iter().chain(self.workspaces.iter().flat_map(|workspace| {
            workspace.source_watch_roots().iter().chain(workspace.flycheck_watch_roots())
        }))
    }

    fn is_approved_watch_root(&self, path: &Path) -> bool {
        self.workspace_roots.iter().any(|root| path.starts_with(root))
            || self.workspaces.iter().any(|workspace| {
                workspace
                    .compile_opts()
                    .base_path
                    .as_deref()
                    .is_some_and(|base_path| path.starts_with(base_path))
            })
    }

    pub(crate) fn nested_repository_marker_event_is_relevant(&self, path: &Path) -> bool {
        if !self.watches_nested_repository_markers()
            || path.file_name().is_none_or(|name| name != ".git")
        {
            return false;
        }
        let Some(directory) = path.parent() else { return false };
        if self.git_marker_watch_roots.iter().any(|root| root == directory) {
            return true;
        }
        self.watch_roots().any(|root| covers_directory(root, directory))
    }

    pub(crate) fn shallow_watch_event_is_relevant(&self, path: &Path) -> bool {
        let Some(parent) = path.parent() else { return false };
        let covers = |root: &SourceWatchRoot| !root.recursive && root.path == parent;
        self.watch_roots().any(covers)
            || self.workspaces.iter().any(|workspace| {
                workspace
                    .index_import_only_roots()
                    .iter()
                    .any(|root| self.is_approved_watch_root(root) && root == parent)
            })
    }

    pub(crate) fn workspace_config_event_is_relevant(&self, path: &Path) -> bool {
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            return false;
        };
        let Some(directory) = path.parent() else { return false };

        if file_name == "remappings.txt" {
            return self.workspace_roots.iter().any(|root| root == directory)
                || self.workspaces.iter().any(|workspace| {
                    workspace.compile_opts().base_path.as_deref() == Some(directory)
                });
        }
        if file_name != "foundry.toml" {
            return false;
        }

        if self.workspace_roots.iter().any(|root| root == directory || root.starts_with(directory))
        {
            return true;
        }
        self.workspaces
            .iter()
            .any(|workspace| workspace.compile_opts().base_path.as_deref() == Some(directory))
            || self.watch_roots().any(|root| covers_directory(root, directory))
    }

    pub(crate) fn index_policy(&self) -> &WorkspaceIndexPolicy {
        &self.index_policy
    }

    pub(crate) fn watches_nested_repository_markers(&self) -> bool {
        self.index_policy.excludes_nested_repositories()
    }

    pub(crate) fn index_metrics(&self) -> WorkspaceIndexMetrics {
        self.index_metrics
    }

    pub(crate) fn tracked_source_files_under(&self, roots: &[PathBuf]) -> Vec<PathBuf> {
        let mut files = self
            .workspaces
            .iter()
            .flat_map(Workspace::source_files)
            .filter(|path| roots.iter().any(|root| path.starts_with(root)))
            .cloned()
            .collect::<Vec<_>>();
        files.sort();
        files.dedup();
        files
    }

    pub(crate) fn file_operation_paths_under(&self, roots: &[PathBuf]) -> Vec<PathBuf> {
        let mut paths = self.tracked_source_files_under(roots);
        paths.extend(
            self.workspaces
                .iter()
                .flat_map(Workspace::flycheck_source_files)
                .filter(|path| roots.iter().any(|root| path.starts_with(root)))
                .cloned(),
        );
        paths.extend(
            self.workspaces
                .iter()
                .filter(|workspace| workspace.kind() == WorkspaceKind::Foundry)
                .filter_map(|workspace| workspace.compile_opts().base_path.as_deref())
                .flat_map(|base_path| {
                    ["foundry.toml", "remappings.txt"].map(|name| base_path.join(name))
                })
                .filter(|path| roots.iter().any(|root| path.starts_with(root))),
        );
        paths.sort();
        paths.dedup();
        paths
    }

    pub(crate) fn forge_path(&self) -> PathBuf {
        self.forge_path.clone()
    }

    pub(crate) fn formatter_root_for_path(&self, path: &Path) -> Option<PathBuf> {
        ProjectManifest::discover_in_parents(path)
            .and_then(|ProjectManifest::Foundry(path)| path.parent().map(Path::to_path_buf))
            .or_else(|| {
                workspace_idx_containing_path(&self.workspaces, path)
                    .and_then(|idx| self.workspaces[idx].compile_opts().base_path.clone())
            })
            .or_else(|| path.parent().map(Path::to_path_buf))
    }

    fn matching_flychecks_for_path<'a>(
        &'a self,
        path: &'a Path,
    ) -> impl Iterator<Item = &'a FlycheckConfig> + 'a {
        self.flychecks.iter().filter(move |flycheck| {
            flycheck.applies_to(path)
                || self.workspaces.iter().any(|workspace| {
                    workspace.compile_opts().base_path.as_deref()
                        == Some(flycheck.workspace_root.as_path())
                        && workspace.tracks_flycheck_file(&self.index_policy, path)
                })
        })
    }

    pub(crate) fn flychecks_for_path(&self, path: &Path) -> Vec<FlycheckConfig> {
        self.matching_flychecks_for_path(path).cloned().collect()
    }

    pub(crate) fn flycheck_owners_for_path<'a>(
        &'a self,
        path: &'a Path,
    ) -> impl Iterator<Item = DiagnosticOwner> + 'a {
        self.matching_flychecks_for_path(path).map(FlycheckConfig::owner)
    }

    pub(crate) fn flycheck_owners(&self) -> impl Iterator<Item = DiagnosticOwner> + '_ {
        self.flychecks.iter().map(FlycheckConfig::owner)
    }

    #[cfg(test)]
    pub(crate) fn rediscover_workspaces(&mut self) -> Vec<DiagnosticOwner> {
        self.try_rediscover_workspaces().unwrap_or_default()
    }

    pub(crate) fn try_rediscover_workspaces(
        &mut self,
    ) -> Result<Vec<DiagnosticOwner>, WorkspaceError> {
        let cancellation = IndexingCancellation::default();
        let Some(result) = self.try_discover_workspaces(&cancellation)? else {
            return Ok(Vec::new());
        };
        Ok(self.apply_workspace_discovery(result))
    }

    #[cfg(any(test, feature = "bench"))]
    pub(crate) fn discover_workspaces(
        &self,
        cancellation: &IndexingCancellation,
    ) -> Option<WorkspaceDiscoveryResult> {
        self.try_discover_workspaces(cancellation).ok().flatten()
    }

    pub(crate) fn try_discover_workspaces(
        &self,
        cancellation: &IndexingCancellation,
    ) -> Result<Option<WorkspaceDiscoveryResult>, WorkspaceError> {
        let start = std::time::Instant::now();
        let mut metrics = WorkspaceIndexMetrics::default();
        let mut workspaces = Vec::new();
        let mut manifest_watch_roots = Vec::new();
        let mut git_marker_watch_roots = Vec::new();
        let mut seen_manifests = FxHashSet::default();
        let mut foundry_config = FoundryConfigContext::from_source(
            self.selected_profile.as_deref(),
            &self.foundry_workspace_config_source,
        );
        for root in &self.workspace_roots {
            if cancellation.is_cancelled() {
                return Ok(None);
            }
            let Some((discovered, discovered_watch_roots, discovered_marker_watch_roots)) =
                ProjectManifest::discover_all_with_watch_roots(
                    std::slice::from_ref(root),
                    &self.workspace_roots,
                    &self.index_policy,
                    cancellation,
                    &mut metrics,
                    &mut foundry_config,
                )
            else {
                return Ok(None);
            };
            manifest_watch_roots.extend(discovered_watch_roots);
            git_marker_watch_roots.extend(discovered_marker_watch_roots);
            info!(?root, ?discovered, "discovered projects");
            if discovered.is_empty() {
                info!(?root, "no project manifests found");
                workspaces.push(Workspace::naked(root.clone()));
                continue;
            }

            load_discovered_workspaces(
                discovered,
                &self.workspace_roots,
                &mut foundry_config,
                &mut seen_manifests,
                &mut workspaces,
            )?;
        }
        info!(workspaces = ?workspaces.iter().map(Workspace::kind).collect::<Vec<_>>(), "loaded workspaces");
        if cancellation.is_cancelled() {
            return Ok(None);
        }
        loop {
            if !Workspace::refresh_all_source_files(
                &mut workspaces,
                &self.index_policy,
                cancellation,
                &mut metrics,
            ) {
                return Ok(None);
            }
            let mut roots = workspaces
                .iter()
                .filter(|workspace| workspace.kind() == WorkspaceKind::Foundry)
                .flat_map(|workspace| {
                    workspace.source_watch_roots().iter().chain(workspace.flycheck_watch_roots())
                })
                .cloned()
                .collect::<Vec<_>>();
            roots.sort_unstable();
            roots.dedup();
            let Some(discovered) =
                ProjectManifest::discover_in_source_watch_roots(&roots, cancellation, &mut metrics)
            else {
                return Ok(None);
            };
            let loaded = load_discovered_workspaces(
                discovered,
                &self.workspace_roots,
                &mut foundry_config,
                &mut seen_manifests,
                &mut workspaces,
            )?;
            if loaded == 0 {
                break;
            }
        }
        WorkspacePathIndex::reconcile_source_files(
            &mut workspaces,
            &self.index_policy,
            &mut metrics,
        );
        git_marker_watch_roots
            .extend(workspaces.iter().flat_map(Workspace::git_marker_watch_roots).cloned());
        metrics.discovery_duration = start.elapsed();
        info!(
            visited = metrics.visited,
            pruned = metrics.pruned,
            eager = metrics.eager,
            duration = ?metrics.discovery_duration,
            "completed workspace discovery"
        );
        manifest_watch_roots.sort_unstable();
        manifest_watch_roots.dedup();
        git_marker_watch_roots.sort_unstable();
        git_marker_watch_roots.dedup();
        Ok(Some(WorkspaceDiscoveryResult {
            workspaces,
            manifest_watch_roots,
            git_marker_watch_roots,
            metrics,
        }))
    }

    pub(crate) fn apply_workspace_discovery(
        &mut self,
        result: WorkspaceDiscoveryResult,
    ) -> Vec<DiagnosticOwner> {
        self.invalidate_workspace_path_cache();
        self.workspaces = result.workspaces;
        self.manifest_watch_roots = result.manifest_watch_roots;
        self.git_marker_watch_roots = result.git_marker_watch_roots;
        self.index_metrics = result.metrics;
        self.refresh_flychecks()
    }

    pub(crate) fn remove_workspace(&mut self, path: &Path) {
        if let Some(pos) = self.workspace_roots.iter().position(|it| it == path) {
            self.invalidate_workspace_path_cache();
            self.workspace_roots.remove(pos);
        }
    }

    pub(crate) fn add_workspaces(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        self.invalidate_workspace_path_cache();
        for path in paths {
            if !self.workspace_roots.contains(&path) {
                self.workspace_roots.push(path);
            }
        }
    }

    pub(crate) fn replace_workspace_roots(&mut self, roots: Vec<PathBuf>) {
        self.invalidate_workspace_path_cache();
        self.workspace_roots = roots;
    }

    pub(crate) fn reconcile_workspace_roots(
        &mut self,
        moves: &FileMoveBatch,
        deleted_paths: &[PathBuf],
    ) -> bool {
        let previous = self.workspace_roots.clone();
        self.workspace_roots
            .retain(|root| !deleted_paths.iter().any(|deleted| root.starts_with(deleted)));
        for root in &mut self.workspace_roots {
            if let Some((_, new_root)) = moves.map_path(root) {
                *root = new_root;
            }
        }
        let mut seen = FxHashSet::default();
        self.workspace_roots.retain(|root| seen.insert(root.clone()));
        let changed = self.workspace_roots != previous;
        if changed {
            self.invalidate_workspace_path_cache();
        }
        changed
    }

    fn source_and_flycheck_workspace_idx(&self, path: &Path) -> (Option<usize>, Option<usize>) {
        let index = self.workspace_path_index();
        (
            index.workspace_idx_for_source_path(&self.index_policy, path),
            index.workspace_idx_for_flycheck_path(&self.index_policy, path),
        )
    }

    pub(crate) fn add_source_file(&mut self, path: PathBuf) {
        let (source_idx, flycheck_idx) = self.source_and_flycheck_workspace_idx(&path);
        if let Some(idx) = source_idx {
            self.workspaces[idx].add_source_file(&self.index_policy, path.clone());
        }
        if let Some(idx) = flycheck_idx {
            self.workspaces[idx].add_flycheck_source_file(&self.index_policy, &path);
        }
    }

    pub(crate) fn remove_source_file(&mut self, path: &Path) {
        let (source_idx, flycheck_idx) = self.source_and_flycheck_workspace_idx(path);
        if let Some(idx) = source_idx {
            self.workspaces[idx].remove_source_file(path);
        }
        if let Some(idx) = flycheck_idx {
            self.workspaces[idx].remove_flycheck_source_file(path);
        }
    }

    fn refresh_flychecks(&mut self) -> Vec<DiagnosticOwner> {
        let mut removed_owners =
            self.flychecks.iter().map(FlycheckConfig::owner).collect::<FxHashSet<_>>();
        self.flychecks = self.flycheck_options.configs(
            &self.workspaces,
            &self.forge_path,
            self.selected_profile.as_deref(),
        );

        for owner in self.flychecks.iter().map(FlycheckConfig::owner) {
            removed_owners.remove(&owner);
        }

        let mut removed_owners = removed_owners.into_iter().collect::<Vec<_>>();
        removed_owners.sort();
        info!(flychecks = ?self.flychecks.iter().map(|it| &it.id).collect::<Vec<_>>(), "loaded flychecks");
        removed_owners
    }
}

fn load_discovered_workspaces(
    manifests: impl IntoIterator<Item = ProjectManifest>,
    workspace_roots: &[PathBuf],
    foundry_config: &mut FoundryConfigContext<'_>,
    seen_manifests: &mut FxHashSet<ProjectManifest>,
    workspaces: &mut Vec<Workspace>,
) -> Result<usize, WorkspaceError> {
    let mut loaded = 0;
    for manifest in manifests {
        if !seen_manifests.insert(manifest.clone()) {
            continue;
        }
        let ProjectManifest::Foundry(path) = manifest;
        let fallback_root = path.parent().map(PathBuf::from);
        match Workspace::load_foundry_bounded(path, workspace_roots, foundry_config) {
            Ok(workspace) => workspaces.push(workspace),
            Err(error) => {
                warn!(%error, "failed to load workspace");
                if matches!(error, WorkspaceError::HostConfig { .. }) {
                    return Err(error);
                }
                if let Some(root) = fallback_root {
                    workspaces.push(Workspace::naked(root));
                }
            }
        }
        loaded += 1;
    }
    Ok(loaded)
}

fn covers_directory(root: &SourceWatchRoot, directory: &Path) -> bool {
    if root.recursive { directory.starts_with(&root.path) } else { directory == root.path }
}

fn insert_watch_root_specs(
    specs: &mut BTreeSet<WatchedFileSpec>,
    root: &SourceWatchRoot,
    sources: bool,
    nested_repositories: bool,
) {
    let SourceWatchRoot { path, recursive, watch_contents } = root;
    if !watch_contents {
        specs.insert(WatchedFileSpec::create_delete(path.clone(), "*"));
    } else if *recursive {
        if sources {
            specs.insert(WatchedFileSpec::new(path.clone(), "**/*.sol"));
        }
        specs.insert(WatchedFileSpec::new(path.clone(), "**/foundry.toml"));
        if nested_repositories {
            specs.insert(WatchedFileSpec::create_delete(path.clone(), "**/.git"));
        }
    } else {
        // Discover new child directories without recursively watching pruned paths.
        specs.insert(WatchedFileSpec::create_delete(path.clone(), "*"));
        if sources {
            specs.insert(WatchedFileSpec::with_kind(path.clone(), "*.sol", WatchKind::Change));
        }
        specs.insert(WatchedFileSpec::new(path.clone(), "foundry.toml"));
    }
}

fn workspace_file_operation_options() -> FileOperationRegistrationOptions {
    let filters = [
        ("**/*.sol", FileOperationPatternKind::File),
        ("**/foundry.toml", FileOperationPatternKind::File),
        ("**/remappings.txt", FileOperationPatternKind::File),
        ("**", FileOperationPatternKind::Folder),
    ]
    .map(|(glob, matches)| FileOperationFilter {
        scheme: Some("file".into()),
        pattern: FileOperationPattern { glob: glob.into(), matches: Some(matches), options: None },
    });
    FileOperationRegistrationOptions { filters: filters.into() }
}

fn workspace_roots_from_initialize(
    workspace_folders: Option<Vec<WorkspaceFolder>>,
    root_uri: Option<Url>,
    fallback_root: impl FnOnce() -> Option<PathBuf>,
) -> Vec<PathBuf> {
    let workspace_roots = workspace_folders
        .into_iter()
        .flatten()
        .filter_map(|it| proto::normalize_file_uri(it.uri).to_file_path().ok())
        .collect::<Vec<_>>();
    if !workspace_roots.is_empty() {
        return workspace_roots;
    }

    root_uri
        .and_then(|uri| proto::normalize_file_uri(uri).to_file_path().ok())
        .or_else(fallback_root)
        .into_iter()
        .collect()
}

#[cfg(any(test, feature = "bench"))]
pub(crate) fn negotiate_capabilities(params: InitializeParams) -> (ServerCapabilities, Config) {
    negotiate_capabilities_with_pull_diagnostic_data(params, false, &LaunchConfig::default())
}

pub(crate) fn negotiate_capabilities_with_pull_diagnostic_data(
    params: InitializeParams,
    pull_diagnostics_data: bool,
    launch_config: &LaunchConfig,
) -> (ServerCapabilities, Config) {
    let capabilities = params.capabilities;
    let initialization_options = params.initialization_options;
    #[allow(deprecated)]
    let root_uri = params.root_uri;
    let workspace_folders = params.workspace_folders;
    let option = |key| initialization_options.as_ref().and_then(|options| options.get(key));
    let forge_path = option("forgePath")
        .and_then(|path| PathBuf::deserialize(path).ok())
        .or_else(|| launch_config.default_forge_path().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("forge"));
    let source_change_debounce = option("sourceChangeDebounce")
        .and_then(serde_json::Value::as_u64)
        .map(Duration::from_millis)
        .unwrap_or(DEFAULT_SOURCE_CHANGE_DEBOUNCE);
    let code_lens = option("codeLens")
        .and_then(|value| CodeLensConfig::deserialize(value).ok())
        .unwrap_or_default();
    let flycheck_options = FlycheckInitializationOptions::from_json(initialization_options.clone());
    let indexing_options = IndexingOptions::from_json(initialization_options);
    let index_policy = WorkspaceIndexPolicy::new(indexing_options);

    // The latest LSP spec mandates clients report `workspace_folders`, but some might still report
    // `root_uri`.
    let workspace = capabilities.workspace.as_ref();
    let text_document = capabilities.text_document.as_ref();
    let watched_files = workspace.and_then(|it| it.did_change_watched_files.as_ref());
    let watched_file_dynamic_registration =
        watched_files.and_then(|it| it.dynamic_registration).unwrap_or(false);
    let watched_file_relative_pattern_support =
        watched_files.and_then(|it| it.relative_pattern_support).unwrap_or(false);
    let workspace_edit_document_changes =
        workspace.and_then(|it| it.workspace_edit.as_ref()?.document_changes).unwrap_or(false);
    let code_lens_refresh_support =
        workspace.and_then(|it| it.code_lens.as_ref()?.refresh_support).unwrap_or(false);
    let diagnostic_refresh_support =
        workspace.and_then(|it| it.diagnostic.as_ref()?.refresh_support).unwrap_or(false);
    let supports_document_diagnostics = text_document.is_some_and(|it| it.diagnostic.is_some());
    let diagnostic_delivery = if supports_document_diagnostics && diagnostic_refresh_support {
        DiagnosticDelivery::Pull
    } else {
        DiagnosticDelivery::Push
    };
    let inlay_hint_refresh_support =
        workspace.and_then(|it| it.inlay_hint.as_ref()?.refresh_support).unwrap_or(false);
    let code_action = text_document.and_then(|it| it.code_action.as_ref());
    let code_action_literals =
        code_action.is_some_and(|it| it.code_action_literal_support.is_some());
    let code_action_is_preferred =
        code_action.and_then(|it| it.is_preferred_support).unwrap_or(false);
    let publish_diagnostics = text_document.and_then(|it| it.publish_diagnostics.as_ref());
    let publish_diagnostics_related_information =
        publish_diagnostics.and_then(|it| it.related_information).unwrap_or(false);
    let publish_diagnostics_tags = publish_diagnostics
        .and_then(|it| Some(it.tag_support.as_ref()?.value_set.clone()))
        .unwrap_or_default();
    let publish_diagnostics_data =
        publish_diagnostics.and_then(|it| it.data_support).unwrap_or(false);
    let work_done_progress =
        capabilities.window.as_ref().and_then(|window| window.work_done_progress).unwrap_or(false);
    let hierarchical_document_symbol_support = text_document
        .and_then(|it| it.document_symbol.as_ref()?.hierarchical_document_symbol_support)
        .unwrap_or(false);
    let completion_item =
        text_document.and_then(|it| it.completion.as_ref()?.completion_item.as_ref());
    let completion = CompletionClientOptions {
        snippet_support: completion_item.and_then(|it| it.snippet_support).unwrap_or(false),
        markdown_documentation: prefers_markdown_documentation(
            completion_item.and_then(|it| it.documentation_format.as_deref()),
        ),
        resolve_documentation: completion_item
            .and_then(|it| it.resolve_support.as_ref())
            .is_none_or(|support| {
                support.properties.iter().any(|property| property == "documentation")
            }),
    };
    let signature_information =
        text_document.and_then(|it| it.signature_help.as_ref()?.signature_information.as_ref());
    let signature_help = SignatureHelpClientOptions {
        label_offsets: signature_information
            .and_then(|it| it.parameter_information.as_ref()?.label_offset_support)
            .unwrap_or(false),
        markdown_documentation: prefers_markdown_documentation(
            signature_information.and_then(|it| it.documentation_format.as_deref()),
        ),
        signature_active_parameter: signature_information
            .and_then(|it| it.active_parameter_support)
            .unwrap_or(false),
    };

    let workspace_roots =
        workspace_roots_from_initialize(workspace_folders, root_uri, || env::current_dir().ok());
    let file_operations = workspace_file_operation_options();

    (
        ServerCapabilities {
            completion_provider: Some(CompletionOptions {
                trigger_characters: Some([".", "/", "*", "\"", "'"].map(Into::into).into()),
                resolve_provider: Some(true),
                ..Default::default()
            }),
            declaration_provider: Some(DeclarationCapability::Simple(true)),
            definition_provider: Some(OneOf::Left(true)),
            implementation_provider: Some(ImplementationProviderCapability::Simple(true)),
            type_definition_provider: Some(TypeDefinitionProviderCapability::Simple(true)),
            document_formatting_provider: Some(OneOf::Left(true)),
            folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
            diagnostic_provider: (diagnostic_delivery == DiagnosticDelivery::Pull).then_some({
                DiagnosticServerCapabilities::Options(DiagnosticOptions {
                    identifier: None,
                    inter_file_dependencies: true,
                    workspace_diagnostics: true,
                    work_done_progress_options: WorkDoneProgressOptions {
                        work_done_progress: Some(true),
                    },
                })
            }),
            document_link_provider: Some(DocumentLinkOptions {
                resolve_provider: Some(false),
                work_done_progress_options: WorkDoneProgressOptions::default(),
            }),
            code_action_provider: code_action_literals.then(|| {
                CodeActionProviderCapability::Options(CodeActionOptions {
                    code_action_kinds: Some(vec![CodeActionKind::QUICKFIX]),
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                    resolve_provider: Some(false),
                })
            }),
            execute_command_provider: Some(ExecuteCommandOptions {
                commands: commands::ALL.into_iter().map(str::to_owned).collect(),
                work_done_progress_options: WorkDoneProgressOptions::default(),
            }),
            document_symbol_provider: Some(OneOf::Left(true)),
            code_lens_provider: Some(CodeLensServerOptions { resolve_provider: Some(false) }),
            document_highlight_provider: Some(OneOf::Left(true)),
            hover_provider: Some(HoverProviderCapability::Simple(true)),
            inlay_hint_provider: Some(OneOf::Left(true)),
            references_provider: Some(OneOf::Left(true)),
            call_hierarchy_provider: Some(CallHierarchyServerCapability::Simple(true)),
            selection_range_provider: Some(SelectionRangeProviderCapability::Simple(true)),
            rename_provider: Some(OneOf::Right(RenameOptions {
                prepare_provider: Some(true),
                work_done_progress_options: Default::default(),
            })),
            signature_help_provider: Some(SignatureHelpOptions {
                trigger_characters: Some(vec!["(".into(), ",".into()]),
                retrigger_characters: Some(vec![",".into()]),
                work_done_progress_options: WorkDoneProgressOptions::default(),
            }),
            text_document_sync: Some(TextDocumentSyncCapability::Options(
                TextDocumentSyncOptions {
                    open_close: Some(true),
                    change: Some(TextDocumentSyncKind::INCREMENTAL),
                    will_save: Some(true),
                    save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                        include_text: Some(false),
                    })),
                    ..Default::default()
                },
            )),
            workspace: Some(WorkspaceServerCapabilities {
                workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                    supported: Some(true),
                    change_notifications: Some(OneOf::Left(true)),
                }),
                file_operations: Some(WorkspaceFileOperationsServerCapabilities {
                    did_create: Some(file_operations.clone()),
                    will_create: Some(file_operations.clone()),
                    did_rename: Some(file_operations.clone()),
                    will_rename: Some(file_operations.clone()),
                    did_delete: Some(file_operations.clone()),
                    will_delete: Some(file_operations),
                }),
            }),
            workspace_symbol_provider: Some(OneOf::Left(true)),
            ..Default::default()
        },
        Config {
            workspace_roots,
            forge_path,
            selected_profile: launch_config.selected_profile().map(str::to_owned),
            foundry_workspace_config_source: launch_config
                .foundry_workspace_config_source()
                .clone(),
            index_policy,
            flycheck_options,
            watched_file_dynamic_registration,
            watched_file_relative_pattern_support,
            workspace_edit_document_changes,
            code_action_literals,
            code_action_is_preferred,
            diagnostic_delivery,
            publish_diagnostics_related_information,
            publish_diagnostics_tags,
            publish_diagnostics_data,
            pull_diagnostics_data,
            code_lens_refresh_support,
            diagnostic_refresh_support,
            inlay_hint_refresh_support,
            work_done_progress,
            hierarchical_document_symbol_support,
            completion,
            signature_help,
            code_lens,
            source_change_debounce,
            ..Default::default()
        },
    )
}

fn prefers_markdown_documentation(formats: Option<&[MarkupKind]>) -> bool {
    formats.is_some_and(|formats| {
        formats.iter().find(|format| matches!(format, MarkupKind::Markdown | MarkupKind::PlainText))
            == Some(&MarkupKind::Markdown)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{TestProject, workspace_at};
    use serde_json::{Value, json};
    use snapbox::{IntoData, assert_data_eq, str};

    /// Negotiates like the server, including the pull diagnostic data support read from JSON.
    fn negotiate(capabilities: Value) -> (ServerCapabilities, Config) {
        let params = serde_json::from_value::<proto::InitializeParams>(
            json!({ "capabilities": capabilities }),
        )
        .unwrap();
        let pull_data = params.pull_diagnostic_data_support();
        negotiate_capabilities_with_pull_diagnostic_data(
            params.into_inner(),
            pull_data,
            &LaunchConfig::default(),
        )
    }

    fn discover(project: &TestProject, roots: &[&str], options: Option<Value>) -> Config {
        let mut params = project.initialize_params_with_roots(roots);
        params.initialization_options = options;
        let (_, mut config) = negotiate_capabilities(params);
        config.rediscover_workspaces();
        config
    }

    fn relative(project: &TestProject, path: &Path) -> Option<String> {
        let path = path.strip_prefix(project.root()).ok()?;
        Some(format!("/{}", path.to_string_lossy().replace('\\', "/")))
    }

    /// Renders the watched-file specs inside the project, one per line.
    fn watched_specs(project: &TestProject, config: &Config) -> String {
        config
            .watched_file_specs()
            .iter()
            .filter_map(|spec| {
                let base = relative(project, &spec.base)?;
                Some(format!("{base} {} {:?}\n", spec.pattern, spec.kind))
            })
            .collect()
    }

    #[test]
    fn negotiate_capabilities_advertises_server_capabilities() {
        let (capabilities, _) = negotiate(json!({
            "textDocument": { "codeAction": { "codeActionLiteralSupport": {
                "codeActionKind": { "valueSet": [] }
            } }, "diagnostic": {} },
            "workspace": { "diagnostics": { "refreshSupport": true } },
        }));
        let mut capabilities = serde_json::to_value(capabilities).unwrap();
        let operations =
            capabilities["workspace"].as_object_mut().unwrap().remove("fileOperations").unwrap();
        let operations = operations.as_object().unwrap();
        let options = &operations["didCreate"];
        assert_eq!(operations.len(), 6);
        assert!(operations.values().all(|operation| operation == options));
        let lines = capabilities
            .as_object()
            .unwrap()
            .iter()
            .map(|(key, value)| format!("{key}: {value}\n"))
            .chain(
                options["filters"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|filter| format!("fileOperations: {filter}\n")),
            )
            .collect::<String>();
        assert_data_eq!(lines, str![[r#"
callHierarchyProvider: true
codeActionProvider: {"codeActionKinds":["quickfix"],"resolveProvider":false}
codeLensProvider: {"resolveProvider":false}
completionProvider: {"resolveProvider":true,"triggerCharacters":[".","/","*","\"","'"]}
declarationProvider: true
definitionProvider: true
diagnosticProvider: {"interFileDependencies":true,"workDoneProgress":true,"workspaceDiagnostics":true}
documentFormattingProvider: true
documentHighlightProvider: true
documentLinkProvider: {"resolveProvider":false}
documentSymbolProvider: true
executeCommandProvider: {"commands":["solar.clearCache","solar.reindex"]}
foldingRangeProvider: true
hoverProvider: true
implementationProvider: true
inlayHintProvider: true
referencesProvider: true
renameProvider: {"prepareProvider":true}
selectionRangeProvider: true
signatureHelpProvider: {"retriggerCharacters":[","],"triggerCharacters":["(",","]}
textDocumentSync: {"change":2,"openClose":true,"save":{"includeText":false},"willSave":true}
typeDefinitionProvider: true
workspace: {"workspaceFolders":{"changeNotifications":true,"supported":true}}
workspaceSymbolProvider: true
fileOperations: {"pattern":{"glob":"**/*.sol","matches":"file"},"scheme":"file"}
fileOperations: {"pattern":{"glob":"**/foundry.toml","matches":"file"},"scheme":"file"}
fileOperations: {"pattern":{"glob":"**/remappings.txt","matches":"file"},"scheme":"file"}
fileOperations: {"pattern":{"glob":"**","matches":"folder"},"scheme":"file"}

"#]]
        .raw());
    }

    #[test]
    fn negotiate_capabilities_records_client_support() {
        type Flag = fn(&ServerCapabilities, &Config) -> bool;
        let flags: [(&str, Flag); 23] = [
            ("push_diagnostics", |_, config| config.uses_push_diagnostics()),
            ("pull_diagnostics", |_, config| config.uses_pull_diagnostics()),
            ("diagnostic_provider", |capabilities, _| capabilities.diagnostic_provider.is_some()),
            ("publish_data", |_, config| config.supports_publish_diagnostics_data()),
            ("pull_data", |_, config| config.supports_pull_diagnostics_data()),
            ("code_action_data", |_, config| config.supports_code_action_diagnostic_data()),
            ("diagnostic_refresh", |_, config| config.supports_diagnostic_refresh()),
            ("inlay_hint_refresh", |_, config| config.supports_inlay_hint_refresh()),
            ("code_lens_refresh", |_, config| config.supports_code_lens_refresh()),
            ("watched_files", |_, config| config.supports_watched_file_dynamic_registration()),
            ("relative_patterns", |_, config| config.supports_watched_file_relative_patterns()),
            ("document_changes", |_, config| config.supports_workspace_edit_document_changes()),
            ("code_action_literals", |_, config| config.supports_code_action_literals()),
            ("code_action_provider", |capabilities, _| capabilities.code_action_provider.is_some()),
            ("code_action_is_preferred", |_, config| config.supports_code_action_is_preferred()),
            ("work_done_progress", |_, config| config.supports_work_done_progress()),
            ("hierarchical_symbols", |_, config| config.supports_hierarchical_document_symbols()),
            ("completion_snippets", |_, config| config.completion_options().snippet_support),
            ("completion_markdown", |_, config| config.completion_options().markdown_documentation),
            ("completion_resolve", |_, config| config.completion_options().resolve_documentation),
            ("signature_offsets", |_, config| config.signature_help_options().label_offsets),
            ("signature_markdown", |_, config| {
                config.signature_help_options().markdown_documentation
            }),
            ("signature_active_parameter", |_, config| {
                config.signature_help_options().signature_active_parameter
            }),
        ];
        let enabled = |capabilities: &Value| {
            let (server, config) = negotiate(capabilities.clone());
            flags.map(|(name, flag)| (name, flag(&server, &config)))
        };
        let default = enabled(&json!({}));
        let default_names = default.iter().filter(|flag| flag.1).map(|flag| flag.0);
        assert_eq!(default_names.collect::<Vec<_>>(), ["push_diagnostics", "completion_resolve"]);

        let completion_item =
            |item| json!({ "textDocument": { "completion": { "completionItem": item } } });
        let signature_information = |information| json!({ "textDocument": { "signatureHelp": { "signatureInformation": information } } });
        // Each row lists the flags that differ from the defaults.
        for (capabilities, changed) in [
            (json!({ "window": { "workDoneProgress": false } }), &[][..]),
            (json!({ "window": { "workDoneProgress": true } }), &["work_done_progress"]),
            (
                json!({ "workspace": { "didChangeWatchedFiles": {
                    "dynamicRegistration": true,
                    "relativePatternSupport": true,
                } } }),
                &["watched_files", "relative_patterns"],
            ),
            (
                json!({ "workspace": { "workspaceEdit": { "documentChanges": true } } }),
                &["document_changes"],
            ),
            (
                json!({ "workspace": { "codeLens": { "refreshSupport": true } } }),
                &["code_lens_refresh"],
            ),
            (
                json!({ "workspace": { "inlayHint": { "refreshSupport": true } } }),
                &["inlay_hint_refresh"],
            ),
            // Pull delivery requires both document diagnostics and refresh support.
            (
                json!({ "workspace": { "diagnostics": { "refreshSupport": true } } }),
                &["diagnostic_refresh"],
            ),
            (
                json!({ "textDocument": {
                    "diagnostic": { "dataSupport": true },
                    "publishDiagnostics": { "dataSupport": true },
                } }),
                &["publish_data", "code_action_data"],
            ),
            (
                json!({
                    "textDocument": {
                        "diagnostic": { "dataSupport": true },
                        "publishDiagnostics": { "dataSupport": true },
                    },
                    "workspace": { "diagnostics": { "refreshSupport": true } },
                }),
                &[
                    "push_diagnostics",
                    "pull_diagnostics",
                    "diagnostic_provider",
                    "pull_data",
                    "code_action_data",
                    "diagnostic_refresh",
                ],
            ),
            (
                json!({
                    "textDocument": {
                        "diagnostic": {},
                        "publishDiagnostics": { "dataSupport": true },
                    },
                    "workspace": { "diagnostics": { "refreshSupport": true } },
                }),
                &[
                    "push_diagnostics",
                    "pull_diagnostics",
                    "diagnostic_provider",
                    "diagnostic_refresh",
                ],
            ),
            // Any literal support enables quick fixes, even without the quick-fix kind.
            (
                json!({ "textDocument": { "codeAction": { "codeActionLiteralSupport": {
                    "codeActionKind": { "valueSet": ["refactor"] }
                } } } }),
                &["code_action_literals", "code_action_provider"],
            ),
            (
                json!({ "textDocument": { "codeAction": { "isPreferredSupport": true } } }),
                &["code_action_is_preferred"],
            ),
            (
                json!({ "textDocument": { "documentSymbol": {
                    "hierarchicalDocumentSymbolSupport": true
                } } }),
                &["hierarchical_symbols"],
            ),
            (
                completion_item(json!({
                    "snippetSupport": true,
                    "documentationFormat": ["markdown", "plaintext"],
                })),
                &["completion_snippets", "completion_markdown"],
            ),
            (completion_item(json!({ "documentationFormat": ["plaintext", "markdown"] })), &[]),
            (
                completion_item(
                    json!({ "resolveSupport": { "properties": ["additionalTextEdits"] } }),
                ),
                &["completion_resolve"],
            ),
            (
                completion_item(json!({ "resolveSupport": {
                    "properties": ["additionalTextEdits", "documentation"]
                } })),
                &[],
            ),
            (
                signature_information(json!({
                    "documentationFormat": ["markdown"],
                    "parameterInformation": { "labelOffsetSupport": true },
                    "activeParameterSupport": true,
                })),
                &["signature_offsets", "signature_markdown", "signature_active_parameter"],
            ),
            (
                signature_information(json!({ "documentationFormat": ["plaintext", "markdown"] })),
                &[],
            ),
        ] {
            let flags = enabled(&capabilities);
            let flipped = flags
                .iter()
                .zip(&default)
                .filter(|(flag, default)| flag.1 != default.1)
                .map(|(&(name, _), _)| name)
                .collect::<Vec<_>>();
            assert_eq!(flipped, changed, "{capabilities}");
        }
    }

    #[test]
    fn negotiate_capabilities_reads_initialization_options() {
        let config = |options, launch_config: &LaunchConfig| {
            let params = InitializeParams { initialization_options: options, ..Default::default() };
            negotiate_capabilities_with_pull_diagnostic_data(params, false, launch_config).1
        };
        let default_launch = LaunchConfig::default();

        assert_eq!(Config::default().source_change_debounce(), Duration::from_millis(150));
        for (options, expected_ms) in [
            (None, 150),
            (Some(json!({})), 150),
            (Some(json!({ "sourceChangeDebounce": 500 })), 500),
            (Some(json!({ "sourceChangeDebounce": 0 })), 0),
            (Some(json!({ "sourceChangeDebounce": -1 })), 150),
            (Some(json!({ "sourceChangeDebounce": 1.5 })), 150),
            (Some(json!({ "sourceChangeDebounce": "500" })), 150),
            (Some(json!({ "sourceChangeDebounce": null })), 150),
        ] {
            let debounce = config(options, &default_launch).source_change_debounce();
            assert_eq!(debounce, Duration::from_millis(expected_ms));
        }

        let embedded = LaunchConfig::default().with_default_forge_path("/embedded/forge");
        let forge = json!({ "forgePath": "/tools/forge" });
        assert_eq!(config(None, &default_launch).forge_path(), PathBuf::from("forge"));
        assert_eq!(config(None, &embedded).forge_path(), PathBuf::from("/embedded/forge"));
        assert_eq!(config(Some(forge), &embedded).forge_path(), PathBuf::from("/tools/forge"));

        let code_lens = json!({ "codeLens": {
            "enable": false,
            "selectors": false,
            "references": true,
            "inheritance": false,
            "clientCommands": true,
        } });
        assert_eq!(
            config(Some(code_lens), &default_launch).code_lens_options(),
            CodeLensConfig {
                enable: false,
                selectors: false,
                references: true,
                inheritance: false,
                client_commands: true,
            }
        );

        let indexing = json!({ "indexing": {
            "exclude": ["generated/**"],
            "useDefaultExcludes": false,
            "excludeHiddenDirectories": false,
            "excludeNestedRepositories": false,
        } });
        let config = config(Some(indexing), &default_launch);
        let root = env::temp_dir();
        for (directory, pruned) in
            [("generated", true), ("node_modules", false), (".hidden", false)]
        {
            let prunes =
                config.index_policy().should_prune_directory(&root, &root, &root.join(directory));
            assert_eq!(prunes, pruned, "{directory}");
        }
    }

    #[test]
    fn code_lens_activity_requires_an_enabled_lens_kind() {
        let inactive = CodeLensConfig {
            enable: true,
            selectors: false,
            references: false,
            inheritance: false,
            client_commands: false,
        };
        assert!(!inactive.is_active());
        assert!(!CodeLensConfig { enable: false, selectors: true, ..inactive }.is_active());
        assert!(!CodeLensConfig { client_commands: true, ..inactive }.is_active());

        for active in [
            CodeLensConfig { selectors: true, ..inactive },
            CodeLensConfig { references: true, ..inactive },
            CodeLensConfig { inheritance: true, ..inactive },
        ] {
            assert!(active.is_active());
        }
    }

    #[test]
    fn initialize_workspace_roots_prefer_folders_and_normalize_uris() {
        let workspace_root = env::temp_dir().join("solar-lsp-workspace");
        let uri = Url::from_file_path(&workspace_root).unwrap();
        let equivalent = Url::parse(&uri.as_str().replacen(
            "solar-lsp-workspace",
            "missing%2F..%2Fsolar-lsp-workspace",
            1,
        ))
        .unwrap();
        let folders = |uri| Some(vec![WorkspaceFolder { uri, name: "workspace".into() }]);
        let other_root = Url::from_file_path(env::temp_dir().join("other")).unwrap();

        for (folders, root_uri) in [
            (folders(uri), Some(other_root)),
            (folders(equivalent.clone()), None),
            (None, Some(equivalent)),
        ] {
            let roots = workspace_roots_from_initialize(folders, root_uri, || {
                panic!("valid workspace URI should not use the fallback")
            });
            assert_eq!(roots, std::slice::from_ref(&workspace_root));
        }
        assert!(workspace_roots_from_initialize(None, None, || None).is_empty());
    }

    #[test]
    fn flychecks_follow_configured_and_removed_workspaces() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [profile.default]
            src = "src"

            //- /src/Test.sol
            contract Test {}

            //- /lib/Dependency.sol
            contract Dependency {}
            "#,
        );
        let options = json!({ "flychecks": [{
            "id": "custom",
            "command": "custom-lint",
            "args": ["--json"],
            "output": "solc-json",
        }] });
        let (_, mut config) = negotiate_capabilities(InitializeParams {
            initialization_options: Some(options),
            ..project.initialize_params()
        });
        assert!(config.rediscover_workspaces().is_empty());

        let path = project.path("/src/Test.sol");
        let flychecks = config.flychecks_for_path(&path);
        let owners = config.flycheck_owners_for_path(&path).collect::<Vec<_>>();
        assert_eq!(flychecks.len(), 1);
        assert_eq!(owners, flychecks.iter().map(FlycheckConfig::owner).collect::<Vec<_>>());
        assert_eq!(flychecks[0].id, "custom");
        assert_eq!(flychecks[0].command, PathBuf::from("custom-lint"));
        assert_eq!(flychecks[0].args, ["--json"]);
        assert_eq!(flychecks[0].cwd, project.root());
        assert_eq!(config.flychecks_for_path(&project.path("/lib/Dependency.sol")).len(), 1);

        config.remove_workspace(project.root());
        assert_eq!(
            config.rediscover_workspaces(),
            [DiagnosticOwner::Flycheck {
                id: "custom".into(),
                workspace: project.root().to_path_buf(),
            }]
        );
    }

    #[test]
    fn source_file_updates_follow_external_foundry_flycheck_roots() {
        let project = TestProject::from_fixture(
            r#"
            //- /first/foundry.toml
            [profile.default]
            src = "src"

            //- /second/foundry.toml
            [profile.default]
            src = "src"
            test = "../checks"
            "#,
        );
        let mut config = project.config_with_roots(&["/"]);
        let path = project.path("/checks/New.t.sol");
        project.write_file("/checks/New.t.sol", "contract NewTest {}\n");
        let tracked = |config: &Config, root| {
            workspace_at(config, &project.path(root)).flycheck_source_files().contains(&path)
        };

        config.add_source_file(path.clone());
        assert!(tracked(&config, "/second"));
        assert!(!tracked(&config, "/first"));

        config.remove_source_file(&path);
        assert!(!tracked(&config, "/second"));
    }

    #[test]
    fn watched_file_specs_cover_flycheck_roots_and_unpruned_manifests() {
        let project = TestProject::from_fixture(
            r#"
            //- /project/foundry.toml
            [profile.default]
            src = "contracts"
            test = "checks"
            script = "automation"

            //- /project/contracts/Main.sol
            contract Main {}

            //- /project/checks/Main.t.sol
            contract Check {}

            //- /project/automation/Deploy.s.sol
            contract Deploy {}

            //- /project/packages/app/foundry.toml

            //- /project/node_modules/ignored/foundry.toml
            "#,
        );
        let config = project.config_with_roots(&["/project"]);

        assert_data_eq!(
            watched_specs(&project, &config),
            str![[r#"
/ foundry.toml Create | Change | Delete
/project * Create | Delete
/project *.sol Change
/project foundry.toml Create | Change | Delete
/project remappings.txt Create | Change | Delete
/project/automation **/*.sol Create | Change | Delete
/project/automation **/.git Create | Delete
/project/automation **/foundry.toml Create | Change | Delete
/project/checks **/*.sol Create | Change | Delete
/project/checks **/.git Create | Delete
/project/checks **/foundry.toml Create | Change | Delete
/project/contracts **/*.sol Create | Change | Delete
/project/contracts **/.git Create | Delete
/project/contracts **/foundry.toml Create | Change | Delete
/project/lib * Create | Delete
/project/packages * Create | Delete
/project/packages **/.git Create | Delete
/project/packages **/foundry.toml Create | Change | Delete
/project/packages *.sol Change
/project/packages foundry.toml Create | Change | Delete
/project/packages/app * Create | Delete
/project/packages/app **/.git Create | Delete
/project/packages/app **/foundry.toml Create | Change | Delete
/project/packages/app *.sol Change
/project/packages/app foundry.toml Create | Change | Delete
/project/packages/app remappings.txt Create | Change | Delete
/project/packages/app/lib * Create | Delete
/project/packages/app/script **/*.sol Create | Change | Delete
/project/packages/app/script **/.git Create | Delete
/project/packages/app/script **/foundry.toml Create | Change | Delete
/project/packages/app/src **/*.sol Create | Change | Delete
/project/packages/app/src **/.git Create | Delete
/project/packages/app/src **/foundry.toml Create | Change | Delete
/project/packages/app/test **/*.sol Create | Change | Delete
/project/packages/app/test **/.git Create | Delete
/project/packages/app/test **/foundry.toml Create | Change | Delete

"#]]
        );
    }

    #[test]
    fn watched_file_specs_cover_bounded_source_and_config_paths() {
        let project = TestProject::from_fixture(
            r#"
            //- /repo/foundry.toml
            [profile.default]
            src = "../external-src"
            libs = ["../external-lib"]
            remappings = ["@external/=../external-remapping/"]

            //- /repo/workspace/nested/foundry.toml
            "#,
        );
        let config = project.config_with_roots(&["/repo/workspace"]);
        let specs = config.watched_file_specs();

        assert!(project.root().ancestors().all(|ancestor| {
            specs.contains(&WatchedFileSpec::new(ancestor.to_path_buf(), "foundry.toml"))
        }));
        assert_data_eq!(
            watched_specs(&project, &config),
            str![[r#"
/ foundry.toml Create | Change | Delete
/repo * Create | Delete
/repo *.sol Change
/repo foundry.toml Create | Change | Delete
/repo remappings.txt Create | Change | Delete
/repo/script **/*.sol Create | Change | Delete
/repo/script **/.git Create | Delete
/repo/script **/foundry.toml Create | Change | Delete
/repo/test **/*.sol Create | Change | Delete
/repo/test **/.git Create | Delete
/repo/test **/foundry.toml Create | Change | Delete
/repo/workspace * Create | Delete
/repo/workspace **/.git Create | Delete
/repo/workspace **/foundry.toml Create | Change | Delete
/repo/workspace *.sol Change
/repo/workspace foundry.toml Create | Change | Delete
/repo/workspace remappings.txt Create | Change | Delete
/repo/workspace/nested * Create | Delete
/repo/workspace/nested **/.git Create | Delete
/repo/workspace/nested **/foundry.toml Create | Change | Delete
/repo/workspace/nested *.sol Change
/repo/workspace/nested foundry.toml Create | Change | Delete
/repo/workspace/nested remappings.txt Create | Change | Delete
/repo/workspace/nested/lib * Create | Delete
/repo/workspace/nested/script **/*.sol Create | Change | Delete
/repo/workspace/nested/script **/.git Create | Delete
/repo/workspace/nested/script **/foundry.toml Create | Change | Delete
/repo/workspace/nested/src **/*.sol Create | Change | Delete
/repo/workspace/nested/src **/.git Create | Delete
/repo/workspace/nested/src **/foundry.toml Create | Change | Delete
/repo/workspace/nested/test **/*.sol Create | Change | Delete
/repo/workspace/nested/test **/.git Create | Delete
/repo/workspace/nested/test **/foundry.toml Create | Change | Delete

"#]]
        );
    }

    #[test]
    fn external_foundry_roots_require_an_explicit_workspace_root() {
        let project = TestProject::from_fixture(
            r#"
            //- /workspace/foundry.toml
            [profile.default]
            src = "../shared/contracts"
            libs = ["../shared/contracts", "../shared/lib"]
            remappings = ["@shared/=../shared/contracts/"]

            //- /shared/contracts/Shared.sol
            contract Shared {}

            //- /shared/lib/pkg/src/Target.sol
            contract Target {}
            "#,
        );
        let config = project.config_with_roots(&["/workspace"]);
        assert!(config.workspaces().iter().all(|workspace| {
            workspace.source_roots() != [project.path("/shared/contracts")]
                && workspace
                    .source_files()
                    .iter()
                    .all(|path| !path.starts_with(project.path("/shared")))
        }));
        assert_data_eq!(
            watched_specs(&project, &config),
            str![[r#"
/ foundry.toml Create | Change | Delete
/workspace * Create | Delete
/workspace **/.git Create | Delete
/workspace **/foundry.toml Create | Change | Delete
/workspace *.sol Change
/workspace foundry.toml Create | Change | Delete
/workspace remappings.txt Create | Change | Delete
/workspace/script **/*.sol Create | Change | Delete
/workspace/script **/.git Create | Delete
/workspace/script **/foundry.toml Create | Change | Delete
/workspace/test **/*.sol Create | Change | Delete
/workspace/test **/.git Create | Delete
/workspace/test **/foundry.toml Create | Change | Delete

"#]]
        );

        let config = project.config_with_roots(&["/workspace", "/shared"]);
        assert_data_eq!(
            watched_specs(&project, &config),
            str![[r#"
/ foundry.toml Create | Change | Delete
/shared * Create | Delete
/shared *.sol Change
/shared foundry.toml Create | Change | Delete
/shared remappings.txt Create | Change | Delete
/shared/contracts * Create | Delete
/shared/contracts **/*.sol Create | Change | Delete
/shared/contracts **/.git Create | Delete
/shared/contracts **/foundry.toml Create | Change | Delete
/shared/lib * Create | Delete
/workspace * Create | Delete
/workspace **/.git Create | Delete
/workspace **/foundry.toml Create | Change | Delete
/workspace *.sol Change
/workspace foundry.toml Create | Change | Delete
/workspace remappings.txt Create | Change | Delete
/workspace/script **/*.sol Create | Change | Delete
/workspace/script **/.git Create | Delete
/workspace/script **/foundry.toml Create | Change | Delete
/workspace/test **/*.sol Create | Change | Delete
/workspace/test **/.git Create | Delete
/workspace/test **/foundry.toml Create | Change | Delete

"#]]
        );
        assert!(config.workspace_config_event_is_relevant(
            &project.path("/shared/contracts/nested/foundry.toml")
        ));
        assert!(config.shallow_watch_event_is_relevant(&project.path("/shared/lib/new-pkg")));
    }

    #[test]
    fn workspace_config_events_follow_watch_boundaries() {
        let nested_fixture = r#"
            //- /foundry.toml

            //- /packages/app/foundry.toml

            //- /packages/app/generated/.keep
        "#;
        let exclude = |glob| Some(json!({ "indexing": { "exclude": [glob] } }));
        for (fixture, roots, options, cases) in [
            (
                r#"
                //- /project/foundry.toml
                [profile.default]
                src = "src"

                //- /project/packages/app/src/.keep
                "#,
                &["/project/packages/app/src"][..],
                None,
                &[
                    ("/project/foundry.toml", true),
                    ("/project/remappings.txt", true),
                    ("/project/packages/app/foundry.toml", true),
                    ("/other/foundry.toml", false),
                ][..],
            ),
            // Custom globs are relative to the deepest workspace.
            (
                nested_fixture,
                &["/"],
                exclude("packages/app/generated/**"),
                &[("/packages/app/generated/foundry.toml", true)],
            ),
            (
                nested_fixture,
                &["/"],
                exclude("generated/**"),
                &[("/packages/app/generated/foundry.toml", false)],
            ),
            // Only the deepest workspace's import roots apply.
            (
                r#"
                //- /foundry.toml
                [profile.default]
                libs = ["packages/app/vendor"]

                //- /packages/app/foundry.toml

                //- /packages/app/vendor/.keep

                //- /packages/app/lib/.keep
                "#,
                &["/"],
                None,
                &[
                    ("/packages/app/vendor/foundry.toml", true),
                    ("/packages/app/lib/foundry.toml", false),
                ],
            ),
            // Import-only directories leading to a source root are not watched.
            (
                r#"
                //- /foundry.toml
                [profile.default]
                src = "lib/contracts"

                //- /lib/contracts/Main.sol
                contract Main {}
                "#,
                &["/"],
                None,
                &[("/lib/foundry.toml", false), ("/lib/contracts/nested/foundry.toml", true)],
            ),
        ] {
            let project = TestProject::from_fixture(fixture);
            let config = discover(&project, roots, options);
            for &(path, expected) in cases {
                let relevant = config.workspace_config_event_is_relevant(&project.path(path));
                assert_eq!(relevant, expected, "{path}");
            }
        }
    }

    #[test]
    fn formatter_root_uses_nearest_foundry_project_workspace_or_file_parent() {
        let project = TestProject::from_fixture(
            r#"
            //- /workspace/A.sol
            contract A {}

            //- /workspace/nested/B.sol
            contract B {}

            //- /outside/foundry.toml

            //- /outside/src/C.sol
            contract C {}

            //- /standalone/D.sol
            contract D {}
            "#,
        );
        let config = project.config_with_roots(&["/workspace", "/workspace/nested"]);

        for (path, root) in [
            ("/workspace/nested/B.sol", "/workspace/nested"),
            ("/workspace/A.sol", "/workspace"),
            ("/outside/src/C.sol", "/outside"),
            ("/standalone/D.sol", "/standalone"),
        ] {
            assert_eq!(
                config.formatter_root_for_path(&project.path(path)),
                Some(project.path(root))
            );
        }
    }

    #[test]
    fn rediscover_workspaces_loads_nested_discovered_project() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml

            //- /packages/token/foundry.toml
            [profile.default]
            src = "contracts"
            "#,
        );
        let config = project.config();

        assert_eq!(config.workspaces().len(), 2);
        assert!(
            config.workspaces().iter().all(|workspace| workspace.kind() == WorkspaceKind::Foundry)
        );
        assert_eq!(
            workspace_at(&config, &project.path("/packages/token")).source_roots(),
            ["", "/contracts", "/test", "/script"]
                .map(|dir| project.path(&format!("/packages/token{dir}")))
        );
    }

    #[test]
    fn rediscover_workspaces_falls_back_to_naked_roots() {
        let project = TestProject::from_fixture(
            r#"
            //- /broken/foundry.toml
            not valid toml =

            //- /configured/foundry.toml
            [profile.default]
            src = "contracts"

            //- /naked/.keep
            "#,
        );
        let mut config = project.config_with_roots(&["/broken", "/configured", "/naked"]);
        let kinds = |config: &Config| {
            config
                .workspaces()
                .iter()
                .map(|workspace| {
                    let root = workspace.compile_opts().base_path.as_deref().unwrap();
                    (relative(&project, root).unwrap(), workspace.kind())
                })
                .collect::<Vec<_>>()
        };

        assert_eq!(
            kinds(&config),
            [
                ("/broken".to_owned(), WorkspaceKind::Naked),
                ("/configured".to_owned(), WorkspaceKind::Foundry),
                ("/naked".to_owned(), WorkspaceKind::Naked),
            ]
        );
        assert_eq!(
            workspace_at(&config, &project.path("/configured")).source_roots(),
            ["", "/contracts", "/test", "/script"]
                .map(|dir| project.path(&format!("/configured{dir}")))
        );

        project.remove_file("/configured/foundry.toml");
        config.rediscover_workspaces();
        assert_eq!(
            kinds(&config),
            [
                ("/broken".to_owned(), WorkspaceKind::Naked),
                ("/configured".to_owned(), WorkspaceKind::Naked),
                ("/naked".to_owned(), WorkspaceKind::Naked),
            ]
        );
    }

    #[test]
    fn foundry_source_roots_apply_index_exclusions() {
        for (fixture, options, expected) in [
            (
                r#"
                //- /foundry.toml
                [profile.default]
                src = "src"

                //- /src/Main.sol
                contract Main {}

                //- /src/lib/Library.sol
                library Library {}

                //- /src/out/Generated.sol
                contract Generated {}

                //- /src/.hidden/Hidden.sol
                contract Hidden {}

                //- /src/vendor/.git
                gitdir: elsewhere

                //- /src/vendor/Nested.sol
                contract Nested {}

                //- /src/generated/Custom.sol
                contract Custom {}
                "#,
                Some(json!({ "indexing": { "exclude": ["src/generated/**"] } })),
                "/src/Main.sol",
            ),
            (
                r#"
                //- /foundry.toml
                [profile.default]
                src = "."

                //- /Main.sol
                contract Main {}

                //- /out/Generated.sol
                contract Generated {}
                "#,
                None,
                "/Main.sol",
            ),
        ] {
            let project = TestProject::from_fixture(fixture);
            let config = discover(&project, &["/"], options);
            assert_eq!(config.workspaces()[0].source_files(), [project.path(expected)]);
        }
    }

    #[test]
    fn rediscovery_reconciles_overlapping_workspace_source_caches() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [profile.default]
            src = "."

            //- /Outer.sol
            contract Outer {}

            //- /nested/foundry.toml
            [profile.default]
            src = "."

            //- /nested/Inner.sol
            contract Inner {}

            //- /nested/generated/Excluded.sol
            contract Excluded {}
            "#,
        );
        let config = discover(
            &project,
            &["/"],
            Some(json!({
                "indexing": { "exclude": ["generated/**"] }
            })),
        );

        assert_eq!(
            workspace_at(&config, &project.path("/")).source_files(),
            [project.path("/Outer.sol")]
        );
        assert_eq!(
            workspace_at(&config, &project.path("/nested")).source_files(),
            [project.path("/nested/Inner.sol")]
        );
        assert_eq!(config.index_metrics().eager, 2);
    }
}
