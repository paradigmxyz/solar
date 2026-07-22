use crate::{
    NotifyResult,
    config::{Config, negotiate_capabilities},
    diagnostics::{DiagnosticMap, DiagnosticOwner, DiagnosticStore},
    flycheck, proto,
    symbols::SymbolTables,
    vfs::Vfs,
    workspace::WorkspacePathIndex,
};
use async_lsp::{ClientSocket, LanguageClient, ResponseError};
use lsp_types::{
    Diagnostic, DidChangeWatchedFilesRegistrationOptions, FileSystemWatcher, GlobPattern,
    InitializeParams, InitializeResult, InitializedParams, LogMessageParams, MessageType,
    PublishDiagnosticsParams, Registration, RegistrationParams, ServerInfo, Url, WatchKind,
    notification::{DidChangeWatchedFiles, Notification},
};
use solar_config::{CompileOpts, version::SHORT_VERSION};
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
};
use tokio::{
    sync::{oneshot, watch},
    task::{JoinError, JoinHandle},
};

#[derive(Clone, Copy)]
enum AnalysisMode {
    Recompute,
    Rediscover,
    IfInvalidated,
}

/// State serialized with analysis and diagnostic publication.
#[derive(Default)]
struct AnalysisCommitState {
    cache_invalidated: bool,
    /// Last version that actually replaced the symbol tables.
    natspec_symbol_tables_version: usize,
    natspec_pending_source_changes: FxHashSet<PathBuf>,
    natspec_context_change_version: usize,
}

pub(crate) struct GlobalState {
    client: ClientSocket,
    pub(crate) sess: Session,
    pub(crate) vfs: Arc<RwLock<Vfs>>,
    pub(crate) config: Arc<Config>,
    analysis_version: Arc<AtomicUsize>,
    published_analysis_version: watch::Sender<usize>,
    analysis_commit: Arc<Mutex<AnalysisCommitState>>,
    flycheck_versions: Arc<RwLock<FxHashMap<DiagnosticOwner, usize>>>,
    flycheck_cancels: FxHashMap<DiagnosticOwner, oneshot::Sender<()>>,
    pub(crate) symbol_tables: Arc<RwLock<SymbolTables>>,
    diagnostics: Arc<RwLock<DiagnosticStore>>,
}

impl GlobalState {
    pub(crate) fn new(client: ClientSocket) -> Self {
        let (published_analysis_version, _) = watch::channel(0);
        Self {
            client,
            sess: Session::default(),
            vfs: Arc::new(Default::default()),
            analysis_version: Arc::new(AtomicUsize::new(0)),
            published_analysis_version,
            analysis_commit: Arc::new(Default::default()),
            flycheck_versions: Arc::new(Default::default()),
            flycheck_cancels: FxHashMap::default(),
            symbol_tables: Arc::new(Default::default()),
            diagnostics: Arc::new(Default::default()),
            config: Arc::new(Default::default()),
        }
    }

    pub(crate) fn on_initialize(
        &mut self,
        params: InitializeParams,
    ) -> impl Future<Output = Result<InitializeResult, ResponseError>> + use<> {
        let (capabilities, mut config) = negotiate_capabilities(params);

        config.rediscover_workspaces();

        self.config = Arc::new(config);
        std::future::ready(Ok(InitializeResult {
            capabilities,
            server_info: Some(ServerInfo {
                name: "solar".into(),
                version: Some(SHORT_VERSION.into()),
            }),
        }))
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
        self.request_analysis(AnalysisMode::Recompute, disk_paths, Vec::new(), changed_paths);
    }

    pub(crate) fn recompute_after_source_changes(&mut self, changed_paths: Vec<PathBuf>) {
        self.request_analysis(AnalysisMode::Recompute, Vec::new(), Vec::new(), changed_paths);
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
        self.request_analysis(mode, disk_paths, removed_paths, changed_paths);
    }

    pub(crate) fn reindex(&mut self) {
        self.request_analysis(AnalysisMode::Rediscover, Vec::new(), Vec::new(), Vec::new());
    }

