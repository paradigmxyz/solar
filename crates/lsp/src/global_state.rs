use crate::{
    NotifyResult,
    config::{
        Config, WatchedFileSpec, WorkspaceDiscoveryResult,
        negotiate_capabilities_with_pull_diagnostic_data,
    },
    diagnostics::{
        AnalyzedDocuments, DiagnosticMap, DiagnosticOwner, DiagnosticStore, PullReport,
        WorkspacePullReport,
    },
    file_operations::FileOperationCoordinator,
    flycheck,
    progress::{ProgressCoordinator, ProgressTicket},
    proto,
    protocol_trace::ProtocolTrace,
    symbols::{SymbolTables, SymbolTablesAggregator},
    vfs::Vfs,
    workspace::{WorkspaceError, WorkspacePathIndex, index_policy::IndexingCancellation},
};
use async_lsp::{ClientSocket, LanguageClient, ResponseError};
use lsp_types::{
    Diagnostic, DidChangeWatchedFilesRegistrationOptions, FileChangeType, FileSystemWatcher,
    GlobPattern, InitializeParams, InitializedParams, LogMessageParams, MessageType, OneOf,
    PreviousResultId, PublishDiagnosticsParams, Registration, RegistrationParams, RelativePattern,
    SetTraceParams, Unregistration, UnregistrationParams, Url, WatchKind,
    WorkDoneProgressCancelParams,
    notification::{DidChangeWatchedFiles, Notification},
};
use normalize_path::NormalizePath;
use solar_config::CompileOpts;
use solar_interface::{
    Session,
    data_structures::{
        map::{FxHashMap, FxHashSet},
        sync::{Mutex, RwLock},
    },
    diagnostics::{DiagCtxt, InMemoryEmitter},
    source_map::{FileLoader, FileName, RealFileLoader, SourceMap},
};
use solar_sema::Compiler;
use std::{
    borrow::Cow,
    io, mem,
    ops::ControlFlow,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    sync::{Mutex as AsyncMutex, Semaphore, oneshot, watch},
    task::{AbortHandle, JoinHandle},
};

#[derive(Clone, Copy)]
enum AnalysisMode {
    Recompute,
    Rediscover,
    IfInvalidated,
}

#[derive(Clone, Copy)]
enum AnalysisTrigger {
    Document,
    External,
}

#[derive(Default)]
struct AnalysisRequest {
    disk_paths: Vec<PathBuf>,
    removed_paths: Vec<PathBuf>,
    retained_paths: Vec<PathBuf>,
    changed_paths: Vec<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SourceFileEventDisposition {
    Relevant,
    Deferred,
    Recover,
    Irrelevant,
}

enum AnalysisTaskOutcome {
    Published,
    Superseded,
}

#[derive(Clone, Default)]
struct AnalysisPathIndex {
    resolved_dependencies: FxHashSet<PathBuf>,
    existing_unresolved_candidates: FxHashSet<PathBuf>,
    missing_candidates: FxHashSet<PathBuf>,
}

impl AnalysisPathIndex {
    fn merge(&mut self, other: Self) {
        self.resolved_dependencies.extend(other.resolved_dependencies);
        self.existing_unresolved_candidates.extend(other.existing_unresolved_candidates);
        self.missing_candidates.extend(other.missing_candidates);
    }

    fn includes(&self, path: &Path, include_missing: bool) -> bool {
        self.resolved_dependencies.contains(path)
            || self.existing_unresolved_candidates.contains(path)
            || include_missing && self.missing_candidates.contains(path)
    }
}

#[derive(Default)]
struct ImportPathProbes {
    existing: FxHashSet<PathBuf>,
    missing: FxHashSet<PathBuf>,
}

#[derive(Clone, Default)]
struct ImportPathTracker(Arc<Mutex<ImportPathProbes>>);

impl ImportPathTracker {
    fn take_probes(&self) -> ImportPathProbes {
        mem::take(&mut *self.0.lock())
    }

    fn clear(&self) {
        let mut probes = self.0.lock();
        probes.existing.clear();
        probes.missing.clear();
    }
}

struct TrackingFileLoader {
    tracker: ImportPathTracker,
}

impl FileLoader for TrackingFileLoader {
    fn canonicalize_path(&self, path: &Path) -> io::Result<PathBuf> {
        let result = RealFileLoader.canonicalize_path(path);
        let mut probes = self.tracker.0.lock();
        if result.as_ref().is_err_and(|error| error.kind() == io::ErrorKind::NotFound) {
            probes.missing.insert(path.to_path_buf());
        } else {
            probes.existing.insert(path.to_path_buf());
        }
        result
    }

    fn load_stdin(&self) -> io::Result<String> {
        RealFileLoader.load_stdin()
    }

    fn load_file(&self, path: &Path) -> io::Result<String> {
        RealFileLoader.load_file(path)
    }

    fn load_binary_file(&self, path: &Path) -> io::Result<Vec<u8>> {
        RealFileLoader.load_binary_file(path)
    }
}

pub(crate) struct WorkspaceDiscoveryReady {
    version: usize,
    result: WorkspaceDiscoveryResult,
    disk_paths: Vec<PathBuf>,
    progress: ProgressTicket,
    cancellation: IndexingCancellation,
}

pub(crate) struct WorkspaceDiscoveryFailed {
    version: usize,
    error: String,
    progress: ProgressTicket,
}

pub(crate) struct DeferredSourceFileEventsReady {
    version: usize,
    events: Vec<(PathBuf, FileChangeType)>,
}

pub(crate) struct WatchedFileRegistrationReady;

struct WorkspaceDiscoveryMonitor {
    version: usize,
    disk_paths: Vec<PathBuf>,
    progress: ProgressTicket,
    cancellation: IndexingCancellation,
    analysis_version: Arc<AtomicUsize>,
    client: ClientSocket,
}

impl WorkspaceDiscoveryMonitor {
    async fn finish(
        self,
        worker: JoinHandle<Result<Option<WorkspaceDiscoveryResult>, WorkspaceError>>,
    ) {
        match worker.await {
            Ok(Ok(Some(result)))
                if !self.cancellation.is_cancelled()
                    && self.analysis_version.load(Ordering::Acquire) == self.version =>
            {
                let _ = self.client.emit(WorkspaceDiscoveryReady {
                    version: self.version,
                    result,
                    disk_paths: self.disk_paths,
                    progress: self.progress,
                    cancellation: self.cancellation,
                });
            }
            Ok(Err(error)) if !self.cancellation.is_cancelled() => {
                let _ = self.client.emit(WorkspaceDiscoveryFailed {
                    version: self.version,
                    error: error.to_string(),
                    progress: self.progress,
                });
            }
            Ok(_) => {}
            Err(error) => {
                let _ = self.client.emit(WorkspaceDiscoveryFailed {
                    version: self.version,
                    error: error.to_string(),
                    progress: self.progress,
                });
            }
        }
    }
}

#[derive(Clone, Copy, Default)]
struct RefreshRequests {
    diagnostics: bool,
    inlay_hints: bool,
}

#[derive(Default)]
struct PendingExternalRefresh {
    diagnostics_changed: bool,
}

/// State serialized with analysis and diagnostic publication.
#[derive(Default)]
struct AnalysisCommitState {
    cache_invalidated: bool,
    discovery_pending: bool,
    workspace_roots_before_change: Option<Vec<PathBuf>>,
    analysis_paths: AnalysisPathIndex,
    deferred_source_file_events: FxHashMap<PathBuf, FileChangeType>,
    external_refresh: Option<PendingExternalRefresh>,
    /// VFS content revision captured when the current analysis epoch began.
    vfs_content_revision: u64,
    /// Last version that actually replaced the symbol tables.
    symbol_tables_version: usize,
    /// Config used to produce the currently published symbol tables.
    ///
    /// Workspace discovery replaces `GlobalState::config` before scheduling the analysis. Keeping
    /// the config with the published epoch prevents requests that wait for that analysis from
    /// validating its results against the previous discovery snapshot.
    analysis_config: Option<Arc<Config>>,
    natspec_pending_source_changes: FxHashSet<PathBuf>,
    natspec_context_change_version: usize,
    cached_output: Option<CachedAnalysisOutput>,
}

#[derive(Clone)]
struct CachedAnalysisOutput {
    vfs_content_revision: u64,
    config: Arc<Config>,
    output: AnalysisOutput,
}

impl AnalysisCommitState {
    fn begin_external_refresh(&mut self) {
        self.external_refresh.get_or_insert_default();
    }

    fn record_external_diagnostics_change(&mut self, changed: bool) {
        if changed && let Some(refresh) = &mut self.external_refresh {
            refresh.diagnostics_changed = true;
        }
    }

    fn fail_external_refresh(&mut self) -> RefreshRequests {
        let diagnostics = self
            .external_refresh
            .as_mut()
            .is_some_and(|refresh| mem::take(&mut refresh.diagnostics_changed));
        RefreshRequests { diagnostics, inlay_hints: false }
    }

