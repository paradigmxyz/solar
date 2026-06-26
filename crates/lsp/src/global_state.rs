use crate::{
    NotifyResult,
    config::{Config, negotiate_capabilities},
    proto,
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
use tokio::task::JoinHandle;

pub(crate) struct GlobalState {
    client: ClientSocket,
    pub(crate) vfs: Arc<RwLock<Vfs>>,
    pub(crate) config: Arc<Config>,
    analysis_version: Arc<AtomicUsize>,
    published_diagnostic_uris: Arc<RwLock<FxHashSet<Url>>>,
}

impl GlobalState {
    pub(crate) fn new(client: ClientSocket) -> Self {
        Self {
            client,
            vfs: Arc::new(Default::default()),
            analysis_version: Arc::new(AtomicUsize::new(0)),
            published_diagnostic_uris: Arc::new(Default::default()),
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

            let mut diagnostics: FxHashMap<Url, Vec<Diagnostic>> = FxHashMap::default();

            for batch in batches {
                if batch.files.is_empty() {
                    continue;
                }

                if !snapshot.is_current(version) {
                    return;
                }

                for (uri, mut batch_diagnostics) in analyze(batch) {
                    diagnostics.entry(uri).or_default().append(&mut batch_diagnostics);
                }

                if !snapshot.is_current(version) {
                    return;
                }
            }

            if snapshot.is_current(version) {
                snapshot.publish_diagnostic_set(diagnostics);
            }
        });
    }

    fn snapshot(&self) -> GlobalStateSnapshot {
        GlobalStateSnapshot {
            client: self.client.clone(),
            vfs: self.vfs.clone(),
            config: self.config.clone(),
            analysis_version: self.analysis_version.clone(),
            published_diagnostic_uris: self.published_diagnostic_uris.clone(),
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
    published_diagnostic_uris: Arc<RwLock<FxHashSet<Url>>>,
}

impl GlobalStateSnapshot {
    fn is_current(&self, version: usize) -> bool {
        self.analysis_version.load(Ordering::Acquire) == version
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

    fn analysis_workspaces(&self) -> Cow<'_, [crate::workspace::Workspace]> {
        let workspaces = self.config.workspaces();
        if !workspaces.is_empty() {
            return Cow::Borrowed(workspaces);
        }

        Cow::Owned(vec![crate::workspace::Workspace::unconfigured()])
    }

    fn publish_diagnostic_set(&mut self, mut diagnostics: FxHashMap<Url, Vec<Diagnostic>>) {
        let mut uris = diagnostics.keys().cloned().collect::<FxHashSet<_>>();

        let mut published_diagnostic_uris = self.published_diagnostic_uris.write();
        uris.extend(published_diagnostic_uris.iter().cloned());
        for uri in uris {
            let uri_diagnostics = diagnostics.remove(&uri).unwrap_or_default();
            if !uri_diagnostics.is_empty() {
                published_diagnostic_uris.insert(uri.clone());
            } else {
                published_diagnostic_uris.remove(&uri);
            }
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

fn analyze(batch: AnalysisBatch) -> FxHashMap<Url, Vec<Diagnostic>> {
    let (emitter, diag_buffer) = InMemoryEmitter::new();
    let sess = Session::builder().opts(batch.opts).dcx(DiagCtxt::new(Box::new(emitter))).build();

    let mut compiler = Compiler::new(sess);
    let _ = compiler.enter_mut(move |compiler| -> solar_interface::Result<_> {
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
                        parsing_context.dcx().err(format!("failed to load source: {error}")).emit()
                    })
            })
            .collect::<solar_interface::Result<Vec<_>>>()?;
        parsing_context.add_files(files);
        parsing_context.parse();

        compiler.sources_mut().topo_sort();
        let _ = compiler.lower_asts();
        let _ = compiler.analysis();

        Ok(())
    });

    compiler.enter(|compiler| {
        diag_buffer
            .read()
            .iter()
            .filter_map(|diag| proto::diagnostic(compiler.sess().source_map(), diag))
            .fold(FxHashMap::<Url, Vec<Diagnostic>>::default(), |mut diagnostics, (uri, diag)| {
                diagnostics.entry(uri).or_default().push(diag);
                diagnostics
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_lsp::ClientSocket;
    use crop::Rope;
    use lsp_types::{Diagnostic, Position, Range, WatchKind, notification::Notification};
    use std::fs;
    use tempfile::TempDir;

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
    fn publish_diagnostic_set_clears_stale_diagnostics() {
        let clean_uri = Url::parse("file:///workspace/src/Clean.sol").unwrap();
        let diagnostic_uri = Url::parse("file:///workspace/src/Error.sol").unwrap();
        let stale_uri = Url::parse("file:///workspace/src/Stale.sol").unwrap();
        let published_diagnostic_uris =
            Arc::new(RwLock::new(FxHashSet::from_iter([clean_uri.clone(), stale_uri.clone()])));
        let mut snapshot = GlobalStateSnapshot {
            client: ClientSocket::new_closed(),
            vfs: Arc::new(Default::default()),
            config: Arc::new(Config::default()),
            analysis_version: Arc::new(AtomicUsize::new(1)),
            published_diagnostic_uris: published_diagnostic_uris.clone(),
        };

        let diagnostic = Diagnostic::new_simple(
            Range {
                start: Position { line: 0, character: 0 },
                end: Position { line: 0, character: 1 },
            },
            "error".into(),
        );
        snapshot.publish_diagnostic_set(FxHashMap::from_iter([(
            diagnostic_uri.clone(),
            vec![diagnostic],
        )]));

        let published = published_diagnostic_uris.read();
        assert!(!published.contains(&clean_uri));
        assert!(published.contains(&diagnostic_uri));
        assert!(!published.contains(&stale_uri));
    }

    #[test]
    fn analysis_batches_read_disk_files() {
        let project = TempDir::new().unwrap();
        let path = project.path().join("Saved.sol");
        std::fs::write(&path, "contract C { function f() public { number+; } }").unwrap();
        let snapshot = GlobalStateSnapshot {
            client: ClientSocket::new_closed(),
            vfs: Arc::new(Default::default()),
            config: Arc::new(Config::default()),
            analysis_version: Arc::new(AtomicUsize::new(1)),
            published_diagnostic_uris: Arc::new(Default::default()),
        };

        let mut batches = snapshot.analysis_batches(vec![path.clone()]);
        let batch = batches.pop().unwrap();

        assert_eq!(
            batch.files,
            vec![(path, "contract C { function f() public { number+; } }".into())]
        );
    }

    #[test]
    fn analysis_batches_scan_workspace_source_roots_and_apply_vfs_overlay() {
        let project = TempDir::new().unwrap();
        let src = project.path().join("src");
        fs::create_dir_all(&src).unwrap();
        let source_path = src.join("A.sol");
        fs::write(&source_path, "contract A {}").unwrap();
        fs::write(src.join("ignored.txt"), "not solidity").unwrap();
        fs::write(
            project.path().join("foundry.toml"),
            r#"
                [profile.default]
                src = "src"
            "#,
        )
        .unwrap();

        let params = InitializeParams {
            workspace_folders: Some(vec![lsp_types::WorkspaceFolder {
                uri: Url::from_file_path(project.path()).unwrap(),
                name: "test".into(),
            }]),
            ..Default::default()
        };
        let (_, mut config) = negotiate_capabilities(params);
        config.rediscover_workspaces();

        let mut vfs = Vfs::default();
        vfs.set_file_contents(
            crate::vfs::VfsPath::from(source_path.clone()),
            Some(Rope::from("contract A { function f() public { number+; } }")),
        );
        let snapshot = GlobalStateSnapshot {
            client: ClientSocket::new_closed(),
            vfs: Arc::new(RwLock::new(vfs)),
            config: Arc::new(config),
            analysis_version: Arc::new(AtomicUsize::new(1)),
            published_diagnostic_uris: Arc::new(Default::default()),
        };

        let mut batches = snapshot.analysis_batches(Vec::new());
        assert_eq!(batches.len(), 1);
        let batch = batches.pop().unwrap();

        assert_eq!(
            batch.files,
            vec![(source_path, "contract A { function f() public { number+; } }".into())]
        );
        assert_eq!(batch.opts.base_path.as_deref(), Some(project.path()));
    }

    #[test]
    fn analysis_batches_use_cached_workspace_source_files() {
        let project = TempDir::new().unwrap();
        let src = project.path().join("src");
        fs::create_dir_all(&src).unwrap();
        let cached_path = src.join("Cached.sol");
        let created_after_discovery = src.join("CreatedAfterDiscovery.sol");
        fs::write(&cached_path, "contract Cached {}").unwrap();
        fs::write(
            project.path().join("foundry.toml"),
            r#"
                [profile.default]
                src = "src"
            "#,
        )
        .unwrap();

        let params = InitializeParams {
            workspace_folders: Some(vec![lsp_types::WorkspaceFolder {
                uri: Url::from_file_path(project.path()).unwrap(),
                name: "test".into(),
            }]),
            ..Default::default()
        };
        let (_, mut config) = negotiate_capabilities(params);
        config.rediscover_workspaces();
        fs::write(&created_after_discovery, "contract CreatedAfterDiscovery {}").unwrap();

        let snapshot = GlobalStateSnapshot {
            client: ClientSocket::new_closed(),
            vfs: Arc::new(Default::default()),
            config: Arc::new(config.clone()),
            analysis_version: Arc::new(AtomicUsize::new(1)),
            published_diagnostic_uris: Arc::new(Default::default()),
        };

        let mut batches = snapshot.analysis_batches(Vec::new());
        let batch = batches.pop().unwrap();
        assert_eq!(batch.files, vec![(cached_path, "contract Cached {}".into())]);

        config.add_source_file(created_after_discovery.clone());
        let outside_source_root = project.path().join("test/Outside.sol");
        fs::create_dir_all(outside_source_root.parent().unwrap()).unwrap();
        fs::write(&outside_source_root, "contract Outside {}").unwrap();
        config.add_source_file(outside_source_root.clone());
        let snapshot = GlobalStateSnapshot {
            client: ClientSocket::new_closed(),
            vfs: Arc::new(Default::default()),
            config: Arc::new(config),
            analysis_version: Arc::new(AtomicUsize::new(1)),
            published_diagnostic_uris: Arc::new(Default::default()),
        };

        let mut batches = snapshot.analysis_batches(Vec::new());
        let batch = batches.pop().unwrap();
        assert!(batch.files.iter().any(|(path, _)| path == &created_after_discovery));
        assert!(!batch.files.iter().any(|(path, _)| path == &outside_source_root));
    }

    #[test]
    fn analysis_batches_assign_cached_source_files_to_most_specific_workspace() {
        let project = TempDir::new().unwrap();
        let nested = project.path().join("nested");
        fs::create_dir(&nested).unwrap();
        let source_path = nested.join("A.sol");
        fs::write(&source_path, "contract A {}").unwrap();

        let params = InitializeParams {
            workspace_folders: Some(vec![
                lsp_types::WorkspaceFolder {
                    uri: Url::from_file_path(project.path()).unwrap(),
                    name: "outer".into(),
                },
                lsp_types::WorkspaceFolder {
                    uri: Url::from_file_path(&nested).unwrap(),
                    name: "inner".into(),
                },
            ]),
            ..Default::default()
        };
        let (_, mut config) = negotiate_capabilities(params);
        config.rediscover_workspaces();
        let snapshot = GlobalStateSnapshot {
            client: ClientSocket::new_closed(),
            vfs: Arc::new(Default::default()),
            config: Arc::new(config),
            analysis_version: Arc::new(AtomicUsize::new(1)),
            published_diagnostic_uris: Arc::new(Default::default()),
        };

        let batches = snapshot.analysis_batches(Vec::new());
        let outer_batch = batches
            .iter()
            .find(|batch| batch.opts.base_path.as_deref() == Some(project.path()))
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
        let project = TempDir::new().unwrap();
        let src = project.path().join("src");
        let lib = project.path().join("lib");
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(&lib).unwrap();
        fs::write(src.join("A.sol"), r#"import "@lib/B.sol"; contract A is B {}"#).unwrap();
        fs::write(lib.join("B.sol"), "contract B {}").unwrap();
        fs::write(
            project.path().join("foundry.toml"),
            r#"
                [profile.default]
                src = "src"
                remappings = ["@lib=lib/"]
            "#,
        )
        .unwrap();

        let params = InitializeParams {
            workspace_folders: Some(vec![lsp_types::WorkspaceFolder {
                uri: Url::from_file_path(project.path()).unwrap(),
                name: "test".into(),
            }]),
            ..Default::default()
        };
        let (_, mut config) = negotiate_capabilities(params);
        config.rediscover_workspaces();
        let snapshot = GlobalStateSnapshot {
            client: ClientSocket::new_closed(),
            vfs: Arc::new(Default::default()),
            config: Arc::new(config),
            analysis_version: Arc::new(AtomicUsize::new(1)),
            published_diagnostic_uris: Arc::new(Default::default()),
        };

        let mut batches = snapshot.analysis_batches(Vec::new());
        assert_eq!(batches.len(), 1);
        let diagnostics = analyze(batches.pop().unwrap());

        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    }

    #[test]
    fn analysis_batches_skip_unreadable_disk_files() {
        let project = TempDir::new().unwrap();
        let path = project.path().join("Missing.sol");
        let snapshot = GlobalStateSnapshot {
            client: ClientSocket::new_closed(),
            vfs: Arc::new(Default::default()),
            config: Arc::new(Config::default()),
            analysis_version: Arc::new(AtomicUsize::new(1)),
            published_diagnostic_uris: Arc::new(Default::default()),
        };

        let mut batches = snapshot.analysis_batches(vec![path]);
        let batch = batches.pop().unwrap();

        assert!(batch.files.is_empty());
    }
}