    pub(crate) fn reindex_if_invalidated(&mut self) {
        self.request_analysis(AnalysisMode::IfInvalidated, Vec::new(), Vec::new(), Vec::new());
    }

    pub(crate) fn clear_analysis_cache(&mut self) {
        let old_symbol_tables = {
            let analysis_commit = self.analysis_commit.clone();
            let mut commit = analysis_commit.lock();
            let version = self.analysis_version.fetch_add(1, Ordering::AcqRel) + 1;

            let old_symbol_tables = mem::take(&mut *self.symbol_tables.write());
            let batches = self
                .diagnostics
                .write()
                .replace_and_publish_batches(DiagnosticOwner::Compiler, DiagnosticMap::default());
            publish_diagnostic_batches(&mut self.client, batches);

            commit.cache_invalidated = true;
            commit.natspec_symbol_tables_version = version;
            commit.natspec_pending_source_changes.clear();
            commit.natspec_context_change_version = version;
            self.published_analysis_version.send_replace(version);
            old_symbol_tables
        };
        drop(old_symbol_tables);
    }

    #[cfg(test)]
    pub(crate) fn analysis_cache_invalidated(&self) -> bool {
        self.analysis_commit.lock().cache_invalidated
    }

    fn request_analysis(
        &mut self,
        mode: AnalysisMode,
        disk_paths: Vec<PathBuf>,
        removed_paths: Vec<PathBuf>,
        changed_paths: Vec<PathBuf>,
    ) {
        let removed_uris = self.prepare_removed_file_diagnostics(removed_paths);
        let Some(version) = self.begin_analysis(mode, removed_uris, changed_paths) else {
            return;
        };
        let task = self.spawn_with_snapshot(move |mut snapshot| {
            if !snapshot.is_current(version) {
                return;
            }

            let batches = snapshot.analysis_batches(disk_paths);
            if !snapshot.is_current(version) {
                return;
            }

            let mut diagnostics = DiagnosticMap::default();
            let mut symbol_tables = SymbolTables::default();

            for batch in batches {
                if batch.files.is_empty() {
                    continue;
                }

                if !snapshot.is_current(version) {
                    return;
                }

                let result = analyze(batch);
                symbol_tables.extend(result.symbol_tables);
                for (uri, mut batch_diagnostics) in result.diagnostics {
                    diagnostics.entry(uri).or_default().append(&mut batch_diagnostics);
                }

                if !snapshot.is_current(version) {
                    return;
                }
            }

            snapshot.publish_analysis(version, AnalysisResult { diagnostics, symbol_tables });
        });
        self.monitor_analysis_task(version, task);
    }

    fn begin_analysis(
        &mut self,
        mode: AnalysisMode,
        removed_uris: Vec<Url>,
        changed_paths: Vec<PathBuf>,
    ) -> Option<usize> {
        let (version, rediscover) = {
            let analysis_commit = self.analysis_commit.clone();
            let mut commit = analysis_commit.lock();
            if matches!(mode, AnalysisMode::IfInvalidated) && !commit.cache_invalidated {
                return None;
            }

            let invalidated = mem::take(&mut commit.cache_invalidated);
            let rediscover = matches!(mode, AnalysisMode::Rediscover) || invalidated;
            let version = self.begin_analysis_epoch(&mut commit, changed_paths, rediscover);
            let batches = self.diagnostics.write().clear_uris_and_publish_batches(removed_uris);
            publish_diagnostic_batches(&mut self.client, batches);
            (version, rediscover)
        };

        if rediscover {
            self.rediscover_workspaces();
        }
        Some(version)
    }

    fn rediscover_workspaces(&mut self) {
        let removed_owners = Arc::make_mut(&mut self.config).rediscover_workspaces();
        self.clear_removed_flycheck_diagnostics(removed_owners);
    }