    fn finish_external_refresh(
        &mut self,
        diagnostics_changed: bool,
        inlay_hints_changed: bool,
    ) -> RefreshRequests {
        let Some(refresh) = self.external_refresh.take() else {
            return RefreshRequests::default();
        };
        RefreshRequests {
            diagnostics: refresh.diagnostics_changed || diagnostics_changed,
            inlay_hints: inlay_hints_changed,
        }
    }
}

/// Serializes compiler analysis while allowing the newest request to replace a pending one.
///
/// Tokio cannot abort a blocking worker after it starts, so its permit stays in the worker until
/// it exits at one of the version checks. Only the newest coordinator waits for that permit.
struct AnalysisScheduler {
    gate: Arc<Semaphore>,
    tasks: Mutex<AnalysisTasks>,
}

#[derive(Default)]
struct WatchedFileRegistrationCoordinator {
    gate: AsyncMutex<()>,
    generation: AtomicUsize,
    desired_specs: Mutex<Option<Vec<WatchedFileSpec>>>,
    active_registration_ids: Mutex<Vec<String>>,
}

struct WatchedFileRegistrationUpdate {
    generation: usize,
    desired_specs: Vec<WatchedFileSpec>,
    registration_id: String,
    registration: RegistrationParams,
}

impl Default for AnalysisScheduler {
    fn default() -> Self {
        Self { gate: Arc::new(Semaphore::new(1)), tasks: Mutex::new(AnalysisTasks::default()) }
    }
}

#[derive(Default)]
struct AnalysisTasks {
    coordinator: Option<(AnalysisTaskKey, AbortHandle)>,
    worker: Option<(AnalysisTaskKey, AbortHandle)>,
    cancellation: Option<IndexingCancellation>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct AnalysisTaskKey {
    version: usize,
    stage: AnalysisTaskStage,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AnalysisTaskStage {
    Discovery,
    Analysis,
}

impl AnalysisTasks {
    fn cancel(&mut self) {
        if let Some(cancellation) = self.cancellation.take() {
            cancellation.cancel();
        }
        if let Some((_, worker)) = self.worker.take() {
            worker.abort();
        }
        if let Some((_, coordinator)) = self.coordinator.take() {
            coordinator.abort();
        }
    }

    fn clear_worker(&mut self, key: AnalysisTaskKey) {
        if self.worker.as_ref().is_some_and(|(task_key, _)| *task_key == key) {
            self.worker = None;
        }
    }

    fn clear_coordinator(&mut self, key: AnalysisTaskKey) {
        if self.coordinator.as_ref().is_some_and(|(task_key, _)| *task_key == key) {
            self.coordinator = None;
        }
    }
}

pub(crate) struct GlobalState {
    client: ClientSocket,
    pub(crate) sess: Session,
    pub(crate) vfs: Arc<RwLock<Vfs>>,
    pub(crate) config: Arc<Config>,
    launch_config: crate::LaunchConfig,
    pub(crate) file_operations: FileOperationCoordinator,
    analysis_version: Arc<AtomicUsize>,
    published_analysis_version: watch::Sender<usize>,
    analysis_commit: Arc<Mutex<AnalysisCommitState>>,
    analysis_progress: ProgressCoordinator,
    analysis_scheduler: Arc<AnalysisScheduler>,
    watched_file_registration: Arc<WatchedFileRegistrationCoordinator>,
    background_discovery: bool,
    protocol_trace: ProtocolTrace,
    flycheck_versions: Arc<RwLock<FxHashMap<DiagnosticOwner, usize>>>,
    flycheck_cancels: FxHashMap<DiagnosticOwner, oneshot::Sender<()>>,
    pub(crate) symbol_tables: Arc<RwLock<SymbolTables>>,
    diagnostics: Arc<RwLock<DiagnosticStore>>,
}

pub(crate) struct AnalysisRevision {
    version: usize,
    current: Arc<AtomicUsize>,
    commit: Arc<Mutex<AnalysisCommitState>>,
    vfs: Arc<RwLock<Vfs>>,
}

impl AnalysisRevision {
    pub(crate) fn is_current(&self, vfs_content_revision: u64) -> bool {
        let commit = self.commit.lock();
        self.current.load(Ordering::Acquire) == self.version
            && commit.symbol_tables_version == self.version
            && commit.vfs_content_revision == vfs_content_revision
            && self.vfs.read().content_revision() == vfs_content_revision
    }
}

impl GlobalState {
    pub(crate) fn new(client: ClientSocket) -> Self {
        let (published_analysis_version, _) = watch::channel(0);
        let config = Arc::new(Config::default());
        let protocol_trace = ProtocolTrace::new(client.clone());
        let analysis_progress = ProgressCoordinator::with_timing(
            client.clone(),
            false,
            config.progress_delay(),
            config.progress_create_timeout(),
        );
        Self {
            client,
            sess: Session::default(),
            vfs: Arc::new(Default::default()),
            file_operations: FileOperationCoordinator::default(),
            analysis_version: Arc::new(AtomicUsize::new(0)),
            published_analysis_version,
            analysis_commit: Arc::new(Default::default()),
            analysis_progress,
            analysis_scheduler: Arc::new(Default::default()),
            watched_file_registration: Arc::new(Default::default()),
            background_discovery: false,
            protocol_trace,
            flycheck_versions: Arc::new(Default::default()),
            flycheck_cancels: FxHashMap::default(),
            symbol_tables: Arc::new(Default::default()),
            diagnostics: Arc::new(Default::default()),
            config,
            launch_config: crate::LaunchConfig::default(),
        }
    }

    pub(crate) fn with_launch_config(mut self, launch_config: crate::LaunchConfig) -> Self {
        self.launch_config = launch_config;
        self
    }

    pub(crate) fn enable_background_discovery(&mut self) {
        self.background_discovery = true;
    }

    pub(crate) fn client_socket(&self) -> ClientSocket {
        self.client.clone()
    }

    pub(crate) fn analysis_revision(&self) -> AnalysisRevision {
        AnalysisRevision {
            version: self.analysis_version.load(Ordering::Acquire),
            current: self.analysis_version.clone(),
            commit: self.analysis_commit.clone(),
            vfs: self.vfs.clone(),
        }
    }

    pub(crate) fn update_analyzed_document_version(&self, uri: Url, version: i32) {
        let commit = self.analysis_commit.lock();
        let requested_version = self.analysis_version.load(Ordering::Acquire);
        let published_version = *self.published_analysis_version.borrow();
        // Version-only edits may relabel only a fully published, valid snapshot.
        let can_update = !commit.cache_invalidated && published_version == requested_version;
        drop(commit);
        if !can_update {
            return;
        }
        self.diagnostics.write().update_analyzed_document_version(uri, i64::from(version));
    }

    pub(crate) fn source_file_event_is_relevant(&self, path: &Path, include_missing: bool) -> bool {
        if self.config.tracks_watched_source_file(path)
            || self.vfs.read().exists(&crate::vfs::VfsPath::from(path.to_path_buf()))
        {
            return true;
        }
        let commit = self.analysis_commit.lock();
        commit.natspec_pending_source_changes.contains(path)
            || commit.analysis_paths.includes(path, include_missing)
    }

    pub(crate) fn classify_source_file_event(
        &self,
        path: &Path,
        typ: FileChangeType,
    ) -> SourceFileEventDisposition {
        if self.config.tracks_watched_source_file(path)
            || self.vfs.read().exists(&crate::vfs::VfsPath::from(path.to_path_buf()))
        {
            return SourceFileEventDisposition::Relevant;
        }

        let mut commit = self.analysis_commit.lock();
        let include_missing = typ == FileChangeType::CREATED;
        if commit.natspec_pending_source_changes.contains(path)
            || commit.analysis_paths.includes(path, include_missing)
        {
            return SourceFileEventDisposition::Relevant;
        }

        let requested = self.analysis_version.load(Ordering::Acquire);
        let published = *self.published_analysis_version.borrow();
        if published < requested {
            commit.deferred_source_file_events.insert(path.to_path_buf(), typ);
            SourceFileEventDisposition::Deferred
        } else if commit.cache_invalidated {
            SourceFileEventDisposition::Recover
        } else {
            SourceFileEventDisposition::Irrelevant
        }
    }

    fn reconcile_deferred_source_file(&mut self, path: &Path, typ: FileChangeType) -> bool {
        let present = match std::fs::symlink_metadata(path) {
            Ok(metadata) => Some(metadata.is_file()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Some(false),
            Err(_) => None,
        };
        match present {
            Some(true) => Arc::make_mut(&mut self.config).add_source_file(path.to_path_buf()),
            Some(false) => Arc::make_mut(&mut self.config).remove_source_file(path),
            None if typ == FileChangeType::CREATED => {
                Arc::make_mut(&mut self.config).add_source_file(path.to_path_buf());
            }
            None if typ == FileChangeType::DELETED => {
                Arc::make_mut(&mut self.config).remove_source_file(path);
            }
            None => {}
        }
        present == Some(false) || present.is_none() && typ == FileChangeType::DELETED
    }

    pub(crate) fn created_file_operation_path_is_relevant(&self, path: &Path) -> bool {
        self.file_operation_path_is_relevant(path, true)
    }

    pub(crate) fn deleted_file_operation_path_is_relevant(&self, path: &Path) -> bool {
        self.file_operation_path_is_relevant(path, false)
    }

    fn file_operation_path_is_relevant(&self, path: &Path, include_missing: bool) -> bool {
        if self.config.workspace_config_event_is_relevant(path)
            || self.source_file_event_is_relevant(path, include_missing)
        {
            return true;
        }

        if !self.symbol_tables.read().file_operation_paths_under(&[path.to_path_buf()]).is_empty() {
            return true;
        }

        if !self.config.file_operation_paths_under(&[path.to_path_buf()]).is_empty() {
            return true;
        }

        if self.config.workspace_roots().iter().any(|root| root.starts_with(path)) {
            return true;
        }

        let commit = self.analysis_commit.lock();
        let known_dependency_under = commit
            .analysis_paths
            .resolved_dependencies
            .iter()
            .chain(commit.analysis_paths.existing_unresolved_candidates.iter())
            .any(|candidate| candidate.starts_with(path));
        let missing_candidate_under = commit
            .analysis_paths
            .missing_candidates
            .iter()
            .any(|candidate| candidate.starts_with(path));
        if known_dependency_under || missing_candidate_under {
            return true;
        }
        let conservatively_admit = commit.discovery_pending
            && self.config.workspaces().is_empty()
            && self
                .config
                .workspace_roots()
                .iter()
                .any(|root| path.starts_with(root) || root.starts_with(path));
        drop(commit);
        if conservatively_admit {
            return true;
        }

        for workspace in self.config.workspaces() {
            for source_root in workspace.source_roots() {
                if source_root.starts_with(path) {
                    return true;
                }
                if path.starts_with(source_root)
                    && !workspace.is_import_only_path(path)
                    && !workspace.excludes_source_directory(
                        self.config.index_policy(),
                        source_root,
                        path,
                    )
                {
                    return include_missing;
                }
            }
        }
        false
    }

    #[cfg(test)]
    pub(crate) fn on_initialize(
        &mut self,
        params: InitializeParams,
    ) -> impl Future<Output = Result<proto::InitializeResponse, ResponseError>> + use<> {
        self.on_initialize_with_pull_diagnostic_data(params, false)
    }

    pub(crate) fn on_initialize_with_pull_diagnostic_data(
        &mut self,
        params: InitializeParams,
        pull_diagnostic_data_support: bool,
    ) -> impl Future<Output = Result<proto::InitializeResponse, ResponseError>> + use<> {
        self.protocol_trace.set_level(params.trace.unwrap_or_default());
        let (capabilities, config) = negotiate_capabilities_with_pull_diagnostic_data(
            params,
            pull_diagnostic_data_support,
            &self.launch_config,
        );

        self.analysis_progress.set_enabled(config.supports_work_done_progress());
        self.config = Arc::new(config);
        std::future::ready(Ok(proto::InitializeResponse::new(capabilities)))
    }

    pub(crate) fn on_set_trace(&mut self, params: SetTraceParams) -> NotifyResult {
        self.protocol_trace.update_level(params.value);
        ControlFlow::Continue(())
    }

    pub(crate) fn protocol_trace(&self) -> ProtocolTrace {
        self.protocol_trace.clone()
    }

    pub(crate) fn on_initialized(&mut self, _: InitializedParams) -> NotifyResult {
        self.update_watched_file_registration();

        self.reindex();

        let _ = self.client.log_message(LogMessageParams {
            typ: MessageType::INFO,
            message: "solar initialized".into(),
        });
        ControlFlow::Continue(())
    }

    pub(crate) fn reregister_watched_files(&self) {
        self.update_watched_file_registration();
    }

    fn update_watched_file_registration(&self) {
        if !self.config.supports_watched_file_dynamic_registration() {
            return;
        }
        let analysis_paths = {
            let commit = self.analysis_commit.lock();
            AnalysisPathIndex {
                resolved_dependencies: commit.analysis_paths.resolved_dependencies.clone(),
                existing_unresolved_candidates: commit
                    .analysis_paths
                    .existing_unresolved_candidates
                    .clone(),
                missing_candidates: commit.analysis_paths.missing_candidates.clone(),
            }
        };
        let specs = watched_file_specs(&self.config, &analysis_paths);
        let update = prepare_watched_file_registration_update(
            &self.config,
            &self.watched_file_registration,
            specs,
        );
        spawn_watched_file_registration_update(
            &self.client,
            &self.watched_file_registration,
            update,
        );
    }

    pub(crate) fn on_work_done_progress_cancel(
        &mut self,
        params: WorkDoneProgressCancelParams,
    ) -> NotifyResult {
        self.analysis_progress.cancel(&params.token);
        ControlFlow::Continue(())
    }

    /// Parses, lowers, and performs analysis on project files, including in-memory only files.
    ///
    /// Each time analysis is triggered, a version is assigned to the analysis. A snapshot is then
    /// taken of the global state ([`GlobalStateSnapshot`]) and analysis is performed on
    /// the entire project in a separate thread.
    ///
    /// Currently, Solar is sufficiently fast at parsing and lowering even large Solidity projects,
    /// so while analysing the entire project is relatively expensive compared to incremental
    /// analysis, it is still fast enough for most workloads. A potential improvement would be to
    /// enable incremental parsing and analysis in Solar using e.g. [`salsa`].
    ///
    /// [`salsa`]: https://docs.rs/salsa/latest/salsa/
    pub(crate) fn recompute_with_disk_files(&mut self, disk_paths: Vec<PathBuf>) {
        let changed_paths = disk_paths.clone();
        let delay = self.config.source_change_debounce();
        self.request_analysis(
            AnalysisMode::Recompute,
            AnalysisRequest { disk_paths, changed_paths, ..Default::default() },
            AnalysisTrigger::Document,
            delay,
        );
    }

    pub(crate) fn recompute_after_opening_source(&mut self, changed_paths: Vec<PathBuf>) {
        self.request_analysis(
            AnalysisMode::Recompute,
            AnalysisRequest { changed_paths, ..Default::default() },
            AnalysisTrigger::Document,
            Duration::ZERO,
        );
    }

    pub(crate) fn recompute_after_source_changes(&mut self, changed_paths: Vec<PathBuf>) {
        let delay = self.config.source_change_debounce();
        self.request_analysis(
            AnalysisMode::Recompute,
            AnalysisRequest { changed_paths, ..Default::default() },
            AnalysisTrigger::Document,
            delay,
        );
    }

    pub(crate) fn recompute_for_file_changes(
        &mut self,
        disk_paths: Vec<PathBuf>,
        removed_paths: Vec<PathBuf>,
        force_rediscover: bool,
    ) {
        let changed_paths = disk_paths.clone();
        let mode =
            if force_rediscover { AnalysisMode::Rediscover } else { AnalysisMode::Recompute };
        let delay = self.config.source_change_debounce();
        self.request_analysis(
            mode,
            AnalysisRequest { disk_paths, removed_paths, changed_paths, ..Default::default() },
            AnalysisTrigger::External,
            delay,
        );
    }

    pub(crate) fn reindex(&mut self) {
        self.request_analysis(
            AnalysisMode::Rediscover,
            AnalysisRequest::default(),
            AnalysisTrigger::External,
            Duration::ZERO,
        );
    }

    pub(crate) fn reindex_after_removing_paths(&mut self, removed_paths: Vec<PathBuf>) {
        let retained_paths = self.config.workspace_roots().to_vec();
        self.request_analysis(
            AnalysisMode::Rediscover,
            AnalysisRequest { removed_paths, retained_paths, ..Default::default() },
            AnalysisTrigger::External,
            Duration::ZERO,
        );
    }

    pub(crate) fn record_workspace_root_change(&mut self, previous_roots: Vec<PathBuf>) {
        self.analysis_commit.lock().workspace_roots_before_change.get_or_insert(previous_roots);
    }

    pub(crate) fn reindex_if_invalidated(&mut self) {
        self.request_analysis(
            AnalysisMode::IfInvalidated,
            AnalysisRequest::default(),
            AnalysisTrigger::Document,
            Duration::ZERO,
        );
    }

    pub(crate) fn clear_analysis_cache(&mut self) {
        let refresh_code_lenses =
            self.config.supports_code_lens_refresh() && self.config.code_lens_options().is_active();
        let compare_inlay_hints = self.config.supports_inlay_hint_refresh();
        let config = self.config.clone();
        let (old_symbol_tables, refresh_requests) = {
            let Self {
                client,
                symbol_tables,
                diagnostics,
                analysis_version,
                published_analysis_version,
                analysis_commit,
                analysis_progress,
                ..
            } = self;
            let mut commit = analysis_commit.lock();
            let version = analysis_version.load(Ordering::Relaxed).wrapping_add(1);

            analysis_progress.finish_active_after("Workspace index cleared", || {
                // Invalidate workers before doing the potentially expensive diagnostic publication.
                analysis_version.store(version, Ordering::Release);
                let mut symbol_tables = symbol_tables.write();
                let inlay_hints_changed = compare_inlay_hints
                    && symbol_tables.inlay_hints_changed(&SymbolTables::default());
                let old_symbol_tables = mem::take(&mut *symbol_tables);
                drop(symbol_tables);
                let update = diagnostics.write().replace_compiler_snapshot_and_publish_batches(
                    DiagnosticMap::default(),
                    AnalyzedDocuments::default(),
                );
                let pull_results_changed =
                    update.pull_reports_changed || update.workspace_documents_changed;
                let external_refresh = commit
                    .finish_external_refresh(update.pull_reports_changed, inlay_hints_changed);
                let refresh_requests = RefreshRequests {
                    diagnostics: external_refresh.diagnostics || pull_results_changed,
                    inlay_hints: external_refresh.inlay_hints || inlay_hints_changed,
                };
                publish_diagnostic_batches(client, update.batches, &config);

                commit.cache_invalidated = true;
                commit.cached_output = None;
                commit.discovery_pending = false;
                commit.workspace_roots_before_change = None;
                commit.analysis_paths = AnalysisPathIndex::default();
                commit.deferred_source_file_events.clear();
                commit.symbol_tables_version = version;
                commit.analysis_config = Some(config.clone());
                commit.natspec_pending_source_changes.clear();
                commit.natspec_context_change_version = version;
                published_analysis_version.send_replace(version);
                (old_symbol_tables, refresh_requests)
            })
        };
        self.analysis_scheduler.tasks.lock().cancel();
        self.reregister_watched_files();
        drop(old_symbol_tables);
        request_pull_result_refreshes(&self.client, &self.config, refresh_requests);
        if refresh_code_lenses {
            request_code_lens_refresh(&self.client);
        }
    }

    #[cfg(test)]
    pub(crate) fn analysis_cache_invalidated(&self) -> bool {
        self.analysis_commit.lock().cache_invalidated
    }

    fn request_analysis(
        &mut self,
        mode: AnalysisMode,
        request: AnalysisRequest,
        trigger: AnalysisTrigger,
        delay: Duration,
    ) {
        let AnalysisRequest { disk_paths, removed_paths, retained_paths, changed_paths } = request;
        self.prepare_removed_file_diagnostics(&removed_paths);
        let Some((version, rediscover, progress)) = self.begin_analysis_retaining_paths(
            mode,
            removed_paths,
            &retained_paths,
            changed_paths,
            trigger,
        ) else {
            return;
        };
        if rediscover {
            if self.background_discovery {
                self.schedule_workspace_discovery(version, disk_paths, progress, delay);
            } else {
                match self.rediscover_workspaces() {
                    Ok(()) => {
                        let mut commit = self.analysis_commit.lock();
                        commit.discovery_pending = false;
                        commit.workspace_roots_before_change = None;
                        drop(commit);
                        self.schedule_analysis(version, disk_paths, progress, delay);
                    }
                    Err(error) => {
                        self.finish_workspace_discovery_failure(version, error, progress);
                    }
                }
            }
        } else {
            self.schedule_analysis(version, disk_paths, progress, delay);
        }
    }

    fn schedule_analysis(
        &self,
        version: usize,
        disk_paths: Vec<PathBuf>,
        progress: ProgressTicket,
        delay: Duration,
    ) {
        self.schedule_analysis_with_cancellation(
            version,
            disk_paths,
            progress,
            delay,
            IndexingCancellation::default(),
            true,
        );
    }

    fn schedule_workspace_discovery(
        &self,
        version: usize,
        disk_paths: Vec<PathBuf>,
        progress: ProgressTicket,
        delay: Duration,
    ) {
        let scheduler = self.analysis_scheduler.clone();
        let task_scheduler = scheduler.clone();
        let config = self.config.clone();
        let client = self.client.clone();
        let analysis_version = self.analysis_version.clone();
        let cancellation = IndexingCancellation::default();
        let task_key = AnalysisTaskKey { version, stage: AnalysisTaskStage::Discovery };

        let mut tasks = scheduler.tasks.lock();
        tasks.cancel();
        tasks.cancellation = Some(cancellation.clone());
        let coordinator = tokio::spawn(async move {
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            if cancellation.is_cancelled() || analysis_version.load(Ordering::Acquire) != version {
                return;
            }

            let Ok(permit) = task_scheduler.gate.clone().acquire_owned().await else {
                return;
            };
            if cancellation.is_cancelled() || analysis_version.load(Ordering::Acquire) != version {
                return;
            }

            progress.begin();
            progress.report("Discovering workspace sources");
            let worker_cancellation = cancellation.clone();
            let discovery_config = config.clone();
            let worker = tokio::task::spawn_blocking(move || {
                let _permit = permit;
                discovery_config.try_discover_workspaces(&worker_cancellation)
            });
            task_scheduler.tasks.lock().worker = Some((task_key, worker.abort_handle()));
            WorkspaceDiscoveryMonitor {
                version,
                disk_paths,
                progress,
                cancellation,
                analysis_version,
                client,
            }
            .finish(worker)
            .await;

            let mut tasks = task_scheduler.tasks.lock();
            tasks.clear_worker(task_key);
            tasks.clear_coordinator(task_key);
        });
        tasks.coordinator = Some((task_key, coordinator.abort_handle()));
    }

    pub(crate) fn on_workspace_discovery_ready(
        &mut self,
        mut event: WorkspaceDiscoveryReady,
    ) -> NotifyResult {
        if event.cancellation.is_cancelled()
            || self.analysis_version.load(Ordering::Acquire) != event.version
        {
            return ControlFlow::Continue(());
        }

        let removed_owners =
            Arc::make_mut(&mut self.config).apply_workspace_discovery(event.result);
        self.clear_removed_flycheck_diagnostics(removed_owners);
        let deferred_source_file_events = {
            let mut commit = self.analysis_commit.lock();
            commit.discovery_pending = false;
            commit.workspace_roots_before_change = None;
            mem::take(&mut commit.deferred_source_file_events)
        };
        let mut deferred_paths = Vec::new();
        let mut still_deferred = FxHashMap::default();
        for (path, typ) in deferred_source_file_events {
            if !self.config.tracks_watched_source_file(&path)
                && !self.vfs.read().exists(&crate::vfs::VfsPath::from(path.clone()))
            {
                still_deferred.insert(path, typ);
                continue;
            }
            self.reconcile_deferred_source_file(&path, typ);
            deferred_paths.push(path);
        }
        if !still_deferred.is_empty() {
            self.analysis_commit.lock().deferred_source_file_events.extend(still_deferred);
        }
        if !deferred_paths.is_empty() {
            self.analysis_commit
                .lock()
                .natspec_pending_source_changes
                .extend(deferred_paths.iter().cloned());
            event.disk_paths.extend(deferred_paths);
            event.disk_paths.sort();
            event.disk_paths.dedup();
        }
        self.reregister_watched_files();
        self.schedule_analysis_with_cancellation(
            event.version,
            event.disk_paths,
            event.progress,
            Duration::ZERO,
            event.cancellation,
            false,
        );
        ControlFlow::Continue(())
    }

    pub(crate) fn on_workspace_discovery_failed(
        &mut self,
        event: WorkspaceDiscoveryFailed,
    ) -> NotifyResult {
        if self.analysis_version.load(Ordering::Acquire) != event.version {
            return ControlFlow::Continue(());
        }
        self.finish_workspace_discovery_failure(event.version, event.error, event.progress);
        ControlFlow::Continue(())
    }

    pub(crate) fn on_deferred_source_file_events_ready(
        &mut self,
        event: DeferredSourceFileEventsReady,
    ) -> NotifyResult {
        if self.analysis_version.load(Ordering::Acquire) != event.version {
            return ControlFlow::Continue(());
        }

        let mut disk_paths = Vec::with_capacity(event.events.len());
        let mut removed_paths = Vec::new();
        for (path, typ) in event.events {
            if self.reconcile_deferred_source_file(&path, typ) {
                removed_paths.push(path.clone());
            }
            disk_paths.push(path);
        }
        self.recompute_for_file_changes(disk_paths, removed_paths, false);
        ControlFlow::Continue(())
    }

    pub(crate) fn on_watched_file_registration_ready(
        &mut self,
        _: WatchedFileRegistrationReady,
    ) -> NotifyResult {
        let current_missing = self.analysis_commit.lock().analysis_paths.missing_candidates.clone();
        let disk_paths = current_missing
            .into_iter()
            .filter(|path| std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file()))
            .collect::<Vec<_>>();
        if !disk_paths.is_empty() {
            self.recompute_for_file_changes(disk_paths, Vec::new(), true);
        }
        ControlFlow::Continue(())
    }

    fn schedule_analysis_with_cancellation(
        &self,
        version: usize,
        disk_paths: Vec<PathBuf>,
        progress: ProgressTicket,
        delay: Duration,
        cancellation: IndexingCancellation,
        cancel_previous: bool,
    ) {
        let scheduler = self.analysis_scheduler.clone();
        let task_scheduler = scheduler.clone();
        let mut snapshot = self.snapshot();
        let analysis_version = self.analysis_version.clone();
        let published_analysis_version = self.published_analysis_version.clone();
        let analysis_commit = self.analysis_commit.clone();
        let client = self.client.clone();
        let config = self.config.clone();
        let task_key = AnalysisTaskKey { version, stage: AnalysisTaskStage::Analysis };

        let mut tasks = scheduler.tasks.lock();
        if cancel_previous {
            tasks.cancel();
        }
        tasks.cancellation = Some(cancellation.clone());
        let coordinator = tokio::spawn(async move {
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }

            let Ok(permit) = task_scheduler.gate.clone().acquire_owned().await else {
                return;
            };
            let worker = {
                let mut tasks = task_scheduler.tasks.lock();
                if cancellation.is_cancelled() || !snapshot.is_current(version) {
                    return;
                }

                progress.begin();
                let worker_progress = progress.clone();
                let worker_cancellation = cancellation.clone();
                let worker = tokio::task::spawn_blocking(move || {
                    let _permit = permit;
                    run_analysis(
                        &mut snapshot,
                        version,
                        disk_paths,
                        &worker_progress,
                        &worker_cancellation,
                    )
                });
                tasks.worker = Some((task_key, worker.abort_handle()));
                worker
            };

            if let Some(refresh_requests) = monitor_analysis_task(
                version,
                worker,
                progress,
                &analysis_version,
                &published_analysis_version,
                &analysis_commit,
            )
            .await
            {
                request_pull_result_refreshes(&client, &config, refresh_requests);
            }

            let mut tasks = task_scheduler.tasks.lock();
            tasks.clear_worker(task_key);
            tasks.clear_coordinator(task_key);
        });
        tasks.coordinator = Some((task_key, coordinator.abort_handle()));
    }

    #[cfg(test)]
    fn begin_analysis(
        &mut self,
        mode: AnalysisMode,
        removed_paths: Vec<PathBuf>,
        changed_paths: Vec<PathBuf>,
        trigger: AnalysisTrigger,
    ) -> Option<(usize, ProgressTicket)> {
        self.begin_analysis_retaining_paths(mode, removed_paths, &[], changed_paths, trigger)
            .map(|(version, _, progress)| (version, progress))
    }

    fn begin_analysis_retaining_paths(
        &mut self,
        mode: AnalysisMode,
        removed_paths: Vec<PathBuf>,
        retained_paths: &[PathBuf],
        changed_paths: Vec<PathBuf>,
        trigger: AnalysisTrigger,
    ) -> Option<(usize, bool, ProgressTicket)> {
        let (version, rediscover, progress) = {
            let analysis_commit = self.analysis_commit.clone();
            let mut commit = analysis_commit.lock();
            if matches!(mode, AnalysisMode::IfInvalidated) && !commit.cache_invalidated {
                return None;
            }

            let invalidated = mem::take(&mut commit.cache_invalidated);
            let rediscover =
                matches!(mode, AnalysisMode::Rediscover) || invalidated || commit.discovery_pending;
            commit.discovery_pending = rediscover;
            let refresh_pull_results = invalidated || matches!(trigger, AnalysisTrigger::External);
            let version = self.next_analysis_version();
            // Reserve progress before publishing the epoch so a delayed create response cannot end
            // the previous wave after the new analysis becomes current. The progress delay is armed
            // after debounce and scheduler wait complete.
            let progress = self.analysis_progress.reserve(version);
            if refresh_pull_results {
                commit.begin_external_refresh();
            }
            self.commit_analysis_epoch(&mut commit, version, changed_paths, rediscover);
            let update =
                self.diagnostics.write().clear_file_path_prefixes_retaining_and_publish_batches(
                    &removed_paths,
                    retained_paths,
                );
            commit.record_external_diagnostics_change(
                update.pull_reports_changed || update.workspace_documents_changed,
            );
            publish_diagnostic_batches(&mut self.client, update.batches, &self.config);
            (version, rediscover, progress)
        };

        Some((version, rediscover, progress))
    }

    fn next_analysis_version(&self) -> usize {
        self.analysis_version.load(Ordering::Relaxed).wrapping_add(1)
    }

    fn rediscover_workspaces(&mut self) -> Result<(), WorkspaceError> {
        let removed_owners = Arc::make_mut(&mut self.config).try_rediscover_workspaces()?;
        self.clear_removed_flycheck_diagnostics(removed_owners);
        self.reregister_watched_files();
        Ok(())
    }

    fn finish_workspace_discovery_failure(
        &mut self,
        version: usize,
        error: impl std::fmt::Display,
        progress: ProgressTicket,
    ) {
        let roots = self.analysis_commit.lock().workspace_roots_before_change.take();
        if let Some(roots) = roots {
            Arc::make_mut(&mut self.config).replace_workspace_roots(roots);
            self.reregister_watched_files();
        }
        if let Some(refresh_requests) = handle_analysis_failure(
            version,
            error,
            &self.analysis_version,
            &self.published_analysis_version,
            &self.analysis_commit,
        ) {
            finish_analysis_progress_if_current(
                version,
                &self.analysis_version,
                &self.analysis_commit,
                &progress,
                "Workspace indexing failed",
            );
            request_pull_result_refreshes(&self.client, &self.config, refresh_requests);
        }
    }

    #[cfg(test)]
    fn begin_analysis_epoch(
        &self,
        commit: &mut AnalysisCommitState,
        changed_paths: Vec<PathBuf>,
        context_changed: bool,
    ) -> usize {
        let version = self.next_analysis_version();
        self.commit_analysis_epoch(commit, version, changed_paths, context_changed);
        version
    }

    fn commit_analysis_epoch(
        &self,
        commit: &mut AnalysisCommitState,
        version: usize,
        changed_paths: Vec<PathBuf>,
        context_changed: bool,
    ) {
        commit.vfs_content_revision = self.vfs.read().content_revision();
        if context_changed {
            commit.natspec_context_change_version = version;
        }
        commit.natspec_pending_source_changes.extend(changed_paths);
        self.analysis_version.store(version, Ordering::Release);
    }

    /// Waits for analysis results at least as new as the latest version requested before this call.
    pub(crate) fn latest_analysis(
        &self,
    ) -> impl Future<Output = Result<Arc<RwLock<SymbolTables>>, ResponseError>> + use<> {
        let mut published = self.published_analysis_version.subscribe();
        let version = self.analysis_version.load(Ordering::Acquire);
        let symbol_tables = self.symbol_tables.clone();
        async move {
            published.wait_for(|published| *published >= version).await.map_err(|_| {
                ResponseError::new(async_lsp::ErrorCode::REQUEST_FAILED, "analysis was cancelled")
            })?;
            Ok(symbol_tables)
        }
    }

    /// Waits for the latest analysis and returns the config snapshot that produced it.
    pub(crate) fn latest_analysis_with_config(
        &self,
    ) -> impl Future<Output = Result<(Arc<RwLock<SymbolTables>>, Arc<Config>), ResponseError>> + use<>
    {
        let latest_analysis = self.latest_analysis();
        let analysis_commit = self.analysis_commit.clone();
        let fallback_config = self.config.clone();
        async move {
            let symbol_tables = latest_analysis.await?;
            let config = analysis_commit.lock().analysis_config.clone().unwrap_or(fallback_config);
            Ok((symbol_tables, config))
        }
    }

    pub(crate) fn pull_diagnostic_report(
        &self,
        uri: Url,
        previous_result_id: Option<String>,
    ) -> impl Future<Output = Result<PullReport, ResponseError>> + use<> {
        let (uri, latest_analysis) = match uri.to_file_path() {
            Ok(path) => (Url::from_file_path(path).unwrap_or(uri), Some(self.latest_analysis())),
            Err(()) => (uri, None),
        };
        let diagnostics = self.diagnostics.clone();
        let include_data = self.config.supports_pull_diagnostics_data();
        async move {
            if let Some(latest_analysis) = latest_analysis {
                latest_analysis.await?;
            }
            let mut report = diagnostics.read().pull_report(&uri, previous_result_id.as_deref());
            if !include_data {
                strip_pull_report_data(&mut report);
            }
            Ok(report)
        }
    }

    pub(crate) fn code_action_diagnostics(
        &self,
        uri: Url,
    ) -> impl Future<Output = Result<Vec<Diagnostic>, ResponseError>> + use<> {
        let (uri, latest_analysis) = match uri.to_file_path() {
            Ok(path) => (Url::from_file_path(path).unwrap_or(uri), Some(self.latest_analysis())),
            Err(()) => (uri, None),
        };
        let diagnostics = self.diagnostics.clone();
        async move {
            if let Some(latest_analysis) = latest_analysis {
                latest_analysis.await?;
            }
            let PullReport::Full { diagnostics, .. } = diagnostics.read().pull_report(&uri, None)
            else {
                unreachable!("a report without a result ID is full")
            };
            Ok(diagnostics)
        }
    }

    pub(crate) fn workspace_diagnostic_reports(
        &self,
        previous_result_ids: Vec<PreviousResultId>,
    ) -> impl Future<Output = Result<Vec<WorkspacePullReport>, ResponseError>> + use<> {
        let latest_analysis = self.latest_analysis();
        let vfs = self.vfs.clone();
        let diagnostics = self.diagnostics.clone();
        let include_data = self.config.supports_pull_diagnostics_data();
        async move {
            latest_analysis.await?;
            let vfs = vfs.read();
            let mut reports = diagnostics.read().workspace_pull_reports(previous_result_ids);
            for report in &mut reports {
                if !include_data {
                    strip_pull_report_data(&mut report.report);
                }
                if report.is_stale
                    && let Some(path) = proto::vfs_path(&report.uri)
                    && let Some(version) = vfs.get_file_version(&path)
                {
                    report.version = Some(i64::from(version));
                }
            }
            Ok(reports)
        }
    }

    pub(crate) fn natspec_semantics_are_usable(&self, request_uri: &Url) -> bool {
        let request_path = request_uri.to_file_path().ok();
        let (analysis_version, symbol_tables_version, context_change_version, pending_paths) = {
            let commit = self.analysis_commit.lock();
            (
                self.analysis_version.load(Ordering::Acquire),
                commit.symbol_tables_version,
                commit.natspec_context_change_version,
                commit.natspec_pending_source_changes.iter().cloned().collect::<Vec<_>>(),
            )
        };
        if symbol_tables_version >= analysis_version {
            return true;
        }
        if context_change_version > symbol_tables_version {
            return false;
        }

        for path in pending_paths {
            if request_path.as_deref() == Some(path.as_path()) {
                continue;
            }
            let Ok(uri) = Url::from_file_path(&path) else { return false };
            let analyzed =
                self.symbol_tables.read().natspec_source_fingerprint(&uri).map(str::to_owned);
            let vfs_path = crate::vfs::VfsPath::from(path.clone());
            let open_contents = self.vfs.read().get_file_contents(&vfs_path).cloned();
            let current = open_contents
                .map(|contents| contents.to_string())
                .or_else(|| self.sess.source_map().file_loader().load_file(&path).ok());
            let current =
                current.as_deref().map(crate::natspec_completion::source_syntax_fingerprint);
            if !matches!((analyzed.as_deref(), current.as_deref()),
                (Some(analyzed), Some(current)) if analyzed == current
            ) {
                return false;
            }
        }
        true
    }

    #[cfg(test)]
    pub(crate) fn mark_analysis_pending_for_test(&self) {
        let analysis_commit = self.analysis_commit.clone();
        let mut commit = analysis_commit.lock();
        self.begin_analysis_epoch(&mut commit, Vec::new(), false);
    }

    #[cfg(test)]
    pub(crate) fn mark_source_analysis_pending_for_test(&self, path: PathBuf) {
        let analysis_commit = self.analysis_commit.clone();
        let mut commit = analysis_commit.lock();
        self.begin_analysis_epoch(&mut commit, vec![path], false);
    }

    #[cfg(test)]
    pub(crate) fn mark_context_analysis_pending_for_test(&self) {
        let analysis_commit = self.analysis_commit.clone();
        let mut commit = analysis_commit.lock();
        self.begin_analysis_epoch(&mut commit, Vec::new(), true);
    }

    #[cfg(test)]
    pub(crate) fn replace_diagnostics_for_test(&self, diagnostics: DiagnosticMap) {
        self.diagnostics
            .write()
            .replace_and_publish_batches(DiagnosticOwner::Compiler, diagnostics);
    }

    pub(crate) fn run_flychecks_on_save(&mut self, path: PathBuf) {
        let timeout = self.config.flycheck_timeout();
        for flycheck in self.config.flychecks_for_path(&path) {
            let owner = flycheck.owner();
            let version = self.begin_flycheck_epoch(&owner);
            let id = flycheck.id.clone();
            let mut snapshot = self.snapshot();
            let source_paths = snapshot.flycheck_source_paths(&flycheck, &path);
            let (cancel, cancelled) = oneshot::channel();
            let task_owner = owner.clone();
            tokio::spawn(async move {
                let result = flycheck::run(flycheck, timeout, cancelled, source_paths).await;
                if !snapshot.is_current_flycheck(&task_owner, version) {
                    return;
                }

                match result {
                    Ok(diagnostics) => {
                        snapshot.publish_flycheck_diagnostics(task_owner, version, diagnostics)
                    }
                    Err(error) => {
                        tracing::warn!(%id, %error, "flycheck failed");
                        snapshot.publish_flycheck_diagnostics(
                            task_owner,
                            version,
                            DiagnosticMap::default(),
                        );
                    }
                }
            });
            self.flycheck_cancels.insert(owner, cancel);
        }
    }

    pub(crate) fn clear_removed_flycheck_diagnostics(
        &mut self,
        owners: impl IntoIterator<Item = DiagnosticOwner>,
    ) {
        let owners = owners.into_iter().collect::<Vec<_>>();
        for owner in &owners {
            self.begin_flycheck_epoch(owner);
        }

        let mut snapshot = self.snapshot();
        let refresh_diagnostics = snapshot.clear_diagnostic_owners(owners);
        request_pull_result_refreshes(
            &self.client,
            &self.config,
            RefreshRequests { diagnostics: refresh_diagnostics, inlay_hints: false },
        );
    }

    fn prepare_removed_file_diagnostics(&mut self, paths: &[PathBuf]) {
        if paths.is_empty() {
            return;
        }

        let owners = self.config.flycheck_owners().collect::<FxHashSet<_>>();
        for owner in owners {
            self.begin_flycheck_epoch(&owner);
        }
    }

    fn begin_flycheck_epoch(&mut self, owner: &DiagnosticOwner) -> usize {
        let version = {
            let analysis_commit = self.analysis_commit.clone();
            let _commit = analysis_commit.lock();
            let mut versions = self.flycheck_versions.write();
            let version = versions.get(owner).copied().unwrap_or_default() + 1;
            versions.insert(owner.clone(), version);
            version
        };
        self.cancel_flycheck(owner);
        version
    }

    fn cancel_flycheck(&mut self, owner: &DiagnosticOwner) {
        if let Some(cancel) = self.flycheck_cancels.remove(owner) {
            let _ = cancel.send(());
        }
    }

    fn snapshot(&self) -> GlobalStateSnapshot {
        GlobalStateSnapshot {
            client: self.client.clone(),
            vfs: self.vfs.clone(),
            config: self.config.clone(),
            analysis_version: self.analysis_version.clone(),
            published_analysis_version: self.published_analysis_version.clone(),
            analysis_commit: self.analysis_commit.clone(),
            watched_file_registration: self.watched_file_registration.clone(),
            flycheck_versions: self.flycheck_versions.clone(),
            symbol_tables: self.symbol_tables.clone(),
            diagnostics: self.diagnostics.clone(),
        }
    }

    #[cfg(test)]
    fn monitor_analysis_task(
        &self,
        version: usize,
        task: JoinHandle<AnalysisTaskOutcome>,
        progress: ProgressTicket,
    ) {
        let analysis_version = self.analysis_version.clone();
        let published_analysis_version = self.published_analysis_version.clone();
        let analysis_commit = self.analysis_commit.clone();
        let client = self.client.clone();
        let config = self.config.clone();
        tokio::spawn(async move {
            if let Some(refresh_requests) = monitor_analysis_task(
                version,
                task,
                progress,
                &analysis_version,
                &published_analysis_version,
                &analysis_commit,
            )
            .await
            {
                request_pull_result_refreshes(&client, &config, refresh_requests);
            }
        });
    }
}

fn run_analysis(
    snapshot: &mut GlobalStateSnapshot,
    version: usize,
    disk_paths: Vec<PathBuf>,
    progress: &ProgressTicket,
    cancellation: &IndexingCancellation,
) -> AnalysisTaskOutcome {
    let has_disk_paths = !disk_paths.is_empty();
    let vfs_content_revision = snapshot.vfs.read().content_revision();
    let config = snapshot.config.clone();
    if has_disk_paths {
        snapshot.analysis_commit.lock().cached_output = None;
    }
    if !has_disk_paths && !cancellation.is_cancelled() && snapshot.is_current(version) {
        let cached = {
            let commit = snapshot.analysis_commit.lock();
            if commit.cache_invalidated {
                None
            } else {
                commit.cached_output.as_ref().and_then(|cached| {
                    (cached.vfs_content_revision == vfs_content_revision
                        && Arc::ptr_eq(&cached.config, &config))
                    .then(|| cached.output.clone())
                })
            }
        };
        if let Some(output) = cached {
            progress.report("Reusing workspace index");
            if snapshot.publish_analysis_output(version, output) {
                return AnalysisTaskOutcome::Published;
            }
            return AnalysisTaskOutcome::Superseded;
        }
    }

    progress.report("Reading workspace sources");
    if cancellation.is_cancelled() || !snapshot.is_current(version) {
        return AnalysisTaskOutcome::Superseded;
    }

    let Some((batches, source_files_complete)) =
        snapshot.analysis_batches_cancellable(disk_paths, cancellation)
    else {
        return AnalysisTaskOutcome::Superseded;
    };
    if !source_files_complete {
        Arc::make_mut(&mut snapshot.config).mark_analysis_source_files_incomplete();
    }
    progress.report("Analyzing workspace");
    if cancellation.is_cancelled() || !snapshot.is_current(version) {
        return AnalysisTaskOutcome::Superseded;
    }

    let mut results = AnalysisOutputAccumulator::default();

    for batch in batches {
        if batch.files.is_empty() {
            continue;
        }

        if cancellation.is_cancelled() || !snapshot.is_current(version) {
            return AnalysisTaskOutcome::Superseded;
        }

        let Some(result) = analyze_cancellable(batch, cancellation) else {
            return AnalysisTaskOutcome::Superseded;
        };
        results.push(result);

        if cancellation.is_cancelled() || !snapshot.is_current(version) {
            return AnalysisTaskOutcome::Superseded;
        }
    }

    let output = results.finish();
    if !has_disk_paths {
        snapshot.analysis_commit.lock().cached_output =
            Some(CachedAnalysisOutput { vfs_content_revision, config, output: output.clone() });
    }
    progress.report("Publishing workspace index");
    if snapshot.publish_analysis_output(version, output) {
        AnalysisTaskOutcome::Published
    } else {
        AnalysisTaskOutcome::Superseded
    }
}

async fn monitor_analysis_task(
    version: usize,
    task: JoinHandle<AnalysisTaskOutcome>,
    progress: ProgressTicket,
    analysis_version: &Arc<AtomicUsize>,
    published_analysis_version: &watch::Sender<usize>,
    analysis_commit: &Arc<Mutex<AnalysisCommitState>>,
) -> Option<RefreshRequests> {
    match task.await {
        Ok(AnalysisTaskOutcome::Published) => {
            finish_analysis_progress_if_current(
                version,
                analysis_version,
                analysis_commit,
                &progress,
                "Workspace index ready",
            );
            None
        }
        Ok(AnalysisTaskOutcome::Superseded) => None,
        Err(error) => {
            let refresh_requests = handle_analysis_failure(
                version,
                error,
                analysis_version,
                published_analysis_version,
                analysis_commit,
            )?;
            finish_analysis_progress_if_current(
                version,
                analysis_version,
                analysis_commit,
                &progress,
                "Workspace indexing failed",
            );
            Some(refresh_requests)
        }
    }
}

fn finish_analysis_progress_if_current(
    version: usize,
    analysis_version: &Arc<AtomicUsize>,
    analysis_commit: &Arc<Mutex<AnalysisCommitState>>,
    progress: &ProgressTicket,
    message: &'static str,
) {
    if progress.is_disabled() {
        return;
    }

    let _commit = analysis_commit.lock();
    if analysis_version.load(Ordering::Acquire) == version {
        progress.finish(message);
    }
}

fn handle_analysis_failure(
    version: usize,
    error: impl std::fmt::Display,
    analysis_version: &Arc<AtomicUsize>,
    published_analysis_version: &watch::Sender<usize>,
    analysis_commit: &Arc<Mutex<AnalysisCommitState>>,
) -> Option<RefreshRequests> {
    let mut commit = analysis_commit.lock();
    if analysis_version.load(Ordering::Acquire) != version {
        return None;
    }

    tracing::warn!(%error, version, "workspace indexing task failed");
    let refresh_requests = commit.fail_external_refresh();
    commit.cache_invalidated = true;
    commit.cached_output = None;
    commit.discovery_pending = false;
    commit.natspec_context_change_version = commit.natspec_context_change_version.max(version);
    published_analysis_version.send_replace(version);
    Some(refresh_requests)
}

#[derive(Clone)]
struct AnalysisResult {
    analyzed_documents: AnalyzedDocuments,
    diagnostics: DiagnosticMap,
    symbol_tables: SymbolTables,
}

#[derive(Clone)]
struct AnalysisOutput {
    result: AnalysisResult,
    analysis_paths: AnalysisPathIndex,
}

#[derive(Default)]
struct AnalysisOutputAccumulator {
    result: AnalysisResultAccumulator,
    analysis_paths: AnalysisPathIndex,
}

impl AnalysisOutputAccumulator {
    fn push(&mut self, output: AnalysisOutput) {
        self.result.push(output.result);
        self.analysis_paths.merge(output.analysis_paths);
    }

