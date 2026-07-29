use crate::{
    NotifyResult,
    config::{Config, negotiate_capabilities},
    diagnostics::{DiagnosticMap, DiagnosticOwner, DiagnosticStore, PullReport},
    flycheck,
    progress::{ProgressCoordinator, ProgressTicket},
    proto,
    symbols::{SymbolTables, SymbolTablesAggregator},
    vfs::Vfs,
    workspace::WorkspacePathIndex,
};
use async_lsp::{ClientSocket, LanguageClient, ResponseError};
use lsp_types::{
    Diagnostic, DidChangeWatchedFilesRegistrationOptions, FileSystemWatcher, GlobPattern,
    InitializeParams, InitializedParams, LogMessageParams, MessageType, PublishDiagnosticsParams,
    Registration, RegistrationParams, Url, WatchKind, WorkDoneProgressCancelParams,
    notification::{DidChangeWatchedFiles, Notification},
};
use solar_config::CompileOpts;
use solar_interface::{
    Session,
    data_structures::{
        map::{FxHashMap, FxHashSet},
        sync::{Mutex, RwLock},
    },
    diagnostics::{DiagCtxt, InMemoryEmitter},
    source_map::{FileName, SourceMap},
};
use solar_sema::Compiler;
use std::{
    borrow::Cow,
    mem,
    ops::ControlFlow,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    sync::{Semaphore, oneshot, watch},
    task::{AbortHandle, JoinError, JoinHandle},
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
    changed_paths: Vec<PathBuf>,
}

enum AnalysisTaskOutcome {
    Published,
    Superseded,
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
    external_refresh: Option<PendingExternalRefresh>,
    /// Last version that actually replaced the symbol tables.
    natspec_symbol_tables_version: usize,
    natspec_pending_source_changes: FxHashSet<PathBuf>,
    natspec_context_change_version: usize,
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

impl Default for AnalysisScheduler {
    fn default() -> Self {
        Self { gate: Arc::new(Semaphore::new(1)), tasks: Mutex::new(AnalysisTasks::default()) }
    }
}

#[derive(Default)]
struct AnalysisTasks {
    coordinator: Option<(usize, AbortHandle)>,
    worker: Option<(usize, AbortHandle)>,
}

impl AnalysisTasks {
    fn cancel(&mut self) {
        if let Some((_, worker)) = self.worker.take() {
            worker.abort();
        }
        if let Some((_, coordinator)) = self.coordinator.take() {
            coordinator.abort();
        }
    }
}

pub(crate) struct GlobalState {
    client: ClientSocket,
    pub(crate) sess: Session,
    pub(crate) vfs: Arc<RwLock<Vfs>>,
    pub(crate) config: Arc<Config>,
    analysis_version: Arc<AtomicUsize>,
    published_analysis_version: watch::Sender<usize>,
    analysis_commit: Arc<Mutex<AnalysisCommitState>>,
    analysis_progress: ProgressCoordinator,
    analysis_scheduler: Arc<AnalysisScheduler>,
    flycheck_versions: Arc<RwLock<FxHashMap<DiagnosticOwner, usize>>>,
    flycheck_cancels: FxHashMap<DiagnosticOwner, oneshot::Sender<()>>,
    pub(crate) symbol_tables: Arc<RwLock<SymbolTables>>,
    diagnostics: Arc<RwLock<DiagnosticStore>>,
}

impl GlobalState {
    pub(crate) fn new(client: ClientSocket) -> Self {
        let (published_analysis_version, _) = watch::channel(0);
        let config = Arc::new(Config::default());
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
            analysis_version: Arc::new(AtomicUsize::new(0)),
            published_analysis_version,
            analysis_commit: Arc::new(Default::default()),
            analysis_progress,
            analysis_scheduler: Arc::new(Default::default()),
            flycheck_versions: Arc::new(Default::default()),
            flycheck_cancels: FxHashMap::default(),
            symbol_tables: Arc::new(Default::default()),
            diagnostics: Arc::new(Default::default()),
            config,
        }
    }

    pub(crate) fn on_initialize(
        &mut self,
        params: InitializeParams,
    ) -> impl Future<Output = Result<proto::InitializeResponse, ResponseError>> + use<> {
        let (capabilities, mut config) = negotiate_capabilities(params);

        config.rediscover_workspaces();

        self.analysis_progress.set_enabled(config.supports_work_done_progress());
        self.config = Arc::new(config);
        std::future::ready(Ok(proto::InitializeResponse::new(capabilities)))
    }

