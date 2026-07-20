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
        sync::RwLock,
    },
    diagnostics::{DiagCtxt, InMemoryEmitter},
    source_map::{FileName, SourceMap},
};
use solar_sema::Compiler;
use std::{
    borrow::Cow,
    ops::ControlFlow,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use tokio::{
    sync::{oneshot, watch},
    task::JoinHandle,
};

pub(crate) struct GlobalState {
    client: ClientSocket,
    pub(crate) sess: Session,
    pub(crate) vfs: Arc<RwLock<Vfs>>,
    pub(crate) config: Arc<Config>,
    analysis_version: Arc<AtomicUsize>,
    published_analysis_version: watch::Sender<usize>,
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
    pub(crate) fn recompute(&mut self) {
        self.recompute_with_disk_files(Vec::new());
    }

    pub(crate) fn recompute_with_disk_files(&mut self, disk_paths: Vec<PathBuf>) {
        let version = self.analysis_version.fetch_add(1, Ordering::AcqRel) + 1;
        self.spawn_with_snapshot(move |mut snapshot| {
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

            if snapshot.publish_symbol_tables(version, symbol_tables) {
                snapshot.publish_diagnostics(DiagnosticOwner::Compiler, diagnostics);
            }
        });
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

    #[cfg(test)]
    pub(crate) fn mark_analysis_pending_for_test(&self) {
        self.analysis_version.fetch_add(1, Ordering::AcqRel);
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
                    Ok(diagnostics) => snapshot.publish_diagnostics(task_owner, diagnostics),
                    Err(error) => {
                        tracing::warn!(%id, %error, "flycheck failed");
                        snapshot.publish_diagnostics(task_owner, DiagnosticMap::default());
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

    pub(crate) fn clear_removed_file_diagnostics(
        &mut self,
        paths: impl IntoIterator<Item = PathBuf>,
    ) {
        let paths = paths.into_iter().collect::<Vec<_>>();
        let uris =
            paths.iter().filter_map(|path| Url::from_file_path(path).ok()).collect::<Vec<_>>();
        if uris.is_empty() {
            return;
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

        let batches = {
            let mut store = self.diagnostics.write();
            store.clear_uris_and_publish_batches(uris)
        };

        publish_diagnostic_batches(&mut self.client, batches);
    }

    fn begin_flycheck_epoch(&mut self, owner: &DiagnosticOwner) -> usize {
        let version = {
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
            .map(|workspace| AnalysisBatch {
                opts: workspace.compile_opts().clone(),
                files: Vec::new(),
                seen_paths: FxHashSet::default(),
            })
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

    fn publish_symbol_tables(&mut self, version: usize, symbol_tables: SymbolTables) -> bool {
        // Serialize the version check, table swap, and notification so stale tasks cannot publish
        // last.
        let mut current_tables = self.symbol_tables.write();
        if !self.is_current(version) {
            return false;
        }

        *current_tables = symbol_tables;
        self.published_analysis_version.send_replace(version);
        true
    }

    fn analysis_workspaces(&self) -> Cow<'_, [crate::workspace::Workspace]> {
        let workspaces = self.config.workspaces();
        if !workspaces.is_empty() {
            return Cow::Borrowed(workspaces);
        }

        Cow::Owned(vec![crate::workspace::Workspace::unconfigured()])
    }

    fn publish_diagnostics(&mut self, owner: DiagnosticOwner, diagnostics: DiagnosticMap) {
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
    fn push_file(&mut self, path: PathBuf, contents: String) {
        if self.seen_paths.insert(path.clone()) {
            self.files.push((path, contents));
        }
    }

    fn finish(&mut self) {
        self.files.sort_by(|(lhs, _), (rhs, _)| lhs.cmp(rhs));
    }
}

fn analyze(batch: AnalysisBatch) -> AnalysisResult {
    let (emitter, diag_buffer) = InMemoryEmitter::new();
    let document_link_sources =
        batch.files.iter().map(|(path, _)| path.clone()).collect::<FxHashSet<_>>();
    let mut opts = batch.opts;
    opts.unstable.recover_incomplete_input = true;
    let sess = Session::builder().opts(opts).dcx(DiagCtxt::new(Box::new(emitter))).build();

    let mut compiler = Compiler::new(sess);
    compiler.enter_mut(move |compiler| {
        {
            let mut parsing_context = compiler.parse();
            let files = batch
                .files
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

/// Benchmark-only access to a fully analyzed, in-memory source.
#[cfg(feature = "bench")]
pub(crate) mod benchmark {
    use super::{AnalysisBatch, SymbolTables, analyze};
    use lsp_types::{HoverContents, Position, Url};
    use solar_config::CompileOpts;
    use solar_interface::data_structures::map::FxHashSet;
    use std::path::PathBuf;

    /// An opaque analysis snapshot used by the LSP Criterion benchmarks.
    #[doc(hidden)]
    pub struct BenchmarkAnalysis {
        symbol_tables: SymbolTables,
        uri: Url,
    }

    impl BenchmarkAnalysis {
        /// Analyze one in-memory Solidity source without touching the filesystem.
        pub fn from_source(source: String) -> Self {
            let path =
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches").join("benchmark.sol");
            let uri = Url::from_file_path(&path).expect("benchmark path should be a file URL");
            let result = analyze(AnalysisBatch {
                opts: CompileOpts::default(),
                files: vec![(path, source)],
                seen_paths: FxHashSet::default(),
            });
            Self { symbol_tables: result.symbol_tables, uri }
        }

        /// Resolve one declaration or reference position synchronously.
        #[inline(never)]
        pub fn hover(&self, line: u32, character: u32) -> Option<usize> {
            let hover = std::hint::black_box(
                self.symbol_tables.hover(&self.uri, Position::new(line, character)),
            )?;
            let HoverContents::Markup(content) = hover.contents else { return None };
            Some(content.value.len())
        }
    }
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