    fn finish(mut self) -> AnalysisOutput {
        self.analysis_paths
            .existing_unresolved_candidates
            .retain(|path| !self.analysis_paths.resolved_dependencies.contains(path));
        self.analysis_paths.missing_candidates.retain(|path| {
            !self.analysis_paths.resolved_dependencies.contains(path)
                && !self.analysis_paths.existing_unresolved_candidates.contains(path)
        });
        AnalysisOutput { result: self.result.finish(), analysis_paths: self.analysis_paths }
    }
}

#[derive(Default)]
struct AnalysisResultAccumulator {
    analyzed_documents: AnalyzedDocuments,
    diagnostics: DiagnosticMap,
    symbol_tables: SymbolTablesAggregator,
}

impl AnalysisResultAccumulator {
    fn push(&mut self, result: AnalysisResult) {
        let AnalysisResult { analyzed_documents, diagnostics, symbol_tables } = result;
        for (uri, version) in analyzed_documents {
            self.analyzed_documents
                .entry(uri)
                .and_modify(|current| *current = (*current).max(version))
                .or_insert(version);
        }
        for (uri, mut batch_diagnostics) in diagnostics {
            self.diagnostics.entry(uri).or_default().append(&mut batch_diagnostics);
        }
        self.symbol_tables.push(symbol_tables);
    }