    fn begin_analysis_epoch(
        &self,
        commit: &mut AnalysisCommitState,
        changed_paths: Vec<PathBuf>,
        context_changed: bool,
    ) -> usize {
        let version = self.analysis_version.fetch_add(1, Ordering::AcqRel) + 1;
        if context_changed {
            commit.natspec_context_change_version = version;
        }
        commit.natspec_pending_source_changes.extend(changed_paths);
        version
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
        for flycheck in self.config.flychecks_for_path(&path) {
            let owner = flycheck.owner();
            let version = self.begin_flycheck_epoch(&owner);
            let id = flycheck.id.clone();
            let mut snapshot = self.snapshot();
            let (cancel, cancelled) = oneshot::channel();
            let task_owner = owner.clone();
            tokio::spawn(async move {
                let result = flycheck::run(flycheck, cancelled).await;
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
        for owner in owners {
            snapshot.publish_diagnostics(owner, DiagnosticMap::default());
        }
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

    fn spawn_with_snapshot<T: Send + 'static>(
        &self,
        f: impl FnOnce(GlobalStateSnapshot) -> T + Send + 'static,
    ) -> JoinHandle<T> {
        let snapshot = self.snapshot();
        tokio::task::spawn_blocking(move || f(snapshot))
    }

    fn monitor_analysis_task(&self, version: usize, task: JoinHandle<()>) {
        let mut snapshot = self.snapshot();
        tokio::spawn(async move {
            if let Err(error) = task.await {
                snapshot.handle_analysis_failure(version, error);
            }
        });
    }
}

struct AnalysisResult {
    diagnostics: DiagnosticMap,
    symbol_tables: SymbolTables,
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
        let old_symbol_tables = {
            let analysis_commit = self.analysis_commit.clone();
            let mut commit = analysis_commit.lock();
            if !self.is_current(version) {
                return false;
            }

            let old_symbol_tables =
                mem::replace(&mut *self.symbol_tables.write(), result.symbol_tables);
            commit.natspec_symbol_tables_version = version;
            commit.natspec_pending_source_changes.clear();
            let batches = self
                .diagnostics
                .write()
                .replace_and_publish_batches(DiagnosticOwner::Compiler, result.diagnostics);
            publish_diagnostic_batches(&mut self.client, batches);
            self.published_analysis_version.send_replace(version);
            old_symbol_tables
        };
        drop(old_symbol_tables);
        true
    }

    #[cfg(test)]
    fn publish_symbol_tables(&mut self, version: usize, symbol_tables: SymbolTables) -> bool {
        self.publish_analysis(
            version,
            AnalysisResult { diagnostics: DiagnosticMap::default(), symbol_tables },
        )
    }

    fn handle_analysis_failure(&mut self, version: usize, error: JoinError) {
        let analysis_commit = self.analysis_commit.clone();
        let mut commit = analysis_commit.lock();
        if !self.is_current(version) {
            return;
        }

        tracing::warn!(%error, version, "analysis task failed");
        commit.cache_invalidated = true;
        commit.natspec_context_change_version = commit.natspec_context_change_version.max(version);
        self.published_analysis_version.send_replace(version);
    }

    fn analysis_workspaces(&self) -> Cow<'_, [crate::workspace::Workspace]> {
        let workspaces = self.config.workspaces();
        if !workspaces.is_empty() {
            return Cow::Borrowed(workspaces);
        }

        Cow::Owned(vec![crate::workspace::Workspace::unconfigured()])
    }

    fn publish_diagnostics(&mut self, owner: DiagnosticOwner, diagnostics: DiagnosticMap) {
        let analysis_commit = self.analysis_commit.clone();
        let _commit = analysis_commit.lock();
        let batches = {
            let mut store = self.diagnostics.write();
            store.replace_and_publish_batches(owner, diagnostics)
        };

        publish_diagnostic_batches(&mut self.client, batches);
    }

    fn publish_flycheck_diagnostics(
        &mut self,
        owner: DiagnosticOwner,
        version: usize,
        diagnostics: DiagnosticMap,
    ) {
        let analysis_commit = self.analysis_commit.clone();
        let _commit = analysis_commit.lock();
        if !self.is_current_flycheck(&owner, version) {
            return;
        }

        let batches = {
            let mut store = self.diagnostics.write();
            store.replace_and_publish_batches(owner, diagnostics)
        };
        publish_diagnostic_batches(&mut self.client, batches);
    }
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