    pub(crate) fn on_initialized(&mut self, _: InitializedParams) -> NotifyResult {
        if self.config.supports_watched_file_dynamic_registration() {
            let mut client = self.client.clone();
            tokio::spawn(async move {
                if let Err(error) =
                    client.register_capability(watched_file_registration_params()).await
                {
                    tracing::warn!(%error, "failed to register watched-file notifications");
                }
            });
        }

        let _ = self.client.log_message(LogMessageParams {
            typ: MessageType::INFO,
            message: "solar initialized".into(),
        });
        ControlFlow::Continue(())
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
        self.request_analysis(
            AnalysisMode::Recompute,
            AnalysisRequest { disk_paths, changed_paths, ..Default::default() },
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
        self.request_analysis(
            mode,
            AnalysisRequest { disk_paths, removed_paths, changed_paths },
            AnalysisTrigger::External,
            Duration::ZERO,
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
                let update = diagnostics.write().replace_and_publish_batches(
                    DiagnosticOwner::Compiler,
                    DiagnosticMap::default(),
                );
                let external_refresh = commit
                    .finish_external_refresh(update.pull_reports_changed, inlay_hints_changed);
                let refresh_requests = RefreshRequests {
                    diagnostics: external_refresh.diagnostics || update.pull_reports_changed,
                    inlay_hints: external_refresh.inlay_hints || inlay_hints_changed,
                };
                publish_diagnostic_batches(client, update.batches);

                commit.cache_invalidated = true;
                commit.natspec_symbol_tables_version = version;
                commit.natspec_pending_source_changes.clear();
                commit.natspec_context_change_version = version;
                published_analysis_version.send_replace(version);
                (old_symbol_tables, refresh_requests)
            })
        };
        self.analysis_scheduler.tasks.lock().cancel();
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
        let AnalysisRequest { disk_paths, removed_paths, changed_paths } = request;
        let removed_uris = self.prepare_removed_file_diagnostics(removed_paths);
        let Some((version, progress)) =
            self.begin_analysis(mode, removed_uris, changed_paths, trigger)
        else {
            return;
        };
        self.schedule_analysis(version, disk_paths, progress, delay);
    }

    fn schedule_analysis(
        &self,
        version: usize,
        disk_paths: Vec<PathBuf>,
        progress: ProgressTicket,
        delay: Duration,
    ) {
        let scheduler = self.analysis_scheduler.clone();
        let task_scheduler = scheduler.clone();
        let mut snapshot = self.snapshot();
        let analysis_version = self.analysis_version.clone();
        let published_analysis_version = self.published_analysis_version.clone();
        let analysis_commit = self.analysis_commit.clone();
        let client = self.client.clone();
        let config = self.config.clone();

        let mut tasks = scheduler.tasks.lock();
        tasks.cancel();
        let coordinator = tokio::spawn(async move {
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }

            let Ok(permit) = task_scheduler.gate.clone().acquire_owned().await else {
                return;
            };
            let worker = {
                let mut tasks = task_scheduler.tasks.lock();
                if !snapshot.is_current(version) {
                    return;
                }

                progress.begin();
                let worker_progress = progress.clone();
                let worker = tokio::task::spawn_blocking(move || {
                    let _permit = permit;
                    run_analysis(&mut snapshot, version, disk_paths, &worker_progress)
                });
                tasks.worker = Some((version, worker.abort_handle()));
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
            if tasks.worker.as_ref().is_some_and(|(task_version, _)| *task_version == version) {
                tasks.worker = None;
            }
            if tasks.coordinator.as_ref().is_some_and(|(task_version, _)| *task_version == version)
            {
                tasks.coordinator = None;
            }
        });
        tasks.coordinator = Some((version, coordinator.abort_handle()));
    }

    fn begin_analysis(
        &mut self,
        mode: AnalysisMode,
        removed_uris: Vec<Url>,
        changed_paths: Vec<PathBuf>,
        trigger: AnalysisTrigger,
    ) -> Option<(usize, ProgressTicket)> {
        let (version, rediscover, progress) = {
            let analysis_commit = self.analysis_commit.clone();
            let mut commit = analysis_commit.lock();
            if matches!(mode, AnalysisMode::IfInvalidated) && !commit.cache_invalidated {
                return None;
            }

            let invalidated = mem::take(&mut commit.cache_invalidated);
            let rediscover = matches!(mode, AnalysisMode::Rediscover) || invalidated;
            let version = self.next_analysis_version();
            // Reserve progress before publishing the epoch so a delayed create response cannot end
            // the previous wave after the new analysis becomes current. The progress delay is armed
            // after debounce and scheduler wait complete.
            let progress = self.analysis_progress.reserve(version);
            self.commit_analysis_epoch(&mut commit, version, changed_paths, rediscover, trigger);
            let update = self.diagnostics.write().clear_uris_and_publish_batches(removed_uris);
            commit.record_external_diagnostics_change(update.pull_reports_changed);
            publish_diagnostic_batches(&mut self.client, update.batches);
            (version, rediscover, progress)
        };

        if rediscover {
            self.rediscover_workspaces();
        }
        Some((version, progress))
    }

    fn next_analysis_version(&self) -> usize {
        self.analysis_version.load(Ordering::Relaxed).wrapping_add(1)
    }

    fn rediscover_workspaces(&mut self) {
        let removed_owners = Arc::make_mut(&mut self.config).rediscover_workspaces();
        self.clear_removed_flycheck_diagnostics(removed_owners);
    }

    #[cfg(test)]
    fn begin_analysis_epoch(
        &self,
        commit: &mut AnalysisCommitState,
        changed_paths: Vec<PathBuf>,
        context_changed: bool,
    ) -> usize {
        let version = self.next_analysis_version();
        self.commit_analysis_epoch(
            commit,
            version,
            changed_paths,
            context_changed,
            AnalysisTrigger::Document,
        );
        version
    }

    fn commit_analysis_epoch(
        &self,
        commit: &mut AnalysisCommitState,
        version: usize,
        changed_paths: Vec<PathBuf>,
        context_changed: bool,
        trigger: AnalysisTrigger,
    ) {
        if matches!(trigger, AnalysisTrigger::External) {
            commit.begin_external_refresh();
        }
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
        async move {
            if let Some(latest_analysis) = latest_analysis {
                latest_analysis.await?;
            }
            Ok(diagnostics.read().pull_report(&uri, previous_result_id.as_deref()))
        }
    }

    pub(crate) fn natspec_semantics_are_usable(&self, request_uri: &Url) -> bool {
        let request_path = request_uri.to_file_path().ok();
        let (analysis_version, symbol_tables_version, context_change_version, pending_paths) = {
            let commit = self.analysis_commit.lock();
            (
                self.analysis_version.load(Ordering::Acquire),
                commit.natspec_symbol_tables_version,
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

    pub(crate) fn run_flychecks_on_save(&mut self, path: PathBuf) {
        let timeout = self.config.flycheck_timeout();
        for flycheck in self.config.flychecks_for_path(&path) {
            let owner = flycheck.owner();
            let version = self.begin_flycheck_epoch(&owner);
            let id = flycheck.id.clone();
            let mut snapshot = self.snapshot();
            let (cancel, cancelled) = oneshot::channel();
            let task_owner = owner.clone();
            tokio::spawn(async move {
                let result = flycheck::run(flycheck, timeout, cancelled).await;
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

    fn prepare_removed_file_diagnostics(&mut self, paths: Vec<PathBuf>) -> Vec<Url> {
        let uris =
            paths.iter().filter_map(|path| Url::from_file_path(path).ok()).collect::<Vec<_>>();
        if uris.is_empty() {
            return uris;
        }

        let mut owners = FxHashSet::default();
        for path in paths {
            for flycheck in self.config.flychecks_for_path(&path) {
                owners.insert(flycheck.owner());
            }
        }
        for owner in owners {
            self.begin_flycheck_epoch(&owner);
        }
        uris
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
) -> AnalysisTaskOutcome {
    progress.report("Reading workspace sources");
    if !snapshot.is_current(version) {
        return AnalysisTaskOutcome::Superseded;
    }

    let batches = snapshot.analysis_batches(disk_paths);
    progress.report("Analyzing workspace");
    if !snapshot.is_current(version) {
        return AnalysisTaskOutcome::Superseded;
    }

    let mut results = AnalysisResultAccumulator::default();

    for batch in batches {
        if batch.files.is_empty() {
            continue;
        }

        if !snapshot.is_current(version) {
            return AnalysisTaskOutcome::Superseded;
        }

        results.push(analyze(batch));

        if !snapshot.is_current(version) {
            return AnalysisTaskOutcome::Superseded;
        }
    }

    let result = results.finish();
    progress.report("Publishing workspace index");
    if snapshot.publish_analysis(version, result) {
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
    error: JoinError,
    analysis_version: &Arc<AtomicUsize>,
    published_analysis_version: &watch::Sender<usize>,
    analysis_commit: &Arc<Mutex<AnalysisCommitState>>,
) -> Option<RefreshRequests> {
    let mut commit = analysis_commit.lock();
    if analysis_version.load(Ordering::Acquire) != version {
        return None;
    }

    tracing::warn!(%error, version, "analysis task failed");
    let refresh_requests = commit.fail_external_refresh();
    commit.cache_invalidated = true;
    commit.natspec_context_change_version = commit.natspec_context_change_version.max(version);
    published_analysis_version.send_replace(version);
    Some(refresh_requests)
}

struct AnalysisResult {
    diagnostics: DiagnosticMap,
    symbol_tables: SymbolTables,
}

#[derive(Default)]
struct AnalysisResultAccumulator {
    diagnostics: DiagnosticMap,
    symbol_tables: SymbolTablesAggregator,
}

impl AnalysisResultAccumulator {
    fn push(&mut self, result: AnalysisResult) {
        let AnalysisResult { diagnostics, symbol_tables } = result;
        for (uri, mut batch_diagnostics) in diagnostics {
            self.diagnostics.entry(uri).or_default().append(&mut batch_diagnostics);
        }
        self.symbol_tables.push(symbol_tables);
    }

    fn finish(self) -> AnalysisResult {
        AnalysisResult { diagnostics: self.diagnostics, symbol_tables: self.symbol_tables.finish() }
    }
}

fn watched_file_registration_params() -> RegistrationParams {
    let kind = Some(WatchKind::Create | WatchKind::Change | WatchKind::Delete);
    let options = DidChangeWatchedFilesRegistrationOptions {
        watchers: vec![
            FileSystemWatcher { glob_pattern: GlobPattern::String("**/*.sol".into()), kind },
            FileSystemWatcher { glob_pattern: GlobPattern::String("**/foundry.toml".into()), kind },
        ],
    };

    RegistrationParams {
        registrations: vec![Registration {
            id: "solar-watched-files".into(),
            method: DidChangeWatchedFiles::METHOD.into(),
            register_options: Some(serde_json::to_value(options).unwrap()),
        }],
    }
}

fn publish_diagnostic_batches(
    client: &mut ClientSocket,
    batches: impl IntoIterator<Item = (Url, Vec<Diagnostic>)>,
) {
    for (uri, uri_diagnostics) in batches {
        let _ =
            client.publish_diagnostics(PublishDiagnosticsParams::new(uri, uri_diagnostics, None));
    }
}

pub(crate) struct GlobalStateSnapshot {
    client: ClientSocket,
    vfs: Arc<RwLock<Vfs>>,
    config: Arc<Config>,
    analysis_version: Arc<AtomicUsize>,
    published_analysis_version: watch::Sender<usize>,
    analysis_commit: Arc<Mutex<AnalysisCommitState>>,
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

    fn analysis_batches(&self, disk_paths: Vec<PathBuf>) -> Vec<AnalysisBatch> {
        let vfs_files = self
            .vfs
            .read()
            .iter()
            .filter_map(|(path, contents)| {
                Some((path.as_path()?.to_path_buf(), contents.to_string()))
            })
            .collect::<Vec<_>>();
        let workspaces = self.analysis_workspaces();
        let workspace_path_index = WorkspacePathIndex::new(&workspaces);
        let mut batches = workspaces
            .iter()
            .map(|workspace| AnalysisBatch::new(workspace.compile_opts().clone()))
            .collect::<Vec<_>>();
        let source_map = SourceMap::empty();

        for (path, contents) in vfs_files {
            let idx = workspace_path_index.workspace_idx_for_path(&path);
            batches[idx].push_file(path, contents);
        }

        for path in disk_paths {
            let idx = workspace_path_index.workspace_idx_for_path(&path);
            if !workspaces[idx].tracks_disk_file(&path) {
                continue;
            }
            if batches[idx].seen_paths.contains(&path) {
                continue;
            }

            if let Ok(contents) = source_map.file_loader().load_file(&path) {
                batches[idx].push_file(path, contents);
            }
        }

        for workspace in workspaces.iter() {
            for path in workspace.source_files() {
                let idx = workspace_path_index.workspace_idx_for_path(path);
                let batch = &mut batches[idx];
                if batch.seen_paths.contains(path) {
                    continue;
                }
                if let Ok(contents) = source_map.file_loader().load_file(path) {
                    batch.push_file(path.clone(), contents);
                }
            }
        }

        for batch in &mut batches {
            batch.finish();
        }
        batches
    }

    fn publish_analysis(&mut self, version: usize, result: AnalysisResult) -> bool {
        let refresh_code_lenses =
            self.config.supports_code_lens_refresh() && self.config.code_lens_options().is_active();
        let (old_symbol_tables, refresh_requests) = {
            let analysis_commit = self.analysis_commit.clone();
            let mut commit = analysis_commit.lock();
            if !self.is_current(version) {
                return false;
            }

            let mut symbol_tables = self.symbol_tables.write();
            let inlay_hints_changed = commit.external_refresh.is_some()
                && self.config.supports_inlay_hint_refresh()
                && symbol_tables.inlay_hints_changed(&result.symbol_tables);
            let old_symbol_tables = mem::replace(&mut *symbol_tables, result.symbol_tables);
            drop(symbol_tables);
            commit.natspec_symbol_tables_version = version;
            commit.natspec_pending_source_changes.clear();
            let update = self
                .diagnostics
                .write()
                .replace_and_publish_batches(DiagnosticOwner::Compiler, result.diagnostics);
            let refresh_requests =
                commit.finish_external_refresh(update.pull_reports_changed, inlay_hints_changed);
            publish_diagnostic_batches(&mut self.client, update.batches);
            self.published_analysis_version.send_replace(version);
            (old_symbol_tables, refresh_requests)
        };
        drop(old_symbol_tables);
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
            AnalysisResult { diagnostics: DiagnosticMap::default(), symbol_tables },
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
        publish_diagnostic_batches(&mut self.client, update.batches);
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
        publish_diagnostic_batches(&mut self.client, update.batches);
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
            publish_diagnostic_batches(&mut self.client, update.batches);
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
    if requests.diagnostics && config.supports_diagnostic_refresh() {
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
    files: Vec<(PathBuf, String)>,
    seen_paths: FxHashSet<PathBuf>,
}

impl AnalysisBatch {
    fn new(opts: CompileOpts) -> Self {
        Self { opts, files: Vec::new(), seen_paths: FxHashSet::default() }
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
        if self.seen_paths.insert(path.clone()) {
            self.files.push((path, contents));
        }
    }

    fn finish(&mut self) {
        self.files.sort_by(|(lhs, _), (rhs, _)| lhs.cmp(rhs));
    }
}

#[cfg(test)]
mod analysis_batch_tests {
    use super::*;

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
        assert_eq!(batch.files[0], (a.clone(), "contract A {}".into()));
        assert_eq!(batch.files[1], (b.clone(), "contract B {}".into()));
        assert_eq!(batch.seen_paths, FxHashSet::from_iter([a, b]));
    }
}

fn analyze(batch: AnalysisBatch) -> AnalysisResult {
    analyze_with_source_map(batch, Arc::new(SourceMap::empty()))
}

fn analyze_with_source_map(batch: AnalysisBatch, source_map: Arc<SourceMap>) -> AnalysisResult {
    let (emitter, diag_buffer) = InMemoryEmitter::new();
    let AnalysisBatch { mut opts, files, seen_paths: document_link_sources } = batch;
    debug_assert_eq!(files.len(), document_link_sources.len());
    debug_assert!(files.iter().all(|(path, _)| document_link_sources.contains(path)));
    opts.unstable.recover_incomplete_input = true;
    let sess = Session::builder()
        .opts(opts)
        .source_map(source_map)
        .dcx(DiagCtxt::new(Box::new(emitter)))
        .build();

    let mut compiler = Compiler::new(sess);
    compiler.enter_mut(move |compiler| {
        {
            let mut parsing_context = compiler.parse();
            let files = files
                .into_iter()
                .map(|(path, contents)| {
                    parsing_context
                        .sess
                        .source_map()
                        .new_source_file(FileName::real(path), contents)
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

                compiler.sources_mut().topo_sort();
                let _ = compiler.lower_asts();
                let _ = compiler.analysis();
            }
        }

        let symbol_tables = SymbolTables::build(compiler.gcx(), &document_link_sources);
        let diagnostics = diag_buffer
            .read()
            .iter()
            .filter_map(|diag| proto::diagnostic(compiler.sess().source_map(), diag))
            .fold(DiagnosticMap::default(), |mut diagnostics, (uri, diag)| {
                diagnostics.entry(uri).or_default().push(diag);
                diagnostics
            });

        AnalysisResult { diagnostics, symbol_tables }
    })
}

/// Access to prepared, fully analyzed in-memory projects for benchmarks and tests.
#[cfg(any(test, feature = "bench"))]
#[cfg_attr(all(test, not(feature = "bench")), allow(dead_code, unreachable_pub))]
pub(crate) mod benchmark;
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