    fn finish(self) -> AnalysisResult {
        AnalysisResult {
            analyzed_documents: self.analyzed_documents,
            diagnostics: self.diagnostics,
            symbol_tables: self.symbol_tables.finish(),
        }
    }
}

const MAX_DYNAMIC_WATCHED_FILE_SPECS: usize = 256;

#[derive(Default)]
struct WatchedSolidityCoverage {
    recursive_roots: FxHashSet<PathBuf>,
    shallow_roots: FxHashSet<PathBuf>,
}

impl WatchedSolidityCoverage {
    fn new(specs: &[WatchedFileSpec]) -> Self {
        let mut coverage = Self::default();
        for spec in specs {
            let roots = match spec.pattern {
                "**/*.sol" => &mut coverage.recursive_roots,
                "*.sol" => &mut coverage.shallow_roots,
                _ => continue,
            };
            roots.insert(spec.base.normalize());
        }
        coverage
    }

    fn covers(&self, path: &Path) -> bool {
        self.shallow_roots.contains(path) || self.covers_recursively(path)
    }

    fn covers_recursively(&self, path: &Path) -> bool {
        path.ancestors().any(|ancestor| self.recursive_roots.contains(ancestor))
    }
}

fn dependency_watch_roots(config: &Config) -> FxHashSet<PathBuf> {
    let mut roots =
        config.workspace_roots().iter().map(|root| root.normalize()).collect::<FxHashSet<_>>();
    for workspace in config.workspaces() {
        let opts = workspace.compile_opts();
        let base_path = opts.base_path.as_deref();
        if let Some(base_path) = base_path {
            roots.insert(base_path.normalize());
        }
        roots.extend(
            opts.include_paths
                .iter()
                .filter_map(|path| resolve_dependency_watch_root(base_path, path)),
        );
        roots.extend(opts.import_remappings.iter().filter_map(|remapping| {
            resolve_dependency_watch_root(base_path, Path::new(&remapping.path))
        }));
    }
    roots
}

fn resolve_dependency_watch_root(base_path: Option<&Path>, path: &Path) -> Option<PathBuf> {
    if path.is_absolute() {
        Some(path.normalize())
    } else {
        base_path.map(|base_path| base_path.join(path).normalize())
    }
}

fn watched_file_specs(config: &Config, analysis_paths: &AnalysisPathIndex) -> Vec<WatchedFileSpec> {
    let mut specs = config.watched_file_specs();
    let coverage = WatchedSolidityCoverage::new(&specs);
    let approved_roots = dependency_watch_roots(config);
    let mut dependency_specs = analysis_paths
        .resolved_dependencies
        .iter()
        .chain(analysis_paths.existing_unresolved_candidates.iter())
        .filter_map(|path| {
            path.parent().map(Path::normalize).map(|parent| WatchedFileSpec::new(parent, "*.sol"))
        })
        .chain(analysis_paths.missing_candidates.iter().flat_map(|path| {
            let mut specs = Vec::new();
            if let Some(parent) = path.parent() {
                let parent_exists = parent.is_dir();
                if parent_exists {
                    specs.push(WatchedFileSpec::new(parent.normalize(), "*.sol"));
                }
                specs.extend(
                    parent
                        .ancestors()
                        .skip(usize::from(parent_exists))
                        .filter(|ancestor| ancestor.is_dir())
                        .map(|ancestor| WatchedFileSpec::new(ancestor.normalize(), "*")),
                );
            }
            specs
        }))
        .filter(|spec| {
            spec.base.ancestors().any(|ancestor| approved_roots.contains(ancestor))
                && if spec.pattern == "*.sol" {
                    !coverage.covers(&spec.base)
                } else {
                    !coverage.covers_recursively(&spec.base)
                }
        })
        .collect::<Vec<_>>();
    dependency_specs.sort_unstable_by(|a, b| {
        let a_fallback = a.pattern == "*";
        let b_fallback = b.pattern == "*";
        a_fallback
            .cmp(&b_fallback)
            .then_with(|| a.base.components().count().cmp(&b.base.components().count()))
            .then_with(|| a.cmp(b))
    });
    dependency_specs.dedup();
    specs.extend(dependency_specs.into_iter().take(MAX_DYNAMIC_WATCHED_FILE_SPECS));
    specs.sort_unstable();
    specs.dedup();
    specs
}

fn prepare_watched_file_registration_update(
    config: &Config,
    coordinator: &WatchedFileRegistrationCoordinator,
    specs: Vec<WatchedFileSpec>,
) -> Option<WatchedFileRegistrationUpdate> {
    if !config.supports_watched_file_dynamic_registration() {
        return None;
    }
    let relative_patterns = config.supports_watched_file_relative_patterns();
    let mut current_specs = coordinator.desired_specs.lock();
    if current_specs.as_ref().is_some_and(|current| {
        if relative_patterns { current == &specs } else { current.is_empty() }
    }) {
        return None;
    }
    let desired_specs = if relative_patterns { specs } else { Vec::new() };
    *current_specs = Some(desired_specs.clone());
    let generation = coordinator.generation.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
    drop(current_specs);

    let registration_id = format!("solar-watched-files-{generation}");
    let registration =
        watched_file_registration_params_with_specs(config, &desired_specs, &registration_id);
    Some(WatchedFileRegistrationUpdate { generation, desired_specs, registration_id, registration })
}

fn spawn_watched_file_registration_update(
    client: &ClientSocket,
    coordinator: &Arc<WatchedFileRegistrationCoordinator>,
    update: Option<WatchedFileRegistrationUpdate>,
) {
    let Some(WatchedFileRegistrationUpdate {
        generation,
        desired_specs,
        registration_id,
        registration,
    }) = update
    else {
        return;
    };
    let coordinator = coordinator.clone();
    let mut client = client.clone();
    tokio::spawn(async move {
        let _guard = coordinator.gate.lock().await;
        if coordinator.generation.load(Ordering::Acquire) != generation {
            return;
        }
        if let Err(error) = client.register_capability(registration).await {
            tracing::warn!(%error, "failed to register watched-file notifications");
            if coordinator.generation.load(Ordering::Acquire) == generation {
                let mut current_specs = coordinator.desired_specs.lock();
                if current_specs.as_ref() == Some(&desired_specs)
                    && coordinator.generation.load(Ordering::Acquire) == generation
                {
                    *current_specs = None;
                }
            }
            return;
        }
        let previous_ids = {
            let mut active_ids = coordinator.active_registration_ids.lock();
            let previous_ids = active_ids.clone();
            active_ids.push(registration_id);
            previous_ids
        };
        let _ = client.emit(WatchedFileRegistrationReady);
        for previous_id in previous_ids {
            if coordinator.generation.load(Ordering::Acquire) != generation {
                break;
            }
            match unregister_watched_file_registration(&mut client, previous_id.clone()).await {
                Ok(()) => {
                    coordinator.active_registration_ids.lock().retain(|id| id != &previous_id);
                }
                Err(error) => {
                    tracing::warn!(%error, "failed to unregister watched-file notifications");
                }
            }
        }
    });
}

async fn unregister_watched_file_registration(
    client: &mut ClientSocket,
    id: String,
) -> async_lsp::Result<()> {
    let params = UnregistrationParams {
        unregisterations: vec![Unregistration { id, method: DidChangeWatchedFiles::METHOD.into() }],
    };
    client.unregister_capability(params).await
}

#[cfg(test)]
fn watched_file_registration_params(config: &Config) -> RegistrationParams {
    watched_file_registration_params_with_specs(
        config,
        &config.watched_file_specs(),
        "solar-watched-files",
    )
}

fn watched_file_registration_params_with_specs(
    config: &Config,
    specs: &[WatchedFileSpec],
    registration_id: &str,
) -> RegistrationParams {
    let watchers = if config.supports_watched_file_relative_patterns() {
        specs
            .iter()
            .filter_map(|spec| {
                let base_uri = Url::from_file_path(&spec.base).ok()?;
                Some(FileSystemWatcher {
                    glob_pattern: GlobPattern::Relative(RelativePattern {
                        base_uri: OneOf::Right(base_uri),
                        pattern: spec.pattern.into(),
                    }),
                    kind: Some(spec.kind),
                })
            })
            .collect::<Vec<_>>()
    } else {
        let mut watchers = [
            ("**/*.sol", WatchKind::Create | WatchKind::Change | WatchKind::Delete),
            ("**/foundry.toml", WatchKind::Create | WatchKind::Change | WatchKind::Delete),
            ("**/remappings.txt", WatchKind::Create | WatchKind::Change | WatchKind::Delete),
        ]
        .into_iter()
        .map(|(pattern, kind)| FileSystemWatcher {
            glob_pattern: GlobPattern::String(pattern.into()),
            kind: Some(kind),
        })
        .collect::<Vec<_>>();
        if config.watches_nested_repository_markers() {
            watchers.push(FileSystemWatcher {
                glob_pattern: GlobPattern::String("**/.git".into()),
                kind: Some(WatchKind::Create | WatchKind::Delete),
            });
        }
        watchers
    };
    let options = DidChangeWatchedFilesRegistrationOptions { watchers };

    RegistrationParams {
        registrations: vec![Registration {
            id: registration_id.into(),
            method: DidChangeWatchedFiles::METHOD.into(),
            register_options: Some(serde_json::to_value(options).unwrap()),
        }],
    }
}

fn publish_diagnostic_batches(
    client: &mut ClientSocket,
    batches: impl IntoIterator<Item = (Url, Vec<Diagnostic>)>,
    config: &Config,
) {
    if !config.uses_push_diagnostics() {
        return;
    }
    let include_data = config.supports_publish_diagnostics_data();
    for (uri, mut uri_diagnostics) in batches {
        if !include_data {
            for diagnostic in &mut uri_diagnostics {
                diagnostic.data = None;
            }
        }
        let _ =
            client.publish_diagnostics(PublishDiagnosticsParams::new(uri, uri_diagnostics, None));
    }
}

fn strip_pull_report_data(report: &mut PullReport) {
    if let PullReport::Full { diagnostics, .. } = report {
        for diagnostic in diagnostics {
            diagnostic.data = None;
        }
    }
}

pub(crate) struct GlobalStateSnapshot {
    client: ClientSocket,
    vfs: Arc<RwLock<Vfs>>,
    config: Arc<Config>,
    analysis_version: Arc<AtomicUsize>,
    published_analysis_version: watch::Sender<usize>,
    analysis_commit: Arc<Mutex<AnalysisCommitState>>,
    watched_file_registration: Arc<WatchedFileRegistrationCoordinator>,
    flycheck_versions: Arc<RwLock<FxHashMap<DiagnosticOwner, usize>>>,
    symbol_tables: Arc<RwLock<SymbolTables>>,
    diagnostics: Arc<RwLock<DiagnosticStore>>,
}

impl GlobalStateSnapshot {
    fn is_current(&self, version: usize) -> bool {
        self.analysis_version.load(Ordering::Acquire) == version
    }

