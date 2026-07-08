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
    DidChangeWatchedFilesRegistrationOptions, FileSystemWatcher, GlobPattern, InitializeParams,
    InitializeResult, InitializedParams, LogMessageParams, MessageType, PublishDiagnosticsParams,
    Registration, RegistrationParams, ServerInfo, Url, WatchKind,
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
use tokio::{sync::oneshot, task::JoinHandle};

pub(crate) struct GlobalState {
    client: ClientSocket,
    pub(crate) vfs: Arc<RwLock<Vfs>>,
    pub(crate) config: Arc<Config>,
    analysis_version: Arc<AtomicUsize>,
    flycheck_versions: Arc<RwLock<FxHashMap<DiagnosticOwner, usize>>>,
    flycheck_cancels: FxHashMap<DiagnosticOwner, oneshot::Sender<()>>,
    pub(crate) symbol_tables: Arc<RwLock<SymbolTables>>,
    diagnostics: Arc<RwLock<DiagnosticStore>>,
}

impl GlobalState {
    pub(crate) fn new(client: ClientSocket) -> Self {
        Self {
            client,
            vfs: Arc::new(Default::default()),
            analysis_version: Arc::new(AtomicUsize::new(0)),
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

            if snapshot.is_current(version) {
                snapshot.set_symbol_tables(symbol_tables);
                snapshot.publish_diagnostics(DiagnosticOwner::Compiler, diagnostics);
            }
        });
    }

    pub(crate) fn run_flychecks_on_save(&mut self, path: PathBuf) {
        let flychecks = self.config.flychecks_for_path(&path);
        if flychecks.is_empty() {
            return;
        }

        for flycheck in flychecks {
            let owner = flycheck.owner();
            let version = self.begin_flycheck_epoch(&owner);
            let id = flycheck.id.as_str().to_string();
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

        for (uri, uri_diagnostics) in batches {
            let _ = self.client.publish_diagnostics(PublishDiagnosticsParams::new(
                uri,
                uri_diagnostics,
                None,
            ));
        }
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

pub(crate) struct GlobalStateSnapshot {
    client: ClientSocket,
    vfs: Arc<RwLock<Vfs>>,
    config: Arc<Config>,
    analysis_version: Arc<AtomicUsize>,
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

    fn set_symbol_tables(&mut self, symbol_tables: SymbolTables) {
        *self.symbol_tables.write() = symbol_tables;
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

        for (uri, uri_diagnostics) in batches {
            let _ = self.client.publish_diagnostics(PublishDiagnosticsParams::new(
                uri,
                uri_diagnostics,
                None,
            ));
        }
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
    let mut opts = batch.opts;
    opts.unstable.typeck = true;
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

        let symbol_tables = SymbolTables::build(compiler.gcx());
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

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use crate::test_support::process_exists;
    use crate::{config::negotiate_capabilities, test_support::TestProject};
    use async_lsp::ClientSocket;
    use lsp_types::{
        DocumentSymbol, SymbolKind, WatchKind, WorkspaceSymbol, notification::Notification,
    };
    use std::{path::Path, time::Duration};

    mod completion;
    mod goto_definition;
    mod inlay_hint;
    mod references;
    mod support;

    fn snapshot(project: &TestProject) -> GlobalStateSnapshot {
        snapshot_with_config(project.config(), project.vfs())
    }

    fn snapshot_with_config(config: Config, vfs: Vfs) -> GlobalStateSnapshot {
        GlobalStateSnapshot {
            client: ClientSocket::new_closed(),
            vfs: Arc::new(RwLock::new(vfs)),
            config: Arc::new(config),
            analysis_version: Arc::new(AtomicUsize::new(1)),
            flycheck_versions: Arc::new(Default::default()),
            symbol_tables: Arc::new(Default::default()),
            diagnostics: Arc::new(Default::default()),
        }
    }

    fn flycheck_owner(workspace: impl Into<PathBuf>) -> DiagnosticOwner {
        DiagnosticOwner::Flycheck { id: "slow".into(), workspace: workspace.into() }
    }

    #[test]
    fn watched_file_registration_watches_solidity_and_foundry_manifests() {
        let [registration] = watched_file_registration_params().registrations.try_into().unwrap();
        assert_eq!(registration.id, "solar-watched-files");
        assert_eq!(registration.method, lsp_types::notification::DidChangeWatchedFiles::METHOD);

        assert_eq!(
            registration.register_options,
            Some(serde_json::json!({
                "watchers": [
                    { "globPattern": "**/*.sol", "kind": WatchKind::Create | WatchKind::Change | WatchKind::Delete },
                    { "globPattern": "**/foundry.toml", "kind": WatchKind::Create | WatchKind::Change | WatchKind::Delete },
                ],
            }))
        );
    }

    #[test]
    fn saving_without_matching_flychecks_keeps_previous_flycheck_results_current() {
        let mut state = GlobalState::new(ClientSocket::new_closed());
        let snapshot = state.snapshot();
        let owner = flycheck_owner("/workspace");

        assert!(snapshot.is_current_flycheck(&owner, 0));

        state.run_flychecks_on_save(PathBuf::from("/workspace/Untracked.sol"));

        assert!(snapshot.is_current_flycheck(&owner, 0));
    }

    #[test]
    fn clearing_removed_flychecks_without_owners_keeps_previous_flycheck_results_current() {
        let mut state = GlobalState::new(ClientSocket::new_closed());
        let snapshot = state.snapshot();
        let owner = flycheck_owner("/workspace");

        assert!(snapshot.is_current_flycheck(&owner, 0));

        state.clear_removed_flycheck_diagnostics(Vec::new());

        assert!(snapshot.is_current_flycheck(&owner, 0));
    }

    #[test]
    fn clearing_removed_flychecks_stales_removed_owner_results() {
        let mut state = GlobalState::new(ClientSocket::new_closed());
        let snapshot = state.snapshot();
        let owner = flycheck_owner("/workspace");

        assert!(snapshot.is_current_flycheck(&owner, 0));

        state.clear_removed_flycheck_diagnostics([owner.clone()]);

        assert!(!snapshot.is_current_flycheck(&owner, 0));
    }

    #[test]
    fn clearing_removed_file_diagnostics_stales_matching_flycheck_owner_only() {
        let project = TestProject::from_fixture(
            r#"
            //- /first/foundry.toml
            [profile.default]
            src = "src"
            //- /second/foundry.toml
            [profile.default]
            src = "src"
            "#,
        );
        let mut params = project.initialize_params_with_roots(&["/first", "/second"]);
        params.initialization_options = Some(serde_json::json!({
            "flychecks": [{
                "id": "slow",
                "command": "slow",
            }],
        }));
        let (_, mut config) = negotiate_capabilities(params);
        config.rediscover_workspaces();
        let mut state = GlobalState::new(ClientSocket::new_closed());
        state.config = Arc::new(config);
        let snapshot = state.snapshot();
        let first_owner = flycheck_owner(project.path("/first"));
        let second_owner = flycheck_owner(project.path("/second"));

        assert!(snapshot.is_current_flycheck(&first_owner, 0));
        assert!(snapshot.is_current_flycheck(&second_owner, 0));

        state.clear_removed_file_diagnostics([project.path("/first/src/Deleted.sol")]);

        assert!(!snapshot.is_current_flycheck(&first_owner, 0));
        assert!(snapshot.is_current_flycheck(&second_owner, 0));
    }

    #[test]
    fn beginning_flycheck_epoch_keeps_other_owner_cancel_pending() {
        let mut state = GlobalState::new(ClientSocket::new_closed());
        let first_owner = flycheck_owner("/first");
        let second_owner = flycheck_owner("/second");
        let (cancel, mut cancelled) = oneshot::channel();
        state.flycheck_cancels.insert(first_owner, cancel);

        state.begin_flycheck_epoch(&second_owner);

        assert!(matches!(cancelled.try_recv(), Err(oneshot::error::TryRecvError::Empty)));
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "current_thread")]
    async fn saving_again_cancels_in_flight_flychecks() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [profile.default]
            src = "src"
            //- /src/Test.sol
            contract Test {}
            "#,
        );
        let first_pid_path = project.path("/first-flycheck-pid.txt");
        let second_pid_path = project.path("/second-flycheck-pid.txt");
        let mut params = project.initialize_params();
        params.initialization_options = Some(serde_json::json!({
            "flychecks": [{
                "id": "slow",
                "command": "/bin/sh",
                "args": [
                    "-c",
                    "if [ ! -f \"$1\" ]; then printf '%s' \"$$\" > \"$1\"; exec sleep 120; fi; printf '%s' \"$$\" > \"$2\"; printf '{}\n'",
                    "sh",
                    first_pid_path.display().to_string(),
                    second_pid_path.display().to_string(),
                ],
            }],
        }));
        let (_, mut config) = negotiate_capabilities(params);
        config.rediscover_workspaces();
        let mut state = GlobalState::new(ClientSocket::new_closed());
        state.config = Arc::new(config);

        state.run_flychecks_on_save(project.path("/src/Test.sol"));
        wait_for_path(&first_pid_path).await;
        let first_pid = project.read_file("/first-flycheck-pid.txt").parse().unwrap();

        state.run_flychecks_on_save(project.path("/src/Test.sol"));
        wait_for_path(&second_pid_path).await;
        wait_for_process_exit(first_pid).await;

        assert!(!process_exists(first_pid));
    }

    #[test]
    fn analysis_batches_read_tracked_disk_files() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [profile.default]
            src = "src"

            //- /src/Saved.sol
            contract C { function f() public { number+; } }
            "#,
        );
        let path = project.path("/src/Saved.sol");
        let snapshot = snapshot(&project);

        let mut batches = snapshot.analysis_batches(vec![path.clone()]);
        let batch = batches.pop().unwrap();

        assert_eq!(
            batch.files,
            vec![(path, "contract C { function f() public { number+; } }".into())]
        );
    }

    #[cfg(unix)]
    async fn wait_for_path(path: &Path) {
        for _ in 0..100 {
            if path.exists() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("timed out waiting for {}", path.display());
    }

    #[cfg(unix)]
    async fn wait_for_process_exit(pid: u32) {
        for _ in 0..100 {
            if !process_exists(pid) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    #[test]
    fn analysis_batches_ignore_naked_workspace_disk_files() {
        let project = TestProject::from_fixture(
            r#"
            //- /Disk.sol
            contract Disk {}

            //- /Open.sol open
            contract Open { function f() public { number+; } }
            "#,
        );
        let disk_path = project.path("/Disk.sol");
        let open_path = project.path("/Open.sol");
        let snapshot = snapshot(&project);

        let mut batches = snapshot.analysis_batches(vec![disk_path]);
        let batch = batches.pop().unwrap();

        assert_eq!(
            batch.files,
            vec![(open_path, "contract Open { function f() public { number+; } }".into())]
        );
    }

    #[test]
    fn analysis_batches_scan_workspace_source_roots_and_apply_vfs_overlay() {
        let mut project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [profile.default]
            src = "src"

            //- /src/A.sol
            contract A {}

            //- /src/ignored.txt
            not solidity
            "#,
        );
        project.open_file("/src/A.sol", "contract A { function f() public { number+; } }");
        let source_path = project.path("/src/A.sol");
        let snapshot = snapshot(&project);

        let mut batches = snapshot.analysis_batches(Vec::new());
        assert_eq!(batches.len(), 1);
        let batch = batches.pop().unwrap();

        assert_eq!(
            batch.files,
            vec![(source_path, "contract A { function f() public { number+; } }".into())]
        );
        assert_eq!(batch.opts.base_path.as_deref(), Some(project.root()));
    }

    #[test]
    fn analysis_batches_use_cached_workspace_source_files() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [profile.default]
            src = "src"

            //- /src/Cached.sol
            contract Cached {}
            "#,
        );
        let cached_path = project.path("/src/Cached.sol");
        let created_after_discovery = project.path("/src/CreatedAfterDiscovery.sol");
        let mut config = project.config();
        project.write_file("/src/CreatedAfterDiscovery.sol", "contract CreatedAfterDiscovery {}");

        let snapshot = snapshot_with_config(config.clone(), Vfs::default());

        let mut batches = snapshot.analysis_batches(Vec::new());
        let batch = batches.pop().unwrap();
        assert_eq!(batch.files, vec![(cached_path, "contract Cached {}".into())]);

        config.add_source_file(created_after_discovery.clone());
        let outside_source_root = project.path("/test/Outside.sol");
        project.write_file("/test/Outside.sol", "contract Outside {}");
        config.add_source_file(outside_source_root.clone());
        let snapshot = snapshot_with_config(config, Vfs::default());

        let mut batches = snapshot.analysis_batches(Vec::new());
        let batch = batches.pop().unwrap();
        assert!(batch.files.iter().any(|(path, _)| path == &created_after_discovery));
        assert!(!batch.files.iter().any(|(path, _)| path == &outside_source_root));
    }

    #[test]
    fn analysis_batches_assign_open_files_to_most_specific_workspace() {
        let project = TestProject::from_fixture(
            r#"
            //- /nested/A.sol open
            contract A {}
            "#,
        );
        let source_path = project.path("/nested/A.sol");
        let nested = project.path("/nested");
        let config = project.config_with_roots(&["/", "/nested"]);
        let snapshot = snapshot_with_config(config, project.vfs());

        let batches = snapshot.analysis_batches(Vec::new());
        let outer_batch = batches
            .iter()
            .find(|batch| batch.opts.base_path.as_deref() == Some(project.root()))
            .unwrap();
        let inner_batch = batches
            .iter()
            .find(|batch| batch.opts.base_path.as_deref() == Some(nested.as_path()))
            .unwrap();

        assert!(!outer_batch.files.iter().any(|(path, _)| path == &source_path));
        assert_eq!(inner_batch.files, vec![(source_path, "contract A {}".into())]);
    }

    #[test]
    fn analysis_uses_workspace_remappings_for_import_resolution() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [profile.default]
            src = "src"
            remappings = ["@lib=lib/"]

            //- /src/A.sol
            import "@lib/B.sol"; contract A is B {}

            //- /lib/B.sol
            contract B {}
            "#,
        );
        let snapshot = snapshot(&project);

        let mut batches = snapshot.analysis_batches(Vec::new());
        assert_eq!(batches.len(), 1);
        let result = analyze(batches.pop().unwrap());

        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    }

    #[test]
    fn analysis_resolves_relative_imports_when_cwd_differs_from_workspace_root() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [profile.default]
            src = "src"

            //- /src/A.sol
            import "./B.sol"; contract A is B {}

            //- /src/B.sol
            contract B {}
            "#,
        );
        let snapshot = snapshot(&project);

        let mut batches = snapshot.analysis_batches(Vec::new());
        assert_eq!(batches.len(), 1);
        let result = analyze(batches.pop().unwrap());

        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    }

    #[test]
    fn analysis_uses_foundry_auto_remappings_for_import_resolution() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [profile.default]
            src = "src"

            //- /src/A.sol
            import "forge-std/Test.sol"; contract A is Test {}

            //- /lib/forge-std/src/Test.sol
            contract Test {}
            "#,
        );
        let snapshot = snapshot(&project);

        let mut batches = snapshot.analysis_batches(Vec::new());
        assert_eq!(batches.len(), 1);
        let result = analyze(batches.pop().unwrap());

        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    }

    #[test]
    fn analysis_batches_skip_unreadable_disk_files() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [profile.default]
            src = "src"

            //- /src/.keep
            "#,
        );
        let path = project.path("/src/Missing.sol");
        let snapshot = snapshot(&project);

        let mut batches = snapshot.analysis_batches(vec![path]);
        let batch = batches.pop().unwrap();

        assert!(batch.files.is_empty());
    }

    #[test]
    fn analyze_builds_declaration_symbol_table() {
        let project = TestProject::from_fixture(
            r#"
            //- /Symbols.sol
            uint256 constant TOP = 1;
            contract C {
                uint256 public x;
                uint256 public constant K = 1;
                struct S { uint256 field; }
                struct GetterValue {
                    uint256 visible;
                    uint256 other;
                    mapping(uint256 => uint256) hidden;
                }
                mapping(uint256 key => uint256 value) public getterMap;
                mapping(uint256 key => GetterValue value) public getterValues;
                constructor() {}
                fallback() external {}
                receive() external payable {}
                function f(uint256 y) public returns (uint256 z) {
                    uint256 local = x + y;
                    return local;
                }
            }
            enum E { A }
            "#,
        );
        let path = project.path("/Symbols.sol");
        let uri = Url::from_file_path(&path).unwrap();
        let result = analyze(AnalysisBatch {
            opts: CompileOpts::default(),
            files: vec![(path, project.read_file("/Symbols.sol"))],
            seen_paths: FxHashSet::default(),
        });

        assert!(result.diagnostics.is_empty());

        let declarations = result.symbol_tables.file_declarations(&uri).collect::<Vec<_>>();
        assert_declaration(&declarations, "TOP", SymbolKind::CONSTANT);
        assert_declaration(&declarations, "C", SymbolKind::CLASS);
        assert_declaration(&declarations, "x", SymbolKind::PROPERTY);
        assert_declaration(&declarations, "K", SymbolKind::CONSTANT);
        assert_declaration(&declarations, "S", SymbolKind::STRUCT);
        assert_declaration(&declarations, "field", SymbolKind::PROPERTY);
        assert_declaration(&declarations, "GetterValue", SymbolKind::STRUCT);
        assert_declaration(&declarations, "visible", SymbolKind::PROPERTY);
        assert_declaration(&declarations, "other", SymbolKind::PROPERTY);
        assert_declaration(&declarations, "hidden", SymbolKind::PROPERTY);
        assert_declaration(&declarations, "getterMap", SymbolKind::PROPERTY);
        assert_declaration(&declarations, "getterValues", SymbolKind::PROPERTY);
        assert_declaration(&declarations, "constructor", SymbolKind::CONSTRUCTOR);
        assert_declaration(&declarations, "fallback", SymbolKind::FUNCTION);
        assert_declaration(&declarations, "receive", SymbolKind::FUNCTION);
        assert_declaration(&declarations, "f", SymbolKind::METHOD);
        assert_declaration(&declarations, "y", SymbolKind::VARIABLE);
        assert_declaration(&declarations, "z", SymbolKind::VARIABLE);
        assert_declaration(&declarations, "local", SymbolKind::VARIABLE);
        assert_declaration(&declarations, "E", SymbolKind::ENUM);
        assert_declaration(&declarations, "A", SymbolKind::ENUM_MEMBER);

        assert_parent(&declarations, "x", "C");
        assert_parent(&declarations, "K", "C");
        assert_parent(&declarations, "field", "S");
        assert_parent(&declarations, "visible", "GetterValue");
        assert_parent(&declarations, "other", "GetterValue");
        assert_parent(&declarations, "hidden", "GetterValue");
        assert_parent(&declarations, "getterMap", "C");
        assert_parent(&declarations, "getterValues", "C");
        assert_parent(&declarations, "constructor", "C");
        assert_parent(&declarations, "y", "f");
        assert_parent(&declarations, "z", "f");
        assert_parent(&declarations, "local", "f");
        assert_parent(&declarations, "A", "E");

        assert_declaration_count(&declarations, "x", SymbolKind::PROPERTY, 1);
        assert_declaration_count(&declarations, "visible", SymbolKind::PROPERTY, 1);
        assert_declaration_count(&declarations, "other", SymbolKind::PROPERTY, 1);
        assert_no_declaration(&declarations, "key");
        assert_no_declaration(&declarations, "value");
        assert_no_declaration(&declarations, "__tmp_struct");
        assert_eq!(declarations.len(), result.symbol_tables.declarations().len());
    }

    #[test]
    fn analyze_builds_lsp_symbol_responses() {
        let project = TestProject::from_fixture(
            r#"
            //- /Symbols.sol
            interface I {
                function iface(uint256 value) external;
            }
            library L {
                event Logged(uint256 value);
                function helper(uint256 value) internal pure returns (uint256 result) {
                    return value;
                }
            }
            contract C {
                enum E { A, B }
                struct S { uint256 field; }
                uint256 public x;
                constructor() {}
                function f(uint256 y) public returns (uint256 z) {
                    uint256 local = y;
                    return local;
                }
            }
            "#,
        );
        let path = project.path("/Symbols.sol");
        let uri = Url::from_file_path(&path).unwrap();
        let result = analyze(AnalysisBatch {
            opts: CompileOpts::default(),
            files: vec![(path, project.read_file("/Symbols.sol"))],
            seen_paths: FxHashSet::default(),
        });

        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);

        let document_symbols = result.symbol_tables.document_symbols(&uri);
        assert_eq!(
            document_symbols.iter().map(|symbol| symbol.name.as_str()).collect::<Vec<_>>(),
            ["I", "L", "C"]
        );
        assert_eq!(document_symbols[0].kind, SymbolKind::INTERFACE);
        assert_eq!(document_symbols[1].kind, SymbolKind::MODULE);
        assert_eq!(document_symbols[2].kind, SymbolKind::CLASS);

        let contract = find_document_symbol(&document_symbols, "C");
        assert_eq!(child_names(contract), ["E", "S", "x", "constructor", "f"]);

        let enumm = find_document_child(contract, "E");
        assert_eq!(enumm.kind, SymbolKind::ENUM);
        assert_eq!(child_names(enumm), ["A", "B"]);

        let function = find_document_child(contract, "f");
        assert_eq!(function.kind, SymbolKind::METHOD);
        assert_eq!(child_names(function), ["y", "z", "local"]);

        let workspace_symbols = result.symbol_tables.workspace_symbols("helper");
        assert_eq!(
            workspace_symbols.iter().map(|symbol| symbol.name.as_str()).collect::<Vec<_>>(),
            ["helper"]
        );
        assert_eq!(workspace_symbols[0].kind, SymbolKind::METHOD);
        assert_eq!(workspace_symbols[0].container_name.as_deref(), Some("L"));

        let all_workspace_symbols = result.symbol_tables.workspace_symbols("");
        assert_eq!(find_workspace_symbol(&all_workspace_symbols, "I").kind, SymbolKind::INTERFACE);
        assert_eq!(find_workspace_symbol(&all_workspace_symbols, "L").kind, SymbolKind::MODULE);
        assert_eq!(find_workspace_symbol(&all_workspace_symbols, "C").kind, SymbolKind::CLASS);
    }

    fn assert_parent(
        declarations: &[&crate::symbols::DeclarationSymbol],
        name: &str,
        parent: &str,
    ) {
        let declaration = find_declaration(declarations, name);
        let parent_id = declaration.parent.unwrap_or_else(|| {
            panic!("declaration `{name}` has no parent in {declarations:#?}");
        });
        let parent_declaration = declarations
            .iter()
            .find(|candidate| candidate.id == parent_id)
            .unwrap_or_else(|| panic!("parent {parent_id:?} for `{name}` not found"));
        assert_eq!(parent_declaration.name, parent);
    }

    fn assert_declaration(
        declarations: &[&crate::symbols::DeclarationSymbol],
        name: &str,
        kind: SymbolKind,
    ) {
        assert!(
            declarations.iter().any(|symbol| symbol.name == name && symbol.kind == kind),
            "missing {kind:?} declaration `{name}` in {declarations:#?}"
        );
    }

    fn assert_declaration_count(
        declarations: &[&crate::symbols::DeclarationSymbol],
        name: &str,
        kind: SymbolKind,
        expected: usize,
    ) {
        assert_eq!(
            declarations.iter().filter(|symbol| symbol.name == name && symbol.kind == kind).count(),
            expected,
            "unexpected count for {kind:?} declaration `{name}` in {declarations:#?}",
        );
    }

    fn assert_no_declaration(declarations: &[&crate::symbols::DeclarationSymbol], name: &str) {
        assert!(
            declarations.iter().all(|symbol| symbol.name != name),
            "unexpected declaration `{name}` in {declarations:#?}",
        );
    }

    fn find_declaration<'a>(
        declarations: &'a [&crate::symbols::DeclarationSymbol],
        name: &str,
    ) -> &'a crate::symbols::DeclarationSymbol {
        declarations
            .iter()
            .copied()
            .find(|symbol| symbol.name == name)
            .unwrap_or_else(|| panic!("missing declaration `{name}` in {declarations:#?}"))
    }

    fn find_document_symbol<'a>(symbols: &'a [DocumentSymbol], name: &str) -> &'a DocumentSymbol {
        symbols
            .iter()
            .find(|symbol| symbol.name == name)
            .unwrap_or_else(|| panic!("missing document symbol `{name}` in {symbols:#?}"))
    }

    fn find_document_child<'a>(symbol: &'a DocumentSymbol, child_name: &str) -> &'a DocumentSymbol {
        let children = symbol.children.as_deref().unwrap_or_else(|| {
            panic!("document symbol `{}` has no children", symbol.name);
        });
        find_document_symbol(children, child_name)
    }

    fn child_names(symbol: &DocumentSymbol) -> Vec<&str> {
        symbol
            .children
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|child| child.name.as_str())
            .collect()
    }

    fn find_workspace_symbol<'a>(
        symbols: &'a [WorkspaceSymbol],
        name: &str,
    ) -> &'a WorkspaceSymbol {
        symbols
            .iter()
            .find(|symbol| symbol.name == name)
            .unwrap_or_else(|| panic!("missing workspace symbol `{name}` in {symbols:#?}"))
    }
}