    fn is_current_flycheck(&self, owner: &DiagnosticOwner, version: usize) -> bool {
        self.flycheck_versions.read().get(owner).copied().unwrap_or_default() == version
    }

    fn flycheck_source_paths(
        &self,
        flycheck: &flycheck::FlycheckConfig,
        saved_path: &Path,
    ) -> Vec<PathBuf> {
        let workspaces = self.config.workspaces();
        let mut paths = workspaces
            .iter()
            .flat_map(|workspace| workspace.source_files())
            .filter(|path| path.starts_with(&flycheck.workspace_root))
            .cloned()
            .collect::<Vec<_>>();
        paths.extend(
            workspaces
                .iter()
                .filter(|workspace| {
                    workspace.compile_opts().base_path.as_deref()
                        == Some(flycheck.workspace_root.as_path())
                })
                .flat_map(|workspace| workspace.flycheck_source_files())
                .cloned(),
        );
        paths.push(saved_path.to_path_buf());
        paths.sort_unstable();
        paths.dedup();
        paths
    }

    #[cfg(any(test, feature = "bench"))]
    fn analysis_batches(&self, disk_paths: Vec<PathBuf>) -> Vec<AnalysisBatch> {
        self.analysis_batches_cancellable(disk_paths, &IndexingCancellation::default())
            .map(|(batches, _)| batches)
            .unwrap_or_default()
    }

    fn analysis_batches_cancellable(
        &self,
        disk_paths: Vec<PathBuf>,
        cancellation: &IndexingCancellation,
    ) -> Option<(Vec<AnalysisBatch>, bool)> {
        let vfs_files = {
            let vfs = self.vfs.read();
            let mut files = Vec::new();
            for (path, _) in vfs.iter() {
                if cancellation.is_cancelled() {
                    return None;
                }
                let Some(path_buf) = path.as_path() else { continue };
                let contents = vfs
                    .get_file_analysis_source(path)
                    .expect("iterated VFS path should retain its source");
                files.push((path_buf.to_path_buf(), contents, vfs.get_file_version(path)));
            }
            files
        };
        let workspaces = self.analysis_workspaces();
        let workspace_path_index = WorkspacePathIndex::new(&workspaces);
        let mut batches = workspaces
            .iter()
            .map(|workspace| AnalysisBatch::new(workspace.compile_opts().clone()))
            .collect::<Vec<_>>();
        let source_map = SourceMap::empty();
        let mut source_files_complete = true;

        for (path, contents, version) in vfs_files {
            if cancellation.is_cancelled() {
                return None;
            }
            let query = workspace_path_index.query(&path);
            let primary = workspace_path_index
                .workspace_idx_for_source_path(self.config.index_policy(), &path)
                .unwrap_or_else(|| query.workspace_idx_for_path());
            let mut indices = query.workspace_idxs_for_import_path();
            let Some(last_idx) = indices.next_back() else {
                batches[primary].push_open_file(path, contents, version);
                continue;
            };
            for idx in indices {
                if idx == primary {
                    batches[idx].push_open_file(path.clone(), contents.clone(), version);
                } else {
                    batches[idx].push_preloaded_file(path.clone(), contents.clone(), version);
                }
            }
            if last_idx == primary {
                batches[last_idx].push_open_file(path, contents, version);
            } else {
                batches[last_idx].push_preloaded_file(path, contents, version);
            }
        }

        for path in disk_paths {
            if cancellation.is_cancelled() {
                return None;
            }
            let Some(idx) = workspace_path_index
                .workspace_idx_for_source_path(self.config.index_policy(), &path)
            else {
                continue;
            };
            if batches[idx].seen_paths.contains(&path) {
                continue;
            }

            match std::fs::symlink_metadata(&path) {
                Ok(metadata) if metadata.is_file() => {
                    if let Ok(contents) = source_map.file_loader().load_file(&path) {
                        batches[idx].push_file(path, contents);
                    } else {
                        source_files_complete = false;
                    }
                }
                Ok(_) => source_files_complete = false,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(_) => source_files_complete = false,
            }
        }

        for (idx, workspace) in workspaces.iter().enumerate() {
            let batch = &mut batches[idx];
            for path in workspace.source_files() {
                if cancellation.is_cancelled() {
                    return None;
                }
                if batch.seen_paths.contains(path) {
                    continue;
                }
                match std::fs::symlink_metadata(path) {
                    Ok(metadata) if metadata.is_file() => {
                        if let Ok(contents) = source_map.file_loader().load_file(path) {
                            batch.push_file(path.clone(), contents);
                        } else {
                            source_files_complete = false;
                        }
                    }
                    Ok(_) | Err(_) => source_files_complete = false,
                }
            }
        }

        for batch in &mut batches {
            batch.finish();
        }
        Some((batches, source_files_complete))
    }

    #[cfg(test)]
    fn publish_analysis(&mut self, version: usize, result: AnalysisResult) -> bool {
        self.publish_analysis_output(
            version,
            AnalysisOutput { result, analysis_paths: AnalysisPathIndex::default() },
        )
    }

    fn publish_analysis_output(&mut self, version: usize, output: AnalysisOutput) -> bool {
        let refresh_code_lenses =
            self.config.supports_code_lens_refresh() && self.config.code_lens_options().is_active();
        let AnalysisOutput { result, analysis_paths } = output;
        let analysis_watched_file_specs = self
            .config
            .supports_watched_file_dynamic_registration()
            .then(|| watched_file_specs(&self.config, &analysis_paths));
        let (old_symbol_tables, refresh_requests) = {
            let analysis_commit = self.analysis_commit.clone();
            let mut commit = analysis_commit.lock();
            if !self.is_current(version) {
                return false;
            }

            // Unknown watcher events are deferred while an analysis is in flight. Recheck them
            // against the path index produced by that analysis before making its results visible.
            let deferred_source_file_events = mem::take(&mut commit.deferred_source_file_events);
            let mut relevant_deferred_events = if deferred_source_file_events.is_empty() {
                Vec::new()
            } else {
                let vfs = self.vfs.read();
                deferred_source_file_events
                    .into_iter()
                    .filter(|(path, typ)| {
                        self.config.tracks_watched_source_file(path)
                            || vfs.exists(&crate::vfs::VfsPath::from(path.clone()))
                            || analysis_paths
                                .includes(path, *typ == FileChangeType::CREATED || path.exists())
                    })
                    .collect::<Vec<_>>()
            };
            if !relevant_deferred_events.is_empty() {
                relevant_deferred_events.sort_by(|(a, _), (b, _)| a.cmp(b));
                drop(commit);
                let _ = self.client.emit(DeferredSourceFileEventsReady {
                    version,
                    events: relevant_deferred_events,
                });
                return false;
            }
            let AnalysisResult { mut analyzed_documents, diagnostics, symbol_tables: new_tables } =
                result;
            let mut index_metrics = self.config.index_metrics();
            index_metrics.resolved = analysis_paths.resolved_dependencies.len();
            index_metrics.unresolved = analysis_paths.existing_unresolved_candidates.len()
                + analysis_paths.missing_candidates.len();
            let vfs = self.vfs.read();
            // A content-identical edit may advance the LSP version without scheduling analysis.
            // Only adopt current versions when the analyzed VFS contents are still current.
            if commit.vfs_content_revision == vfs.content_revision() {
                for (uri, analyzed_version) in &mut analyzed_documents {
                    if let Some(path) = proto::vfs_path(uri)
                        && let Some(open_version) = vfs.get_file_version(&path)
                    {
                        *analyzed_version = Some(i64::from(open_version));
                    }
                }
            }
            let mut symbol_tables = self.symbol_tables.write();
            let inlay_hints_changed = commit.external_refresh.is_some()
                && self.config.supports_inlay_hint_refresh()
                && symbol_tables.inlay_hints_changed(&new_tables);
            let old_symbol_tables = mem::replace(&mut *symbol_tables, new_tables);
            drop(symbol_tables);
            commit.analysis_paths = analysis_paths;
            commit.symbol_tables_version = version;
            commit.analysis_config = Some(self.config.clone());
            commit.natspec_pending_source_changes.clear();
            let update = self
                .diagnostics
                .write()
                .replace_compiler_snapshot_and_publish_batches(diagnostics, analyzed_documents);
            drop(vfs);
            let external_refresh =
                commit.finish_external_refresh(update.pull_reports_changed, inlay_hints_changed);
            let refresh_requests = RefreshRequests {
                diagnostics: external_refresh.diagnostics || update.workspace_documents_changed,
                inlay_hints: external_refresh.inlay_hints,
            };
            publish_diagnostic_batches(&mut self.client, update.batches, &self.config);
            self.published_analysis_version.send_replace(version);
            tracing::info!(
                visited = index_metrics.visited,
                pruned = index_metrics.pruned,
                eager = index_metrics.eager,
                resolved = index_metrics.resolved,
                unresolved = index_metrics.unresolved,
                discovery_duration = ?index_metrics.discovery_duration,
                "published workspace index"
            );
            (old_symbol_tables, refresh_requests)
        };
        let watched_file_registration_update = analysis_watched_file_specs.and_then(|specs| {
            prepare_watched_file_registration_update(
                &self.config,
                &self.watched_file_registration,
                specs,
            )
        });
        let active_registration = watched_file_registration_update.is_none()
            && !self.watched_file_registration.active_registration_ids.lock().is_empty();
        drop(old_symbol_tables);
        spawn_watched_file_registration_update(
            &self.client,
            &self.watched_file_registration,
            watched_file_registration_update,
        );
        if active_registration {
            let _ = self.client.emit(WatchedFileRegistrationReady);
        }
        request_pull_result_refreshes(&self.client, &self.config, refresh_requests);
        if refresh_code_lenses {
            request_code_lens_refresh(&self.client);
        }
        true
    }

    #[cfg(test)]
    fn publish_symbol_tables(&mut self, version: usize, symbol_tables: SymbolTables) -> bool {
        self.publish_analysis(
            version,
            AnalysisResult {
                analyzed_documents: AnalyzedDocuments::default(),
                diagnostics: DiagnosticMap::default(),
                symbol_tables,
            },
        )
    }

    fn analysis_workspaces(&self) -> Cow<'_, [crate::workspace::Workspace]> {
        let workspaces = self.config.workspaces();
        if !workspaces.is_empty() {
            return Cow::Borrowed(workspaces);
        }

        Cow::Owned(vec![crate::workspace::Workspace::unconfigured()])
    }

    #[cfg(test)]
    fn publish_diagnostics(&mut self, owner: DiagnosticOwner, diagnostics: DiagnosticMap) -> bool {
        let analysis_commit = self.analysis_commit.clone();
        let mut commit = analysis_commit.lock();
        let update = {
            let mut store = self.diagnostics.write();
            store.replace_and_publish_batches(owner, diagnostics)
        };

        let refresh_immediately = update.pull_reports_changed && commit.external_refresh.is_none();
        commit.record_external_diagnostics_change(update.pull_reports_changed);
        publish_diagnostic_batches(&mut self.client, update.batches, &self.config);
        refresh_immediately
    }

    fn clear_diagnostic_owners(
        &mut self,
        owners: impl IntoIterator<Item = DiagnosticOwner>,
    ) -> bool {
        let analysis_commit = self.analysis_commit.clone();
        let mut commit = analysis_commit.lock();
        let update = self.diagnostics.write().clear_owners_and_publish_batches(owners);

        let refresh_immediately = update.pull_reports_changed && commit.external_refresh.is_none();
        commit.record_external_diagnostics_change(update.pull_reports_changed);
        publish_diagnostic_batches(&mut self.client, update.batches, &self.config);
        refresh_immediately
    }

    fn publish_flycheck_diagnostics(
        &mut self,
        owner: DiagnosticOwner,
        version: usize,
        diagnostics: DiagnosticMap,
    ) {
        let pull_reports_changed = {
            let analysis_commit = self.analysis_commit.clone();
            let _commit = analysis_commit.lock();
            if !self.is_current_flycheck(&owner, version) {
                return;
            }

            let update = {
                let mut store = self.diagnostics.write();
                store.replace_and_publish_batches(owner, diagnostics)
            };
            let pull_reports_changed = update.pull_reports_changed;
            publish_diagnostic_batches(&mut self.client, update.batches, &self.config);
            pull_reports_changed
        };
        request_pull_result_refreshes(
            &self.client,
            &self.config,
            RefreshRequests { diagnostics: pull_reports_changed, inlay_hints: false },
        );
    }
}

fn request_code_lens_refresh(client: &ClientSocket) {
    let mut client = client.clone();
    let Ok(handle) = tokio::runtime::Handle::try_current() else { return };
    handle.spawn(async move {
        if let Err(error) = client.code_lens_refresh(()).await {
            tracing::debug!(%error, "client does not accept CodeLens refresh");
        }
    });
}

fn request_pull_result_refreshes(
    client: &ClientSocket,
    config: &Config,
    requests: RefreshRequests,
) {
    if requests.diagnostics
        && config.uses_pull_diagnostics()
        && config.supports_diagnostic_refresh()
    {
        request_diagnostic_refresh(client);
    }
    if requests.inlay_hints && config.supports_inlay_hint_refresh() {
        request_inlay_hint_refresh(client);
    }
}

fn request_diagnostic_refresh(client: &ClientSocket) {
    let mut client = client.clone();
    let Ok(handle) = tokio::runtime::Handle::try_current() else { return };
    handle.spawn(async move {
        if let Err(error) = client.workspace_diagnostic_refresh(()).await {
            tracing::debug!(%error, "client does not accept diagnostic refresh");
        }
    });
}

fn request_inlay_hint_refresh(client: &ClientSocket) {
    let mut client = client.clone();
    let Ok(handle) = tokio::runtime::Handle::try_current() else { return };
    handle.spawn(async move {
        if let Err(error) = client.inlay_hint_refresh(()).await {
            tracing::debug!(%error, "client does not accept inlay-hint refresh");
        }
    });
}

struct AnalysisBatch {
    opts: CompileOpts,
    files: Vec<(PathBuf, Arc<String>)>,
    preloaded_files: Vec<(PathBuf, Arc<String>)>,
    preloaded_paths: FxHashSet<PathBuf>,
    open_file_versions: FxHashMap<Url, i64>,
    seen_paths: FxHashSet<PathBuf>,
}

impl AnalysisBatch {
    fn new(opts: CompileOpts) -> Self {
        Self {
            opts,
            files: Vec::new(),
            preloaded_files: Vec::new(),
            preloaded_paths: FxHashSet::default(),
            open_file_versions: FxHashMap::default(),
            seen_paths: FxHashSet::default(),
        }
    }

    #[cfg(any(test, feature = "bench"))]
    fn from_files(opts: CompileOpts, files: impl IntoIterator<Item = (PathBuf, String)>) -> Self {
        let mut batch = Self::new(opts);
        for (path, contents) in files {
            batch.push_file(path, contents);
        }
        batch.finish();
        batch
    }

    fn push_file(&mut self, path: PathBuf, contents: String) {
        if self.seen_paths.contains(&path) {
            return;
        }
        // `SourceFile::new` trims uniquely owned sources after indexing; doing it here can
        // reallocate every disk source twice.
        self.push_shared_file(path, Arc::new(contents));
    }

    fn push_shared_file(&mut self, path: PathBuf, contents: Arc<String>) {
        if self.seen_paths.insert(path.clone()) {
            if self.preloaded_paths.remove(&path) {
                self.preloaded_files.retain(|(preloaded, _)| preloaded != &path);
            }
            self.files.push((path, contents));
        }
    }

    fn push_open_file(&mut self, path: PathBuf, contents: Arc<String>, version: Option<i32>) {
        if let Some(version) = version
            && let Ok(uri) = Url::from_file_path(&path)
        {
            self.open_file_versions.insert(uri, i64::from(version));
        }
        self.push_shared_file(path, contents);
    }

    fn push_preloaded_file(&mut self, path: PathBuf, contents: Arc<String>, version: Option<i32>) {
        if self.seen_paths.contains(&path) || !self.preloaded_paths.insert(path.clone()) {
            return;
        }
        if let Some(version) = version
            && let Ok(uri) = Url::from_file_path(&path)
        {
            self.open_file_versions.insert(uri, i64::from(version));
        }
        self.preloaded_files.push((path, contents));
    }

    fn finish(&mut self) {
        self.files.sort_by(|(lhs, _), (rhs, _)| lhs.cmp(rhs));
        self.preloaded_files.sort_by(|(lhs, _), (rhs, _)| lhs.cmp(rhs));
    }
}

#[cfg(test)]
mod analysis_batch_tests {
    use super::*;
    use crate::{config::negotiate_capabilities, test_support::TestProject};

    #[test]
    fn from_files_tracks_unique_sorted_paths() {
        let a = PathBuf::from("a.sol");
        let b = PathBuf::from("b.sol");
        let batch = AnalysisBatch::from_files(
            CompileOpts::default(),
            [
                (b.clone(), "contract B {}".into()),
                (a.clone(), "contract A {}".into()),
                (b.clone(), "contract Duplicate {}".into()),
            ],
        );

        assert_eq!(batch.files.len(), 2);
        assert_eq!(batch.files[0], (a.clone(), Arc::new("contract A {}".into())));
        assert_eq!(batch.files[1], (b.clone(), Arc::new("contract B {}".into())));
        assert_eq!(batch.seen_paths, FxHashSet::from_iter([a, b]));
    }

    #[test]
    fn flycheck_snapshot_includes_saved_file_and_default_foundry_inputs() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [profile.default]
            src = "src"
            //- /src/Tracked.sol
            contract Tracked {}
            //- /test/Tracked.t.sol
            contract TrackedTest {}
            //- /script/Tracked.s.sol
            contract TrackedScript {}
            "#,
        );
        let mut params = project.initialize_params();
        params.initialization_options = Some(serde_json::json!({
            "flychecks": [{
                "id": "custom",
                "command": "custom-lint"
            }]
        }));
        let (_, mut config) = negotiate_capabilities(params);
        config.rediscover_workspaces();
        let saved_path = project.path("/src/SavedAfterDiscovery.sol");
        project.write_file("/src/SavedAfterDiscovery.sol", "contract SavedAfterDiscovery {}\n");
        let [flycheck] = config.flychecks_for_path(&saved_path).try_into().unwrap();
        let mut state = GlobalState::new(ClientSocket::new_closed());
        state.config = Arc::new(config);

        let paths = state.snapshot().flycheck_source_paths(&flycheck, &saved_path);

        assert!(paths.contains(&project.path("/src/Tracked.sol")));
        assert!(paths.contains(&project.path("/test/Tracked.t.sol")));
        assert!(paths.contains(&project.path("/script/Tracked.s.sol")));
        assert!(paths.contains(&saved_path));
    }

    #[test]
    fn flycheck_snapshot_preserves_nested_workspace_sources() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [profile.default]
            src = "src"
            //- /src/Outer.sol
            contract Outer {}
            //- /nested/foundry.toml
            [profile.default]
            src = "src"
            //- /nested/src/Nested.sol
            contract Nested {}
            "#,
        );
        let mut params = project.initialize_params();
        params.initialization_options = Some(serde_json::json!({
            "flychecks": [{
                "id": "custom",
                "command": "custom-lint"
            }]
        }));
        let (_, mut config) = negotiate_capabilities(params);
        config.rediscover_workspaces();
        let saved_path = project.path("/src/SavedAfterDiscovery.sol");
        project.write_file("/src/SavedAfterDiscovery.sol", "contract SavedAfterDiscovery {}\n");
        let [flycheck] = config.flychecks_for_path(&saved_path).try_into().unwrap();
        let mut state = GlobalState::new(ClientSocket::new_closed());
        state.config = Arc::new(config);

        let paths = state.snapshot().flycheck_source_paths(&flycheck, &saved_path);

        assert!(paths.contains(&project.path("/nested/src/Nested.sol")));
    }

    #[test]
    fn flycheck_snapshot_includes_all_configured_foundry_input_roots() {
        let project = TestProject::new();
        let external_test_root = project.path("/checks");
        project.write_file(
            "/workspace/foundry.toml",
            &format!(
                r#"
            [profile.default]
            src = "contracts"
            test = '{}'
            script = "deployments"
            "#,
                external_test_root.display()
            ),
        );
        project.write_file("/workspace/contracts/Tracked.sol", "contract Tracked {}");
        project.write_file("/checks/Tracked.t.sol", "contract TrackedTest {}");
        project.write_file("/workspace/deployments/Tracked.s.sol", "contract TrackedScript {}");
        let mut params = project.initialize_params();
        params.initialization_options = Some(serde_json::json!({
            "flychecks": [{
                "id": "custom",
                "command": "custom-lint"
            }]
        }));
        let (_, mut config) = negotiate_capabilities(params);
        config.rediscover_workspaces();
        assert_eq!(
            config.workspaces()[0].source_files(),
            &[project.path("/workspace/contracts/Tracked.sol")]
        );
        let saved_path = project.path("/checks/SavedAfterDiscovery.t.sol");
        project
            .write_file("/checks/SavedAfterDiscovery.t.sol", "contract SavedAfterDiscovery {}\n");
        let [flycheck] = config.flychecks_for_path(&saved_path).try_into().unwrap();
        let mut state = GlobalState::new(ClientSocket::new_closed());
        state.config = Arc::new(config);

        let paths = state.snapshot().flycheck_source_paths(&flycheck, &saved_path);

        assert_eq!(
            paths,
            vec![
                project.path("/checks/SavedAfterDiscovery.t.sol"),
                project.path("/checks/Tracked.t.sol"),
                project.path("/workspace/contracts/Tracked.sol"),
                project.path("/workspace/deployments/Tracked.s.sol"),
            ]
        );
    }
}

#[cfg(any(test, feature = "bench"))]
fn analyze(batch: AnalysisBatch) -> AnalysisResult {
    analyze_cancellable(batch, &IndexingCancellation::default())
        .expect("fresh analysis cancellation cannot be cancelled")
        .result
}

#[cfg(any(test, feature = "bench"))]
fn analyze_with_source_map(batch: AnalysisBatch, source_map: Arc<SourceMap>) -> AnalysisResult {
    analyze_cancellable_with_source_map(
        batch,
        source_map,
        ImportPathTracker::default(),
        &IndexingCancellation::default(),
    )
    .expect("fresh analysis cancellation cannot be cancelled")
    .result
}

fn analyze_cancellable(
    batch: AnalysisBatch,
    cancellation: &IndexingCancellation,
) -> Option<AnalysisOutput> {
    let tracker = ImportPathTracker::default();
    let source_map = Arc::new(SourceMap::empty());
    source_map.set_file_loader(TrackingFileLoader { tracker: tracker.clone() });
    analyze_cancellable_with_source_map(batch, source_map, tracker, cancellation)
}

fn analyze_cancellable_with_source_map(
    batch: AnalysisBatch,
    source_map: Arc<SourceMap>,
    import_paths: ImportPathTracker,
    cancellation: &IndexingCancellation,
) -> Option<AnalysisOutput> {
    if cancellation.is_cancelled() {
        return None;
    }
    let (emitter, diag_buffer) = InMemoryEmitter::new();
    let AnalysisBatch {
        mut opts,
        files,
        preloaded_files,
        preloaded_paths: _,
        open_file_versions,
        seen_paths: document_link_sources,
    } = batch;
    debug_assert_eq!(files.len(), document_link_sources.len());
    debug_assert!(files.iter().all(|(path, _)| document_link_sources.contains(path)));
    opts.unstable.recover_incomplete_input = true;
    let sess = Session::builder()
        .opts(opts)
        .source_map(source_map)
        .dcx(DiagCtxt::new(Box::new(emitter)))
        .build();
    // Session construction canonicalizes the base path through the same loader. Only subsequent
    // resolver probes are import candidates.
    import_paths.clear();

    let mut compiler = Compiler::new(sess);
    compiler.enter_mut(move |compiler| {
        let sources_loaded = {
            let mut parsing_context = compiler.parse();
            for (path, contents) in preloaded_files {
                if let Err(error) = parsing_context
                    .sess
                    .source_map()
                    .new_source_file_shared(FileName::real(path), contents)
                {
                    parsing_context.dcx().err(format!("failed to preload source: {error}")).emit();
                }
            }
            let files = files
                .into_iter()
                .map(|(path, contents)| {
                    parsing_context
                        .sess
                        .source_map()
                        .new_source_file_shared(FileName::real(path), contents)
                        .map_err(|error| {
                            parsing_context
                                .dcx()
                                .err(format!("failed to load source: {error}"))
                                .emit()
                        })
                })
                .collect::<solar_interface::Result<Vec<_>>>();

            if let Ok(files) = files {
                parsing_context.add_files(files);
                parsing_context.parse();
                true
            } else {
                false
            }
        };

        if cancellation.is_cancelled() {
            return None;
        }
        if sources_loaded {
            compiler.sources_mut().topo_sort();
            if cancellation.is_cancelled() {
                return None;
            }
            let _ = compiler.lower_asts();
            if cancellation.is_cancelled() {
                return None;
            }
            let _ = compiler.analysis();
            if cancellation.is_cancelled() {
                return None;
            }
        }

        let symbol_tables = SymbolTables::build(compiler.gcx(), &document_link_sources);
        if cancellation.is_cancelled() {
            return None;
        }
        let mut parsed_paths = compiler
            .sources()
            .iter()
            .filter_map(|source| source.file.name.as_real().map(Path::to_path_buf))
            .collect::<FxHashSet<_>>();
        let ImportPathProbes {
            existing: mut existing_unresolved_candidates,
            missing: mut missing_candidates,
        } = import_paths.take_probes();
        existing_unresolved_candidates.retain(|path| !parsed_paths.contains(path));
        missing_candidates.retain(|path| !parsed_paths.contains(path));
        parsed_paths.retain(|path| !document_link_sources.contains(path));
        let analysis_paths = AnalysisPathIndex {
            resolved_dependencies: parsed_paths,
            existing_unresolved_candidates,
            missing_candidates,
        };
        let diagnostics = diag_buffer
            .read()
            .iter()
            .filter_map(|diag| proto::diagnostic(compiler.sess().source_map(), diag))
            .fold(DiagnosticMap::default(), |mut diagnostics, (uri, diag)| {
                diagnostics.entry(uri).or_default().push(diag);
                diagnostics
            });
        let analyzed_documents = compiler
            .sess()
            .source_map()
            .files()
            .iter()
            .filter_map(|file| Url::from_file_path(file.name.as_real()?).ok())
            .map(|uri| {
                let version = open_file_versions.get(&uri).copied();
                (uri, version)
            })
            .collect();

        Some(AnalysisOutput {
            result: AnalysisResult { analyzed_documents, diagnostics, symbol_tables },
            analysis_paths,
        })
    })
}

/// Access to prepared, fully analyzed in-memory projects for benchmarks and tests.
#[cfg(any(test, feature = "bench"))]
#[cfg_attr(all(test, not(feature = "bench")), allow(dead_code, unreachable_pub))]
pub(crate) mod benchmark;
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
