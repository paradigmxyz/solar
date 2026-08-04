use super::*;
#[cfg(unix)]
use crate::test_support::process_exists;
use crate::{
    config::negotiate_capabilities,
    test_support::{MarkedProject, TestProject},
};
use async_lsp::{ClientSocket, ErrorCode, ResponseError, ServerSocket, router::Router};
use lsp_types::{
    CodeLensWorkspaceClientCapabilities, CreateFilesParams, DeleteFilesParams, Diagnostic,
    DiagnosticWorkspaceClientCapabilities, DidChangeConfigurationParams,
    DidChangeTextDocumentParams, DidChangeWatchedFilesClientCapabilities,
    DidChangeWatchedFilesParams, DidChangeWorkspaceFoldersParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DidSaveTextDocumentParams, DocumentDiagnosticParams,
    DocumentDiagnosticReport, DocumentDiagnosticReportResult, DocumentSymbol, FileChangeType,
    FileCreate, FileDelete, FileEvent, InlayHintWorkspaceClientCapabilities, PartialResultParams,
    Position, ProgressParams, ProgressParamsValue, PublishDiagnosticsParams, Range,
    RegistrationParams, SymbolKind, TextDocumentContentChangeEvent, TextDocumentIdentifier,
    TextDocumentItem, UnregistrationParams, VersionedTextDocumentIdentifier, WatchKind,
    WorkDoneProgress, WorkDoneProgressCreateParams, WorkDoneProgressParams,
    WorkspaceClientCapabilities, WorkspaceFolder, WorkspaceFoldersChangeEvent, WorkspaceSymbol,
    notification, notification::Notification, request,
};
use std::{
    future::Future,
    path::Path,
    sync::{Barrier, mpsc as std_mpsc},
    task::{Context, Poll, Waker},
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, oneshot};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

mod call_hierarchy;
mod code_lens;
mod completion;
mod completion_resolve;
mod document_highlight;
mod document_link;
mod file_operations;
mod folding_range;
mod goto_definition;
mod hover;
mod implementation;
mod inlay_hint;
#[path = "protocol_trace.rs"]
mod protocol_trace_tests;
mod references;
mod refresh;
mod rename;
mod selection_range;
mod signature_help;
mod support;
mod type_definition;
mod type_hierarchy;
mod workspace_diagnostic;

const ASYNC_TEST_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
enum WorkDoneEvent {
    Create(WorkDoneProgressCreateParams),
    Progress(ProgressParams),
    Diagnostics(PublishDiagnosticsParams),
}

#[derive(Debug)]
enum WatchedFileClientEvent {
    Register(RegistrationParams),
    Unregister(UnregistrationParams),
}

async fn next_watched_file_client_event(
    events: &mut mpsc::UnboundedReceiver<WatchedFileClientEvent>,
) -> WatchedFileClientEvent {
    tokio::time::timeout(ASYNC_TEST_TIMEOUT, events.recv())
        .await
        .expect("watched-file client event should arrive")
        .expect("watched-file client event channel should stay open")
}

fn watched_file_registration_has_spec(
    params: &RegistrationParams,
    base: &Path,
    pattern: &str,
) -> bool {
    let base_uri = Url::from_file_path(base).unwrap().to_string();
    params.registrations.iter().any(|registration| {
        registration.register_options.as_ref().is_some_and(|options| {
            options["watchers"].as_array().is_some_and(|watchers| {
                watchers.iter().any(|watcher| {
                    watcher["globPattern"]["baseUri"].as_str() == Some(&base_uri)
                        && watcher["globPattern"]["pattern"].as_str() == Some(pattern)
                })
            })
        })
    })
}

struct WorkDoneClientState {
    events: mpsc::UnboundedSender<WorkDoneEvent>,
    create_ack: Option<oneshot::Receiver<()>>,
}

struct WorkDoneHarness {
    client: ClientSocket,
    server: ServerSocket,
    events: mpsc::UnboundedReceiver<WorkDoneEvent>,
    create_ack: Option<oneshot::Sender<()>>,
    server_task: tokio::task::JoinHandle<async_lsp::Result<()>>,
    client_task: tokio::task::JoinHandle<async_lsp::Result<()>>,
}

impl WorkDoneHarness {
    async fn next_event(&mut self) -> WorkDoneEvent {
        tokio::time::timeout(ASYNC_TEST_TIMEOUT, self.events.recv())
            .await
            .expect("work-done client event should arrive")
            .expect("work-done client event channel should stay open")
    }

    fn acknowledge_create(&mut self) {
        self.create_ack.take().expect("one create acknowledgement").send(()).unwrap();
    }

    async fn probe(&self) {
        self.client.request::<request::Shutdown>(()).await.unwrap();
    }

    async fn shutdown(self) {
        self.server.notify::<notification::Exit>(()).unwrap();
        assert!(self.server_task.await.unwrap().is_ok());
        assert!(matches!(self.client_task.await.unwrap(), Err(async_lsp::Error::Eof)));
    }
}

fn work_done_harness() -> WorkDoneHarness {
    let (server_main, client) = async_lsp::MainLoop::new_server(|_| {
        let mut router = Router::new(());
        router.notification::<notification::Exit>(|_, ()| ControlFlow::Break(Ok(())));
        router
    });
    let (events_tx, events) = mpsc::unbounded_channel();
    let (create_ack_tx, create_ack_rx) = oneshot::channel();
    let (client_main, server) = async_lsp::MainLoop::new_client(move |_| {
        let mut router =
            Router::new(WorkDoneClientState { events: events_tx, create_ack: Some(create_ack_rx) });
        router.request::<request::WorkDoneProgressCreate, _>(|state, params| {
            state.events.send(WorkDoneEvent::Create(params)).unwrap();
            let create_ack = state.create_ack.take().expect("one progress create request");
            async move {
                create_ack.await.map_err(|_| {
                    ResponseError::new(ErrorCode::REQUEST_FAILED, "test create ack dropped")
                })?;
                Ok(())
            }
        });
        router.request::<request::Shutdown, _>(|_, ()| async { Ok(()) });
        router.notification::<notification::Progress>(|state, params| {
            state.events.send(WorkDoneEvent::Progress(params)).unwrap();
            ControlFlow::Continue(())
        });
        router.notification::<notification::PublishDiagnostics>(|state, params| {
            state.events.send(WorkDoneEvent::Diagnostics(params)).unwrap();
            ControlFlow::Continue(())
        });
        router
    });

    let (server_stream, client_stream) = tokio::io::duplex(64 << 10);
    let (server_rx, server_tx) = tokio::io::split(server_stream);
    let server_task =
        tokio::spawn(server_main.run_buffered(server_rx.compat(), server_tx.compat_write()));
    let (client_rx, client_tx) = tokio::io::split(client_stream);
    let client_task =
        tokio::spawn(client_main.run_buffered(client_rx.compat(), client_tx.compat_write()));

    WorkDoneHarness {
        client,
        server,
        events,
        create_ack: Some(create_ack_tx),
        server_task,
        client_task,
    }
}

fn snapshot(project: &TestProject) -> GlobalStateSnapshot {
    snapshot_with_config(project.config(), project.vfs())
}

fn config_with_indexing_excludes(project: &TestProject, excludes: &[&str]) -> Config {
    let mut params = project.initialize_params();
    params.initialization_options = Some(serde_json::json!({
        "indexing": { "exclude": excludes }
    }));
    let (_, mut config) = negotiate_capabilities(params);
    config.rediscover_workspaces();
    config
}

fn snapshot_with_config(config: Config, vfs: Vfs) -> GlobalStateSnapshot {
    let (published_analysis_version, _) = watch::channel(1);
    GlobalStateSnapshot {
        client: ClientSocket::new_closed(),
        vfs: Arc::new(RwLock::new(vfs)),
        config: Arc::new(config),
        analysis_version: Arc::new(AtomicUsize::new(1)),
        published_analysis_version,
        analysis_commit: Arc::new(Default::default()),
        watched_file_registration: Arc::new(Default::default()),
        flycheck_versions: Arc::new(Default::default()),
        symbol_tables: Arc::new(Default::default()),
        diagnostics: Arc::new(Default::default()),
    }
}

#[test]
fn analysis_result_accumulator_finishes_empty() {
    let result = AnalysisResultAccumulator::default().finish();

    assert!(result.diagnostics.is_empty());
    assert!(result.symbol_tables.workspace_symbols("").is_empty());
}

#[test]
fn analysis_result_accumulator_preserves_single_batch_indexes() {
    let mut batch = analyze(AnalysisBatch::from_files(
        CompileOpts::default(),
        [(std::env::temp_dir().join("One.sol"), "contract One {}".into())],
    ));
    let uri = diagnostic_uri();
    batch.diagnostics.insert(uri.clone(), vec![diagnostic("one")]);
    let mut results = AnalysisResultAccumulator::default();

    results.push(batch);
    let result = results.finish();

    assert_eq!(
        result.diagnostics[&uri]
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect::<Vec<_>>(),
        ["one"]
    );
    assert_eq!(
        result
            .symbol_tables
            .workspace_symbols("")
            .into_iter()
            .map(|symbol| symbol.name)
            .collect::<Vec<_>>(),
        vec!["One".to_owned()]
    );
}

#[test]
fn analysis_result_accumulator_merges_multiple_batches() {
    let one_path = std::env::temp_dir().join("One.sol");
    let two_path = std::env::temp_dir().join("Two.sol");
    let one_uri = Url::from_file_path(&one_path).unwrap();
    let two_uri = Url::from_file_path(&two_path).unwrap();
    let mut first = analyze(AnalysisBatch::from_files(
        CompileOpts::default(),
        [(one_path, "contract One {}".into())],
    ));
    let mut second = analyze(AnalysisBatch::from_files(
        CompileOpts::default(),
        [(two_path, "contract Two {}".into())],
    ));
    second.analyzed_documents.insert(one_uri.clone(), Some(7));
    let uri = diagnostic_uri();
    first.diagnostics.insert(uri.clone(), vec![diagnostic("first")]);
    second.diagnostics.insert(uri.clone(), vec![diagnostic("second")]);
    let mut results = AnalysisResultAccumulator::default();

    results.push(first);
    results.push(second);
    let result = results.finish();

    assert_eq!(
        result.diagnostics[&uri]
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect::<Vec<_>>(),
        ["first", "second"]
    );
    let mut names = result
        .symbol_tables
        .workspace_symbols("")
        .into_iter()
        .map(|symbol| symbol.name)
        .collect::<Vec<_>>();
    names.sort_unstable();
    assert_eq!(names, vec!["One".to_owned(), "Two".to_owned()]);
    assert_eq!(
        result.analyzed_documents,
        AnalyzedDocuments::from_iter([(one_uri, Some(7)), (two_uri, None)])
    );
}

#[test]
fn analysis_collects_open_versions_and_loaded_dependencies() {
    let project = TestProject::from_fixture(
        r#"
        //- /Main.sol open
        import "./Dependency.sol";
        contract Main is Dependency {}

        //- /Dependency.sol
        contract Dependency {}
        "#,
    );
    let main_path = project.path("/Main.sol");
    let main_uri = Url::from_file_path(&main_path).unwrap();
    let dependency_uri = Url::from_file_path(project.path("/Dependency.sol")).unwrap();
    let mut batches =
        snapshot_with_config(Config::default(), project.vfs()).analysis_batches(Vec::new());
    let batch = batches.pop().unwrap();
    assert!(batches.is_empty());

    let result = analyze(batch);

    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    assert_eq!(
        result.analyzed_documents,
        AnalyzedDocuments::from_iter([(main_uri, Some(0)), (dependency_uri, None)])
    );
}

#[test]
fn analysis_tracks_excluded_transitive_dependencies_and_normalized_missing_candidates() {
    let project = TestProject::from_fixture(
        r#"
        //- /Main.sol
        import "./generated/B.sol";
        contract Main is B {}

        //- /generated/B.sol
        import "./nested/../C.sol";
        contract B is C {}

        //- /generated/C.sol
        import "./missing/../Missing.sol";
        contract C {}

        //- /generated/Unrelated.sol
        contract Unrelated {}
        "#,
    );
    let config = config_with_indexing_excludes(&project, &["generated/**"]);
    let mut batches = snapshot_with_config(config, project.vfs()).analysis_batches(Vec::new());
    let batch = batches.pop().unwrap();
    assert!(batches.is_empty());
    assert_eq!(batch.seen_paths, FxHashSet::from_iter([project.path("/Main.sol")]));

    let output = analyze_cancellable(batch, &IndexingCancellation::default()).unwrap();

    assert_eq!(
        output.analysis_paths.resolved_dependencies,
        FxHashSet::from_iter([project.path("/generated/B.sol"), project.path("/generated/C.sol")])
    );
    assert_eq!(
        output.analysis_paths.missing_candidates,
        FxHashSet::from_iter([project.path("/generated/Missing.sol")])
    );
    assert!(output.analysis_paths.existing_unresolved_candidates.is_empty());
    assert!(
        output.result.symbol_tables.workspace_symbols("B").iter().any(|symbol| symbol.name == "B")
    );
    assert!(
        output.result.symbol_tables.workspace_symbols("C").iter().any(|symbol| symbol.name == "C")
    );
    assert!(output.result.symbol_tables.workspace_symbols("Unrelated").is_empty());
}

#[test]
fn analysis_output_accumulator_resolved_path_wins_across_batches() {
    let path = PathBuf::from("Dependency.sol");
    let result = || AnalysisResult {
        analyzed_documents: AnalyzedDocuments::default(),
        diagnostics: DiagnosticMap::default(),
        symbol_tables: SymbolTables::default(),
    };
    let mut accumulator = AnalysisOutputAccumulator::default();
    accumulator.push(AnalysisOutput {
        result: result(),
        analysis_paths: AnalysisPathIndex {
            missing_candidates: FxHashSet::from_iter([path.clone()]),
            ..Default::default()
        },
    });
    accumulator.push(AnalysisOutput {
        result: result(),
        analysis_paths: AnalysisPathIndex {
            resolved_dependencies: FxHashSet::from_iter([path.clone()]),
            ..Default::default()
        },
    });

    let output = accumulator.finish();

    assert_eq!(output.analysis_paths.resolved_dependencies, FxHashSet::from_iter([path]));
    assert!(output.analysis_paths.existing_unresolved_candidates.is_empty());
    assert!(output.analysis_paths.missing_candidates.is_empty());
}

#[test]
fn stale_analysis_does_not_replace_published_path_index() {
    let current_path = PathBuf::from("Current.sol");
    let stale_path = PathBuf::from("Stale.sol");
    let output = |path| AnalysisOutput {
        result: AnalysisResult {
            analyzed_documents: AnalyzedDocuments::default(),
            diagnostics: DiagnosticMap::default(),
            symbol_tables: SymbolTables::default(),
        },
        analysis_paths: AnalysisPathIndex {
            resolved_dependencies: FxHashSet::from_iter([path]),
            ..Default::default()
        },
    };
    let state = GlobalState::new(ClientSocket::new_closed());
    assert!(state.snapshot().publish_analysis_output(0, output(current_path.clone())));
    let mut stale_snapshot = state.snapshot();
    state.mark_analysis_pending_for_test();

    assert!(!stale_snapshot.publish_analysis_output(0, output(stale_path)));
    assert_eq!(
        state.analysis_commit.lock().analysis_paths.resolved_dependencies,
        FxHashSet::from_iter([current_path])
    );
}

#[test]
fn deferred_dependency_change_prevents_stale_analysis_publish() {
    let state = GlobalState::new(ClientSocket::new_closed());
    state.mark_analysis_pending_for_test();
    let version = state.analysis_version.load(Ordering::Acquire);
    let path = PathBuf::from("Dependency.sol");
    assert_eq!(
        state.classify_source_file_event(&path, FileChangeType::CHANGED),
        SourceFileEventDisposition::Deferred
    );
    let output = AnalysisOutput {
        result: AnalysisResult {
            analyzed_documents: AnalyzedDocuments::default(),
            diagnostics: DiagnosticMap::default(),
            symbol_tables: SymbolTables::default(),
        },
        analysis_paths: AnalysisPathIndex {
            resolved_dependencies: FxHashSet::from_iter([path]),
            ..Default::default()
        },
    };

    assert!(!state.snapshot().publish_analysis_output(version, output));
    assert_eq!(*state.published_analysis_version.borrow(), 0);
    assert!(state.analysis_commit.lock().deferred_source_file_events.is_empty());
}

#[test]
fn source_event_classification_observes_path_index_committed_before_its_lock() {
    let state = GlobalState::new(ClientSocket::new_closed());
    state.mark_analysis_pending_for_test();
    let version = state.analysis_version.load(Ordering::Acquire);
    let path = PathBuf::from("Dependency.sol");
    let mut commit = state.analysis_commit.lock();

    std::thread::scope(|scope| {
        let (started_tx, started_rx) = std_mpsc::sync_channel(1);
        let (result_tx, result_rx) = std_mpsc::sync_channel(1);
        let event_state = &state;
        let event_path = path.clone();
        scope.spawn(move || {
            started_tx.send(()).unwrap();
            result_tx
                .send(event_state.classify_source_file_event(&event_path, FileChangeType::CHANGED))
                .unwrap();
        });
        started_rx.recv_timeout(ASYNC_TEST_TIMEOUT).unwrap();

        commit.analysis_paths.resolved_dependencies.insert(path.clone());
        state.published_analysis_version.send_replace(version);
        drop(commit);

        assert_eq!(
            result_rx.recv_timeout(ASYNC_TEST_TIMEOUT).unwrap(),
            SourceFileEventDisposition::Relevant
        );
    });
}

#[tokio::test(flavor = "current_thread")]
async fn watched_file_specs_are_committed_before_the_next_analysis_epoch() {
    let project = TestProject::new();
    std::fs::create_dir(project.path("/workspace")).unwrap();
    let mut params = project.initialize_params_with_roots(&["/workspace"]);
    params.capabilities.workspace = Some(WorkspaceClientCapabilities {
        did_change_watched_files: Some(DidChangeWatchedFilesClientCapabilities {
            dynamic_registration: Some(true),
            relative_pattern_support: Some(true),
        }),
        ..Default::default()
    });
    let (_, config) = negotiate_capabilities(params);
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(config);
    state.mark_analysis_pending_for_test();
    let version = state.analysis_version.load(Ordering::Acquire);
    let mut snapshot = state.snapshot();
    let output = AnalysisOutput {
        result: AnalysisResult {
            analyzed_documents: AnalyzedDocuments::default(),
            diagnostics: DiagnosticMap::default(),
            symbol_tables: SymbolTables::default(),
        },
        analysis_paths: AnalysisPathIndex {
            resolved_dependencies: FxHashSet::from_iter([project.path("/outside/Dependency.sol")]),
            ..Default::default()
        },
    };
    let desired_specs = state.watched_file_registration.desired_specs.lock();
    let runtime = tokio::runtime::Handle::current();

    std::thread::scope(|scope| {
        let publisher = scope.spawn(move || {
            let _runtime = runtime.enter();
            snapshot.publish_analysis_output(version, output)
        });

        let deadline = Instant::now() + ASYNC_TEST_TIMEOUT;
        while *state.published_analysis_version.borrow() != version && Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert_eq!(*state.published_analysis_version.borrow(), version);

        let deadline = Instant::now() + Duration::from_millis(100);
        let mut commit_became_available = false;
        while Instant::now() < deadline {
            if state.analysis_commit.try_lock().is_some() {
                commit_became_available = true;
                break;
            }
            std::thread::yield_now();
        }

        drop(desired_specs);
        assert!(publisher.join().unwrap());
        assert!(
            !commit_became_available,
            "analysis commit unlocked before its watched-file specs were queued"
        );
    });
}

#[tokio::test(flavor = "current_thread")]
async fn workspace_folder_change_advances_epoch_before_watcher_reregistration() {
    let project = TestProject::new();
    let old_root = project.path("/old");
    let new_root = project.path("/new");
    std::fs::create_dir(&old_root).unwrap();
    std::fs::create_dir(&new_root).unwrap();
    let mut params = project.initialize_params_with_roots(&["/old"]);
    params.capabilities.workspace = Some(WorkspaceClientCapabilities {
        did_change_watched_files: Some(DidChangeWatchedFilesClientCapabilities {
            dynamic_registration: Some(true),
            relative_pattern_support: Some(true),
        }),
        ..Default::default()
    });
    let (_, config) = negotiate_capabilities(params);
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(config);
    let initial_version = state.analysis_version.load(Ordering::Acquire);
    let registration = state.watched_file_registration.clone();
    let desired_specs = registration.desired_specs.lock();
    let analysis_version = state.analysis_version.clone();
    let runtime = tokio::runtime::Handle::current();
    let worker = std::thread::spawn(move || {
        let _runtime = runtime.enter();
        let result = crate::handlers::did_change_workspace_folders(
            &mut state,
            DidChangeWorkspaceFoldersParams {
                event: WorkspaceFoldersChangeEvent {
                    added: vec![WorkspaceFolder {
                        uri: Url::from_file_path(new_root).unwrap(),
                        name: "new".into(),
                    }],
                    removed: vec![WorkspaceFolder {
                        uri: Url::from_file_path(old_root).unwrap(),
                        name: "old".into(),
                    }],
                },
            },
        );
        assert!(matches!(result, ControlFlow::Continue(())));
        state
    });

    let deadline = Instant::now() + Duration::from_millis(100);
    while analysis_version.load(Ordering::Acquire) == initial_version && Instant::now() < deadline {
        std::thread::yield_now();
    }
    let advanced_before_reregistration =
        analysis_version.load(Ordering::Acquire) != initial_version;

    drop(desired_specs);
    let state = worker.join().unwrap();
    state.analysis_scheduler.tasks.lock().cancel();
    assert!(
        advanced_before_reregistration,
        "workspace-folder change queued watchers before invalidating the old analysis epoch"
    );
}

#[test]
fn deferred_existing_missing_candidate_change_prevents_stale_analysis_publish() {
    let project = TestProject::new();
    let path = project.path("/Missing.sol");
    project.write_file("/Missing.sol", "contract Missing {}");
    let state = GlobalState::new(ClientSocket::new_closed());
    state.mark_analysis_pending_for_test();
    let version = state.analysis_version.load(Ordering::Acquire);
    assert_eq!(
        state.classify_source_file_event(&path, FileChangeType::CHANGED),
        SourceFileEventDisposition::Deferred
    );
    let output = AnalysisOutput {
        result: AnalysisResult {
            analyzed_documents: AnalyzedDocuments::default(),
            diagnostics: DiagnosticMap::default(),
            symbol_tables: SymbolTables::default(),
        },
        analysis_paths: AnalysisPathIndex {
            missing_candidates: FxHashSet::from_iter([path]),
            ..Default::default()
        },
    };

    assert!(!state.snapshot().publish_analysis_output(version, output));
    assert_eq!(*state.published_analysis_version.borrow(), 0);
}

#[test]
fn deferred_unrelated_change_does_not_block_analysis_publish() {
    let state = GlobalState::new(ClientSocket::new_closed());
    state.mark_analysis_pending_for_test();
    let version = state.analysis_version.load(Ordering::Acquire);
    assert_eq!(
        state.classify_source_file_event(Path::new("Unrelated.sol"), FileChangeType::CHANGED),
        SourceFileEventDisposition::Deferred
    );
    let output = AnalysisOutput {
        result: AnalysisResult {
            analyzed_documents: AnalyzedDocuments::default(),
            diagnostics: DiagnosticMap::default(),
            symbol_tables: SymbolTables::default(),
        },
        analysis_paths: AnalysisPathIndex::default(),
    };

    assert!(state.snapshot().publish_analysis_output(version, output));
    assert_eq!(*state.published_analysis_version.borrow(), version);
    assert!(state.analysis_commit.lock().deferred_source_file_events.is_empty());
}

#[test]
fn clearing_analysis_cache_clears_published_path_index() {
    let mut state = GlobalState::new(ClientSocket::new_closed());
    {
        let mut commit = state.analysis_commit.lock();
        commit.discovery_pending = true;
        commit.analysis_paths = AnalysisPathIndex {
            resolved_dependencies: FxHashSet::from_iter([PathBuf::from("Dependency.sol")]),
            existing_unresolved_candidates: FxHashSet::from_iter([PathBuf::from("Unreadable.sol")]),
            missing_candidates: FxHashSet::from_iter([PathBuf::from("Missing.sol")]),
        };
        commit
            .deferred_source_file_events
            .insert(PathBuf::from("Pending.sol"), FileChangeType::CHANGED);
    }

    state.clear_analysis_cache();

    let commit = state.analysis_commit.lock();
    assert!(commit.cache_invalidated);
    assert!(!commit.discovery_pending);
    assert!(commit.analysis_paths.resolved_dependencies.is_empty());
    assert!(commit.analysis_paths.existing_unresolved_candidates.is_empty());
    assert!(commit.analysis_paths.missing_candidates.is_empty());
    assert!(commit.deferred_source_file_events.is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn clearing_analysis_cache_rejects_stale_deferred_event_replay() {
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.mark_analysis_pending_for_test();
    let stale_version = state.analysis_version.load(Ordering::Acquire);
    state.clear_analysis_cache();
    let cleared_version = state.analysis_version.load(Ordering::Acquire);

    assert!(matches!(
        state.on_deferred_source_file_events_ready(DeferredSourceFileEventsReady {
            version: stale_version,
            events: vec![(PathBuf::from("Dependency.sol"), FileChangeType::CHANGED)],
        }),
        ControlFlow::Continue(())
    ));

    assert_eq!(state.analysis_version.load(Ordering::Acquire), cleared_version);
    assert_eq!(*state.published_analysis_version.borrow(), cleared_version);
    let commit = state.analysis_commit.lock();
    assert!(commit.cache_invalidated);
    assert!(!commit.discovery_pending);
    assert!(commit.deferred_source_file_events.is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn unknown_dependency_event_after_cache_clear_starts_recovery() {
    let project = TestProject::from_fixture(
        r#"
        //- /Main.sol
        import "./generated/Dependency.sol";
        contract Main is Dependency {}

        //- /generated/Dependency.sol
        contract Dependency {}
        "#,
    );
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(config_with_indexing_excludes(&project, &["generated/**"]));
    state.clear_analysis_cache();
    let cleared_version = state.analysis_version.load(Ordering::Acquire);
    project.write_file("/generated/Dependency.sol", "contract Dependency {} contract Latest {}");
    let path = project.path("/generated/Dependency.sol");

    assert!(matches!(
        crate::handlers::did_change_watched_files(
            &mut state,
            DidChangeWatchedFilesParams {
                changes: vec![FileEvent {
                    uri: Url::from_file_path(path).unwrap(),
                    typ: FileChangeType::CHANGED,
                }],
            },
        ),
        ControlFlow::Continue(())
    ));

    assert_eq!(state.analysis_version.load(Ordering::Acquire), cleared_version + 1);
    let tables = tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
        .await
        .expect("cache recovery analysis should finish")
        .unwrap();
    assert!(!state.analysis_cache_invalidated());
    assert!(tables.read().workspace_symbols("Latest").iter().any(|symbol| symbol.name == "Latest"));
}

#[tokio::test(flavor = "current_thread")]
async fn discovery_cleanup_does_not_remove_analysis_handles_for_the_same_epoch() {
    let version = 1;
    let discovery = AnalysisTaskKey { version, stage: AnalysisTaskStage::Discovery };
    let analysis = AnalysisTaskKey { version, stage: AnalysisTaskStage::Analysis };
    let coordinator = tokio::spawn(std::future::pending::<()>());
    let worker = tokio::spawn(std::future::pending::<()>());
    let mut tasks = AnalysisTasks {
        coordinator: Some((analysis, coordinator.abort_handle())),
        worker: Some((analysis, worker.abort_handle())),
        cancellation: None,
    };

    tasks.clear_worker(discovery);
    tasks.clear_coordinator(discovery);

    assert!(tasks.worker.as_ref().is_some_and(|(key, _)| *key == analysis));
    assert!(tasks.coordinator.as_ref().is_some_and(|(key, _)| *key == analysis));
    tasks.cancel();
    let _ = coordinator.await;
    let _ = worker.await;
}

#[tokio::test(flavor = "current_thread")]
async fn workspace_discovery_router_rejects_stale_and_cancelled_ready_events() {
    struct DiscoveryStateProbe(oneshot::Sender<(Vec<PathBuf>, bool)>);

    let project = TestProject::from_fixture(
        r#"
        //- /stale/Stale.sol
        contract Stale {}

        //- /latest/Latest.sol
        contract Latest {}
        "#,
    );
    let discovery_result = |root| {
        let (_, config) = negotiate_capabilities(project.initialize_params_with_roots(&[root]));
        config.discover_workspaces(&IndexingCancellation::default()).unwrap()
    };
    let stale_result = discovery_result("/stale");
    let cancelled_result = discovery_result("/stale");
    let latest_result = discovery_result("/latest");
    let (_, latest_config) =
        negotiate_capabilities(project.initialize_params_with_roots(&["/latest"]));

    let (setup_tx, setup_rx) = std_mpsc::sync_channel(1);
    let (server_main, internal_client) = async_lsp::MainLoop::new_server(move |client| {
        let mut state = GlobalState::new(client);
        state.config = Arc::new(latest_config);
        let (stale_version, stale_progress) = state
            .begin_analysis(
                AnalysisMode::Rediscover,
                Vec::new(),
                Vec::new(),
                AnalysisTrigger::External,
            )
            .unwrap();
        let (latest_version, latest_progress) = state
            .begin_analysis(
                AnalysisMode::Rediscover,
                Vec::new(),
                Vec::new(),
                AnalysisTrigger::External,
            )
            .unwrap();
        setup_tx
            .send((
                stale_version,
                stale_progress,
                latest_version,
                latest_progress,
                state.published_analysis_version.subscribe(),
                state.symbol_tables.clone(),
            ))
            .unwrap();

        let mut router = crate::new_router_with_state(state);
        router.event::<DiscoveryStateProbe>(|state, probe| {
            let roots = state
                .config
                .workspaces()
                .iter()
                .filter_map(|workspace| workspace.compile_opts().base_path.clone())
                .collect();
            let pending = state.analysis_commit.lock().discovery_pending;
            probe.0.send((roots, pending)).unwrap();
            ControlFlow::Continue(())
        });
        router
    });
    let (client_main, server) = async_lsp::MainLoop::new_client(|_| {
        let mut router = Router::new(());
        router.notification::<notification::PublishDiagnostics>(|_, _| ControlFlow::Continue(()));
        router.notification::<notification::LogMessage>(|_, _| ControlFlow::Continue(()));
        router
    });
    let (stale_version, stale_progress, latest_version, latest_progress, mut published, tables) =
        setup_rx.recv().unwrap();

    let (server_stream, client_stream) = tokio::io::duplex(64 << 10);
    let (server_rx, server_tx) = tokio::io::split(server_stream);
    let server_task =
        tokio::spawn(server_main.run_buffered(server_rx.compat(), server_tx.compat_write()));
    let (client_rx, client_tx) = tokio::io::split(client_stream);
    let client_task =
        tokio::spawn(client_main.run_buffered(client_rx.compat(), client_tx.compat_write()));

    internal_client
        .emit(WorkspaceDiscoveryReady {
            version: stale_version,
            result: stale_result,
            disk_paths: Vec::new(),
            progress: stale_progress,
            cancellation: IndexingCancellation::default(),
        })
        .unwrap();
    let (probe_tx, probe_rx) = oneshot::channel();
    internal_client.emit(DiscoveryStateProbe(probe_tx)).unwrap();
    let (roots, pending) = probe_rx.await.unwrap();
    assert!(roots.is_empty());
    assert!(pending);

    let cancelled = IndexingCancellation::default();
    cancelled.cancel();
    internal_client
        .emit(WorkspaceDiscoveryReady {
            version: latest_version,
            result: cancelled_result,
            disk_paths: Vec::new(),
            progress: latest_progress.clone(),
            cancellation: cancelled,
        })
        .unwrap();
    let (probe_tx, probe_rx) = oneshot::channel();
    internal_client.emit(DiscoveryStateProbe(probe_tx)).unwrap();
    let (roots, pending) = probe_rx.await.unwrap();
    assert!(roots.is_empty());
    assert!(pending);

    internal_client
        .emit(WorkspaceDiscoveryReady {
            version: latest_version,
            result: latest_result,
            disk_paths: Vec::new(),
            progress: latest_progress,
            cancellation: IndexingCancellation::default(),
        })
        .unwrap();
    tokio::time::timeout(ASYNC_TEST_TIMEOUT, async {
        while *published.borrow() != latest_version {
            published.changed().await.unwrap();
        }
    })
    .await
    .expect("latest workspace discovery should publish analysis");

    assert!(tables.read().workspace_symbols("Stale").is_empty());
    assert!(tables.read().workspace_symbols("Latest").iter().any(|symbol| symbol.name == "Latest"));

    server.request::<request::Shutdown>(()).await.unwrap();
    server.notify::<notification::Exit>(()).unwrap();
    assert!(server_task.await.unwrap().is_ok());
    assert!(matches!(client_task.await.unwrap(), Err(async_lsp::Error::Eof)));
}

#[tokio::test(flavor = "current_thread")]
async fn deferred_dependency_change_router_publishes_replacement_analysis() {
    struct PublishAnalysis {
        version: usize,
        output: AnalysisOutput,
    }

    let project = TestProject::from_fixture(
        r#"
        //- /Main.sol
        import "./generated/Dependency.sol";
        contract Main is Dependency {}

        //- /generated/Dependency.sol
        contract Dependency {}
        "#,
    );
    let config = config_with_indexing_excludes(&project, &["generated/**"]);
    let mut batches =
        snapshot_with_config(config.clone(), project.vfs()).analysis_batches(Vec::new());
    let old_output =
        analyze_cancellable(batches.pop().unwrap(), &IndexingCancellation::default()).unwrap();
    let dependency = project.path("/generated/Dependency.sol");
    project.write_file("/generated/Dependency.sol", "contract Dependency {} contract Latest {}");

    let (setup_tx, setup_rx) = std_mpsc::sync_channel(1);
    let (server_main, internal_client) = async_lsp::MainLoop::new_server(move |client| {
        let mut state = GlobalState::new(client);
        state.config = Arc::new(config);
        state.mark_analysis_pending_for_test();
        let version = state.analysis_version.load(Ordering::Acquire);
        assert_eq!(
            state.classify_source_file_event(&dependency, FileChangeType::CHANGED),
            SourceFileEventDisposition::Deferred
        );
        setup_tx
            .send((
                version,
                state.published_analysis_version.subscribe(),
                state.symbol_tables.clone(),
            ))
            .unwrap();

        let mut router = crate::new_router_with_state(state);
        router.event::<PublishAnalysis>(|state, event| {
            assert!(!state.snapshot().publish_analysis_output(event.version, event.output));
            ControlFlow::Continue(())
        });
        router
    });
    let (client_main, server) = async_lsp::MainLoop::new_client(|_| {
        let mut router = Router::new(());
        router.notification::<notification::PublishDiagnostics>(|_, _| ControlFlow::Continue(()));
        router.notification::<notification::LogMessage>(|_, _| ControlFlow::Continue(()));
        router
    });
    let (version, mut published, tables) = setup_rx.recv().unwrap();

    let (server_stream, client_stream) = tokio::io::duplex(64 << 10);
    let (server_rx, server_tx) = tokio::io::split(server_stream);
    let server_task =
        tokio::spawn(server_main.run_buffered(server_rx.compat(), server_tx.compat_write()));
    let (client_rx, client_tx) = tokio::io::split(client_stream);
    let client_task =
        tokio::spawn(client_main.run_buffered(client_rx.compat(), client_tx.compat_write()));

    internal_client.emit(PublishAnalysis { version, output: old_output }).unwrap();
    tokio::time::timeout(ASYNC_TEST_TIMEOUT, async {
        while *published.borrow() <= version {
            published.changed().await.unwrap();
        }
    })
    .await
    .expect("replacement dependency analysis should publish");

    assert_eq!(*published.borrow(), version + 1);
    assert!(tables.read().workspace_symbols("Latest").iter().any(|symbol| symbol.name == "Latest"));

    server.request::<request::Shutdown>(()).await.unwrap();
    server.notify::<notification::Exit>(()).unwrap();
    assert!(server_task.await.unwrap().is_ok());
    assert!(matches!(client_task.await.unwrap(), Err(async_lsp::Error::Eof)));
}

#[tokio::test(flavor = "current_thread")]
async fn analysis_updates_refresh_code_lenses_only_when_active() {
    let (server_main, client) = async_lsp::MainLoop::new_server(|_| {
        let mut router = Router::new(());
        router.notification::<notification::Exit>(|_, ()| ControlFlow::Break(Ok(())));
        router
    });
    let (refresh_tx, mut refresh_rx) = mpsc::unbounded_channel();
    let (client_main, server) = async_lsp::MainLoop::new_client(move |_| {
        let mut router = Router::new(refresh_tx);
        router.request::<request::CodeLensRefresh, _>(|state, ()| {
            state.send(()).unwrap();
            async { Ok(()) }
        });
        router
    });
    let (server_stream, client_stream) = tokio::io::duplex(64 << 10);
    let (server_rx, server_tx) = tokio::io::split(server_stream);
    let server_task =
        tokio::spawn(server_main.run_buffered(server_rx.compat(), server_tx.compat_write()));
    let (client_rx, client_tx) = tokio::io::split(client_stream);
    let client_task =
        tokio::spawn(client_main.run_buffered(client_rx.compat(), client_tx.compat_write()));

    let mut params = InitializeParams::default();
    params.capabilities.workspace = Some(lsp_types::WorkspaceClientCapabilities {
        code_lens: Some(CodeLensWorkspaceClientCapabilities { refresh_support: Some(true) }),
        ..Default::default()
    });
    let (_, config) = negotiate_capabilities(params);
    let mut state = GlobalState::new(client);
    state.config = Arc::new(config);

    assert!(state.snapshot().publish_analysis(
        0,
        AnalysisResult {
            analyzed_documents: AnalyzedDocuments::default(),
            diagnostics: DiagnosticMap::default(),
            symbol_tables: SymbolTables::default(),
        },
    ));
    tokio::time::timeout(ASYNC_TEST_TIMEOUT, refresh_rx.recv())
        .await
        .expect("CodeLens refresh should arrive")
        .expect("CodeLens refresh channel should stay open");

    state.clear_analysis_cache();
    tokio::time::timeout(ASYNC_TEST_TIMEOUT, refresh_rx.recv())
        .await
        .expect("CodeLens refresh should arrive after clearing analysis")
        .expect("CodeLens refresh channel should stay open");

    let mut params = InitializeParams::default();
    params.capabilities.workspace = Some(lsp_types::WorkspaceClientCapabilities {
        code_lens: Some(CodeLensWorkspaceClientCapabilities { refresh_support: Some(true) }),
        ..Default::default()
    });
    params.initialization_options = Some(serde_json::json!({
        "codeLens": { "enable": false }
    }));
    let (_, config) = negotiate_capabilities(params);
    state.config = Arc::new(config);

    assert!(state.snapshot().publish_analysis(
        1,
        AnalysisResult {
            analyzed_documents: AnalyzedDocuments::default(),
            diagnostics: DiagnosticMap::default(),
            symbol_tables: SymbolTables::default(),
        },
    ));
    state.clear_analysis_cache();
    assert!(
        tokio::time::timeout(Duration::from_millis(100), refresh_rx.recv()).await.is_err(),
        "inactive CodeLens should not request refresh"
    );

    server.notify::<notification::Exit>(()).unwrap();
    assert!(server_task.await.unwrap().is_ok());
    assert!(matches!(client_task.await.unwrap(), Err(async_lsp::Error::Eof)));
}

#[test]
fn document_diagnostic_returns_merged_full_and_unchanged_reports() {
    let uri = diagnostic_uri();
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.snapshot().publish_diagnostics(
        DiagnosticOwner::Compiler,
        DiagnosticMap::from_iter([(uri.clone(), vec![diagnostic("compiler")])]),
    );
    state.snapshot().publish_diagnostics(
        flycheck_owner("/workspace"),
        DiagnosticMap::from_iter([(uri.clone(), vec![diagnostic("lint")])]),
    );

    let response = expect_ready(crate::handlers::document_diagnostic(
        &mut state,
        document_diagnostic_params(uri.clone(), None),
    ));
    let DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(report)) =
        response.unwrap()
    else {
        panic!("first diagnostic pull should return a full report");
    };
    assert_eq!(report.related_documents, None);
    assert_eq!(
        report
            .full_document_diagnostic_report
            .items
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect::<Vec<_>>(),
        ["compiler", "lint"]
    );
    let result_id = report.full_document_diagnostic_report.result_id.unwrap();

    let response = expect_ready(crate::handlers::document_diagnostic(
        &mut state,
        document_diagnostic_params(uri, Some(result_id.clone())),
    ));
    let DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Unchanged(report)) =
        response.unwrap()
    else {
        panic!("unchanged diagnostic pull should return an unchanged report");
    };
    assert_eq!(report.related_documents, None);
    assert_eq!(report.unchanged_document_diagnostic_report.result_id, result_id);
}

#[test]
fn document_diagnostic_waits_for_committed_analysis_diagnostics() {
    let uri = diagnostic_uri();
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.mark_analysis_pending_for_test();

    let mut request = std::pin::pin!(crate::handlers::document_diagnostic(
        &mut state,
        document_diagnostic_params(uri.clone(), None),
    ));
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    assert!(request.as_mut().poll(&mut context).is_pending());

    let mut snapshot = state.snapshot();
    assert!(snapshot.publish_analysis(
        1,
        AnalysisResult {
            analyzed_documents: AnalyzedDocuments::default(),
            diagnostics: DiagnosticMap::from_iter([(uri, vec![diagnostic("current")])]),
            symbol_tables: SymbolTables::default(),
        },
    ));

    let Poll::Ready(response) = request.as_mut().poll(&mut context) else {
        panic!("diagnostic pull should complete after analysis is published");
    };
    let DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(report)) =
        response.unwrap()
    else {
        panic!("first diagnostic pull should return a full report");
    };
    assert_eq!(report.full_document_diagnostic_report.items, vec![diagnostic("current")]);
}

#[test]
fn document_diagnostic_canonicalizes_file_uris() {
    let canonical_uri = diagnostic_uri();
    let encoded_uri =
        Url::parse(&canonical_uri.as_str().replacen("Diagnostics.sol", "%44iagnostics.sol", 1))
            .expect("encoded URI should be valid");
    assert_ne!(canonical_uri, encoded_uri);
    assert_eq!(canonical_uri.to_file_path(), encoded_uri.to_file_path());

    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.snapshot().publish_diagnostics(
        DiagnosticOwner::Compiler,
        DiagnosticMap::from_iter([(canonical_uri.clone(), vec![diagnostic("compiler")])]),
    );

    let response = expect_ready(crate::handlers::document_diagnostic(
        &mut state,
        document_diagnostic_params(encoded_uri, None),
    ));
    let DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(report)) =
        response.unwrap()
    else {
        panic!("first diagnostic pull should return a full report");
    };
    assert_eq!(report.full_document_diagnostic_report.items, vec![diagnostic("compiler")]);
    let result_id = report.full_document_diagnostic_report.result_id.unwrap();

    let response = expect_ready(crate::handlers::document_diagnostic(
        &mut state,
        document_diagnostic_params(canonical_uri, Some(result_id.clone())),
    ));
    let DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Unchanged(report)) =
        response.unwrap()
    else {
        panic!("equivalent URI should share the cached result ID");
    };
    assert_eq!(report.unchanged_document_diagnostic_report.result_id, result_id);
}

fn pause_blocking_pool() -> (std_mpsc::Sender<()>, tokio::task::JoinHandle<()>) {
    let (started_tx, started_rx) = std_mpsc::channel();
    let (release_tx, release_rx) = std_mpsc::channel();
    let task = tokio::task::spawn_blocking(move || {
        started_tx.send(()).unwrap();
        release_rx.recv().unwrap();
    });
    started_rx
        .recv_timeout(ASYNC_TEST_TIMEOUT)
        .expect("blocking worker should start before analysis is requested");
    (release_tx, task)
}

fn assert_analysis_stale_before_diagnostic_publication(
    mut state: GlobalState,
    stale_version: usize,
    advance_epoch: impl FnOnce(&mut GlobalState) + Send + 'static,
) {
    let stale_snapshot = state.snapshot();
    assert!(stale_snapshot.is_current(stale_version));

    let diagnostics = state.diagnostics.clone();
    let diagnostics_guard = diagnostics.write();
    let start = Arc::new(Barrier::new(2));
    let worker_start = start.clone();
    let (finished_tx, finished_rx) = std_mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        worker_start.wait();
        advance_epoch(&mut state);
        finished_tx.send(()).unwrap();
    });

    start.wait();
    let deadline = Instant::now() + ASYNC_TEST_TIMEOUT;
    while stale_snapshot.is_current(stale_version) && Instant::now() < deadline {
        std::thread::yield_now();
    }
    let stale_while_diagnostics_locked = !stale_snapshot.is_current(stale_version);
    let finished_while_diagnostics_locked =
        !matches!(finished_rx.try_recv(), Err(std_mpsc::TryRecvError::Empty));

    drop(diagnostics_guard);
    finished_rx
        .recv_timeout(ASYNC_TEST_TIMEOUT)
        .expect("epoch advance should finish after diagnostic publication is unblocked");
    worker.join().unwrap();

    assert!(!finished_while_diagnostics_locked, "diagnostic lock should block publication");
    assert!(
        stale_while_diagnostics_locked,
        "old analysis should be stale before diagnostic publication"
    );
}

fn assert_source_notification_tracks_until_publish(
    project: &TestProject,
    path: &Path,
    external_refresh_pending: bool,
    notify: impl FnOnce(&mut GlobalState),
) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .unwrap();
    runtime.block_on(async {
        let mut state = GlobalState::new(ClientSocket::new_closed());
        state.config = Arc::new(project.config());
        state.vfs = Arc::new(RwLock::new(project.vfs()));
        let (release_worker, worker) = pause_blocking_pool();

        notify(&mut state);

        assert_eq!(
            state.analysis_commit.lock().external_refresh.is_some(),
            external_refresh_pending
        );
        assert!(state.analysis_commit.lock().natspec_pending_source_changes.contains(path));
        let changed_uri = Url::from_file_path(path).unwrap();
        let other_uri = Url::from_file_path(project.path("/OtherRequest.sol")).unwrap();
        assert!(state.natspec_semantics_are_usable(&changed_uri));
        assert!(!state.natspec_semantics_are_usable(&other_uri));
        release_worker.send(()).unwrap();
        worker.await.unwrap();
        tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
            .await
            .expect("source analysis should finish")
            .unwrap();
        assert!(state.analysis_commit.lock().natspec_pending_source_changes.is_empty());
        assert!(state.natspec_semantics_are_usable(&other_uri));
    });
}

fn assert_external_context_notification_until_publish(
    project: &TestProject,
    notify: impl FnOnce(&mut GlobalState),
) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .unwrap();
    runtime.block_on(async {
        let mut state = GlobalState::new(ClientSocket::new_closed());
        state.config = Arc::new(project.config());
        state.vfs = Arc::new(RwLock::new(project.vfs()));
        let (release_worker, worker) = pause_blocking_pool();

        notify(&mut state);

        assert!(state.analysis_commit.lock().external_refresh.is_some());
        release_worker.send(()).unwrap();
        worker.await.unwrap();
        tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
            .await
            .expect("external analysis should finish")
            .unwrap();
        assert!(state.analysis_commit.lock().external_refresh.is_none());
    });
}

#[test]
fn replacement_analysis_invalidates_old_worker_before_removed_diagnostics_publish() {
    let uri = diagnostic_uri();
    let path = uri.to_file_path().unwrap();
    let mut state = GlobalState::new(ClientSocket::new_closed());
    let (stale_version, _stale_progress) = state
        .begin_analysis(AnalysisMode::Recompute, Vec::new(), Vec::new(), AnalysisTrigger::Document)
        .unwrap();
    state.snapshot().publish_diagnostics(
        DiagnosticOwner::Compiler,
        DiagnosticMap::from_iter([(uri, vec![diagnostic("removed")])]),
    );

    assert_analysis_stale_before_diagnostic_publication(state, stale_version, move |state| {
        state
            .begin_analysis(
                AnalysisMode::Recompute,
                vec![path],
                Vec::new(),
                AnalysisTrigger::Document,
            )
            .expect("replacement analysis should start");
    });
}

#[test]
fn clearing_analysis_cache_invalidates_old_worker_before_diagnostics_publish() {
    let uri = diagnostic_uri();
    let state = GlobalState::new(ClientSocket::new_closed());
    state.mark_analysis_pending_for_test();
    let stale_version = state.analysis_version.load(Ordering::Acquire);
    state.snapshot().publish_diagnostics(
        DiagnosticOwner::Compiler,
        DiagnosticMap::from_iter([(uri, vec![diagnostic("cleared")])]),
    );

    assert_analysis_stale_before_diagnostic_publication(
        state,
        stale_version,
        GlobalState::clear_analysis_cache,
    );
}

#[tokio::test(flavor = "current_thread")]
async fn clearing_analysis_cache_publishes_an_empty_current_snapshot() {
    let project = TestProject::from_fixture(
        r#"
        //- /Cached.sol
        contract Cached {}
        "#,
    );
    let mut batches = snapshot(&project).analysis_batches(Vec::new());
    let old_tables = analyze(batches.pop().unwrap()).symbol_tables;
    assert!(!old_tables.workspace_symbols("").is_empty());

    let mut state = GlobalState::new(ClientSocket::new_closed());
    *state.symbol_tables.write() = old_tables;
    let uri = Url::from_file_path(project.path("/Cached.sol")).unwrap();
    let owner = flycheck_owner(project.root());
    let compiler_diagnostic = diagnostic("compiler");
    let flycheck_diagnostic = diagnostic("flycheck");
    let mut state_snapshot = state.snapshot();
    state_snapshot.publish_diagnostics(
        DiagnosticOwner::Compiler,
        DiagnosticMap::from_iter([(uri.clone(), vec![compiler_diagnostic])]),
    );
    state_snapshot.publish_diagnostics(
        owner,
        DiagnosticMap::from_iter([(uri.clone(), vec![flycheck_diagnostic])]),
    );

    state.clear_analysis_cache();

    let tables = tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
        .await
        .expect("cleared analysis should be published")
        .unwrap();
    assert!(tables.read().workspace_symbols("").is_empty());
    assert!(state.analysis_cache_invalidated());

    let probe_owner =
        DiagnosticOwner::Flycheck { id: "probe".into(), workspace: project.root().into() };
    let batches = state
        .diagnostics
        .write()
        .replace_and_publish_batches(
            probe_owner,
            DiagnosticMap::from_iter([(uri, vec![diagnostic("probe")])]),
        )
        .batches;
    assert_eq!(batches.len(), 1);
    let mut messages =
        batches[0].1.iter().map(|diagnostic| diagnostic.message.as_str()).collect::<Vec<_>>();
    messages.sort_unstable();
    assert_eq!(messages, ["flycheck", "probe"]);
}

#[tokio::test(flavor = "current_thread")]
async fn clearing_analysis_cache_publishes_compiler_diagnostic_removals() {
    let (server_main, client_socket) = async_lsp::MainLoop::new_server(|_| Router::new(()));
    let (notifications_tx, mut notifications_rx) = mpsc::unbounded_channel();
    let (client_main, _server_socket) = async_lsp::MainLoop::new_client(move |_| {
        let mut router = Router::new(notifications_tx);
        router.notification::<notification::PublishDiagnostics>(|notifications, params| {
            notifications.send(params).unwrap();
            ControlFlow::Continue(())
        });
        router
    });

    let (server_stream, client_stream) = tokio::io::duplex(64 << 10);
    let (server_rx, server_tx) = tokio::io::split(server_stream);
    let server_task =
        tokio::spawn(server_main.run_buffered(server_rx.compat(), server_tx.compat_write()));
    let (client_rx, client_tx) = tokio::io::split(client_stream);
    let client_task =
        tokio::spawn(client_main.run_buffered(client_rx.compat(), client_tx.compat_write()));

    let mut state = GlobalState::new(client_socket);
    let compiler_only = Url::parse("file:///workspace/CompilerOnly.sol").unwrap();
    let shared = Url::parse("file:///workspace/Shared.sol").unwrap();
    let owner = flycheck_owner("/workspace");
    let mut snapshot = state.snapshot();
    snapshot.publish_diagnostics(
        DiagnosticOwner::Compiler,
        DiagnosticMap::from_iter([
            (compiler_only.clone(), vec![diagnostic("compiler only")]),
            (shared.clone(), vec![diagnostic("compiler shared")]),
        ]),
    );
    snapshot.publish_diagnostics(
        owner,
        DiagnosticMap::from_iter([(shared.clone(), vec![diagnostic("flycheck")])]),
    );
    for _ in 0..3 {
        tokio::time::timeout(ASYNC_TEST_TIMEOUT, notifications_rx.recv())
            .await
            .expect("seed diagnostics should be published")
            .expect("diagnostic channel should stay open");
    }

    state.clear_analysis_cache();

    let mut cleared = Vec::new();
    for _ in 0..2 {
        cleared.push(
            tokio::time::timeout(ASYNC_TEST_TIMEOUT, notifications_rx.recv())
                .await
                .expect("cleared diagnostics should be published")
                .expect("diagnostic channel should stay open"),
        );
    }
    cleared.sort_by(|lhs, rhs| lhs.uri.as_str().cmp(rhs.uri.as_str()));
    assert_eq!(cleared[0].uri, compiler_only);
    assert!(cleared[0].diagnostics.is_empty());
    assert_eq!(cleared[1].uri, shared);
    assert_eq!(
        cleared[1]
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect::<Vec<_>>(),
        ["flycheck"]
    );

    server_task.abort();
    client_task.abort();
    let _ = server_task.await;
    let _ = client_task.await;
}

#[tokio::test(flavor = "current_thread")]
async fn clearing_analysis_cache_publishes_before_ending_progress() {
    let project = TestProject::from_fixture(
        r#"
        //- /Cleared.sol
        contract Cleared {}
        "#,
    );
    let uri = Url::from_file_path(project.path("/Cleared.sol")).unwrap();
    let mut harness = work_done_harness();
    let client = harness.client.clone();
    let mut state = GlobalState::new(client.clone());
    state.analysis_progress =
        ProgressCoordinator::with_timing(client, true, Duration::ZERO, Duration::from_secs(1));
    state.snapshot().publish_diagnostics(
        DiagnosticOwner::Compiler,
        DiagnosticMap::from_iter([(uri.clone(), vec![diagnostic("old compiler")])]),
    );
    match harness.next_event().await {
        WorkDoneEvent::Diagnostics(params) => {
            assert_eq!(params.uri, uri);
            assert_eq!(params.diagnostics.len(), 1);
        }
        event => panic!("expected seeded diagnostics, got {event:?}"),
    }

    let (_, progress) = state
        .begin_analysis(AnalysisMode::Recompute, Vec::new(), Vec::new(), AnalysisTrigger::Document)
        .unwrap();
    progress.begin();
    progress.report("Analyzing workspace");
    let WorkDoneEvent::Create(create) = harness.next_event().await else {
        panic!("expected progress creation")
    };
    let token = create.token;
    harness.acknowledge_create();
    match harness.next_event().await {
        WorkDoneEvent::Progress(ProgressParams {
            token: actual,
            value: ProgressParamsValue::WorkDone(WorkDoneProgress::Begin(begin)),
        }) => {
            assert_eq!(actual, token);
            assert_eq!(begin.message.as_deref(), Some("Analyzing workspace"));
        }
        event => panic!("expected progress begin, got {event:?}"),
    }

    state.clear_analysis_cache();

    match harness.next_event().await {
        WorkDoneEvent::Diagnostics(params) => {
            assert_eq!(params.uri, uri);
            assert!(params.diagnostics.is_empty());
        }
        event => panic!("expected cleared diagnostics, got {event:?}"),
    }
    match harness.next_event().await {
        WorkDoneEvent::Progress(ProgressParams {
            token: actual,
            value: ProgressParamsValue::WorkDone(WorkDoneProgress::End(end)),
        }) => {
            assert_eq!(actual, token);
            assert_eq!(end.message.as_deref(), Some("Workspace index cleared"));
        }
        event => panic!("expected progress end, got {event:?}"),
    }

    harness.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn clearing_analysis_cache_suppresses_progress_pending_creation() {
    let project = TestProject::from_fixture(
        r#"
        //- /Cleared.sol
        contract Cleared {}
        "#,
    );
    let uri = Url::from_file_path(project.path("/Cleared.sol")).unwrap();
    let mut harness = work_done_harness();
    let client = harness.client.clone();
    let mut state = GlobalState::new(client.clone());
    state.analysis_progress =
        ProgressCoordinator::with_timing(client, true, Duration::ZERO, Duration::from_secs(1));
    state.snapshot().publish_diagnostics(
        DiagnosticOwner::Compiler,
        DiagnosticMap::from_iter([(uri.clone(), vec![diagnostic("old compiler")])]),
    );
    assert!(matches!(harness.next_event().await, WorkDoneEvent::Diagnostics(_)));

    let (_, progress) = state
        .begin_analysis(AnalysisMode::Recompute, Vec::new(), Vec::new(), AnalysisTrigger::Document)
        .unwrap();
    progress.begin();
    let WorkDoneEvent::Create(_) = harness.next_event().await else {
        panic!("expected progress creation")
    };
    progress.report("obsolete analysis");
    progress.finish("obsolete completion");

    state.clear_analysis_cache();

    match harness.next_event().await {
        WorkDoneEvent::Diagnostics(params) => {
            assert_eq!(params.uri, uri);
            assert!(params.diagnostics.is_empty());
        }
        event => panic!("expected cleared diagnostics, got {event:?}"),
    }
    harness.acknowledge_create();
    harness.probe().await;
    assert!(matches!(harness.events.try_recv(), Err(mpsc::error::TryRecvError::Empty)));

    harness.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn superseded_analysis_cannot_publish_or_end_latest_progress() {
    let project = TestProject::from_fixture(
        r#"
        //- /Current.sol
        contract Current {}
        "#,
    );
    let uri = Url::from_file_path(project.path("/Current.sol")).unwrap();
    let mut harness = work_done_harness();
    let client = harness.client.clone();
    let mut state = GlobalState::new(client.clone());
    state.analysis_progress =
        ProgressCoordinator::with_timing(client, true, Duration::ZERO, Duration::from_secs(1));

    let (stale_version, stale_progress) = state
        .begin_analysis(AnalysisMode::Recompute, Vec::new(), Vec::new(), AnalysisTrigger::Document)
        .unwrap();
    let mut stale_snapshot = state.snapshot();
    stale_progress.begin();
    let WorkDoneEvent::Create(create) = harness.next_event().await else {
        panic!("expected progress creation")
    };
    let token = create.token;
    harness.acknowledge_create();
    assert!(matches!(
        harness.next_event().await,
        WorkDoneEvent::Progress(ProgressParams {
            token: actual,
            value: ProgressParamsValue::WorkDone(WorkDoneProgress::Begin(_)),
        }) if actual == token
    ));

    let (latest_version, latest_progress) = state
        .begin_analysis(AnalysisMode::Recompute, Vec::new(), Vec::new(), AnalysisTrigger::Document)
        .unwrap();
    let mut latest_snapshot = state.snapshot();
    match harness.next_event().await {
        WorkDoneEvent::Progress(ProgressParams {
            token: actual,
            value: ProgressParamsValue::WorkDone(WorkDoneProgress::Report(report)),
        }) => {
            assert_eq!(actual, token);
            assert_eq!(report.message.as_deref(), Some("Workspace changed, restarting analysis"));
        }
        event => panic!("expected replacement report, got {event:?}"),
    }

    stale_progress.report("stale report");
    stale_progress.finish("stale end");
    let stale_result = AnalysisResult {
        analyzed_documents: AnalyzedDocuments::default(),
        diagnostics: DiagnosticMap::from_iter([(uri.clone(), vec![diagnostic("stale")])]),
        symbol_tables: SymbolTables::default(),
    };
    assert!(!stale_snapshot.publish_analysis(stale_version, stale_result));
    assert!(matches!(harness.events.try_recv(), Err(mpsc::error::TryRecvError::Empty)));

    let latest_result = AnalysisResult {
        analyzed_documents: AnalyzedDocuments::default(),
        diagnostics: DiagnosticMap::from_iter([(uri.clone(), vec![diagnostic("current")])]),
        symbol_tables: SymbolTables::default(),
    };
    assert!(latest_snapshot.publish_analysis(latest_version, latest_result));
    match harness.next_event().await {
        WorkDoneEvent::Diagnostics(params) => {
            assert_eq!(params.uri, uri);
            assert_eq!(
                params
                    .diagnostics
                    .iter()
                    .map(|diagnostic| diagnostic.message.as_str())
                    .collect::<Vec<_>>(),
                ["current"]
            );
        }
        event => panic!("expected current diagnostics, got {event:?}"),
    }

    finish_analysis_progress_if_current(
        latest_version,
        &state.analysis_version,
        &state.analysis_commit,
        &latest_progress,
        "Workspace index ready",
    );
    match harness.next_event().await {
        WorkDoneEvent::Progress(ProgressParams {
            token: actual,
            value: ProgressParamsValue::WorkDone(WorkDoneProgress::End(end)),
        }) => {
            assert_eq!(actual, token);
            assert_eq!(end.message.as_deref(), Some("Workspace index ready"));
        }
        event => panic!("expected progress end, got {event:?}"),
    }

    harness.shutdown().await;
}

#[test]
fn clearing_analysis_cache_rejects_older_analysis_results() {
    let project = TestProject::from_fixture(
        r#"
        //- /Stale.sol
        contract Stale {}
        "#,
    );
    let mut batches = snapshot(&project).analysis_batches(Vec::new());
    let mut stale_result = analyze(batches.pop().unwrap());
    let uri = Url::from_file_path(project.path("/Stale.sol")).unwrap();
    stale_result.diagnostics.insert(uri.clone(), vec![diagnostic("stale compiler")]);
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.mark_analysis_pending_for_test();
    let mut stale_snapshot = state.snapshot();

    state.clear_analysis_cache();

    assert!(!stale_snapshot.publish_analysis(1, stale_result));
    assert!(state.symbol_tables.read().workspace_symbols("").is_empty());
    let probe_owner =
        DiagnosticOwner::Flycheck { id: "probe".into(), workspace: project.root().into() };
    let batches = state
        .diagnostics
        .write()
        .replace_and_publish_batches(
            probe_owner,
            DiagnosticMap::from_iter([(uri, vec![diagnostic("probe")])]),
        )
        .batches;
    assert_eq!(
        batches[0].1.iter().map(|diagnostic| diagnostic.message.as_str()).collect::<Vec<_>>(),
        ["probe"]
    );
}

#[test]
fn reindex_if_invalidated_is_a_no_op_for_a_current_cache() {
    let mut state = GlobalState::new(ClientSocket::new_closed());
    let version = state.analysis_version.load(Ordering::Acquire);

    state.reindex_if_invalidated();

    assert_eq!(state.analysis_version.load(Ordering::Acquire), version);
    assert!(!state.analysis_cache_invalidated());
}

#[tokio::test(flavor = "current_thread")]
async fn failed_current_analysis_ends_visible_progress() {
    let mut harness = work_done_harness();
    let client = harness.client.clone();
    let mut state = GlobalState::new(client.clone());
    state.analysis_progress =
        ProgressCoordinator::with_timing(client, true, Duration::ZERO, Duration::from_secs(1));
    let (version, progress) = state
        .begin_analysis(AnalysisMode::Recompute, Vec::new(), Vec::new(), AnalysisTrigger::Document)
        .unwrap();
    progress.begin();

    let WorkDoneEvent::Create(create) = harness.next_event().await else {
        panic!("expected progress creation")
    };
    let token = create.token;
    harness.acknowledge_create();
    assert!(matches!(
        harness.next_event().await,
        WorkDoneEvent::Progress(ProgressParams {
            token: actual,
            value: ProgressParamsValue::WorkDone(WorkDoneProgress::Begin(_)),
        }) if actual == token
    ));

    let task = tokio::spawn(async { panic!("test analysis failure") });
    state.monitor_analysis_task(version, task, progress);
    tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
        .await
        .expect("failed analysis should publish its terminal version")
        .unwrap();
    assert!(state.analysis_cache_invalidated());

    match harness.next_event().await {
        WorkDoneEvent::Progress(ProgressParams {
            token: actual,
            value: ProgressParamsValue::WorkDone(WorkDoneProgress::End(end)),
        }) => {
            assert_eq!(actual, token);
            assert_eq!(end.message.as_deref(), Some("Workspace indexing failed"));
        }
        event => panic!("expected failure end, got {event:?}"),
    }

    harness.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn failed_current_workspace_discovery_terminates_the_analysis_epoch() {
    let mut state = GlobalState::new(ClientSocket::new_closed());
    let (version, progress) = state
        .begin_analysis(AnalysisMode::Rediscover, Vec::new(), Vec::new(), AnalysisTrigger::External)
        .unwrap();
    assert!(state.analysis_commit.lock().discovery_pending);
    let monitor = WorkspaceDiscoveryMonitor {
        version,
        disk_paths: Vec::new(),
        progress,
        cancellation: IndexingCancellation::default(),
        analysis_version: state.analysis_version.clone(),
        published_analysis_version: state.published_analysis_version.clone(),
        analysis_commit: state.analysis_commit.clone(),
        client: state.client.clone(),
        config: state.config.clone(),
    };
    let worker = tokio::task::spawn_blocking(|| -> Option<WorkspaceDiscoveryResult> {
        panic!("test workspace discovery failure")
    });

    monitor.finish(worker).await;

    tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
        .await
        .expect("failed discovery should publish its terminal version")
        .unwrap();
    let commit = state.analysis_commit.lock();
    assert!(commit.cache_invalidated);
    assert!(!commit.discovery_pending);
}

#[tokio::test(flavor = "current_thread")]
async fn failed_current_analysis_recovers_after_save() {
    let project = TestProject::from_fixture(
        r#"
        //- /Old.sol
        contract Old {}
        "#,
    );
    let mut batches = snapshot(&project).analysis_batches(Vec::new());
    let old_tables = analyze(batches.pop().unwrap()).symbol_tables;
    let uri = Url::from_file_path(project.path("/Old.sol")).unwrap();
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(project.config());
    *state.symbol_tables.write() = old_tables;
    state.snapshot().publish_diagnostics(
        DiagnosticOwner::Compiler,
        DiagnosticMap::from_iter([(uri.clone(), vec![diagnostic("old compiler")])]),
    );

    let (version, progress) = state
        .begin_analysis(AnalysisMode::Recompute, Vec::new(), Vec::new(), AnalysisTrigger::Document)
        .unwrap();
    let task = tokio::spawn(async { panic!("test analysis failure") });
    state.monitor_analysis_task(version, task, progress);

    let tables = tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
        .await
        .expect("failed analysis should release waiters")
        .unwrap();
    assert!(tables.read().workspace_symbols("Old").iter().any(|symbol| symbol.name == "Old"));
    assert!(state.analysis_cache_invalidated());
    assert!(!state.natspec_semantics_are_usable(&uri));

    let probe_owner =
        DiagnosticOwner::Flycheck { id: "probe".into(), workspace: project.root().into() };
    let batches = state
        .diagnostics
        .write()
        .replace_and_publish_batches(
            probe_owner,
            DiagnosticMap::from_iter([(uri.clone(), vec![diagnostic("probe")])]),
        )
        .batches;
    let mut messages =
        batches[0].1.iter().map(|diagnostic| diagnostic.message.as_str()).collect::<Vec<_>>();
    messages.sort_unstable();
    assert_eq!(messages, ["old compiler", "probe"]);

    project.write_file("/Old.sol", "contract Recovered {}");

    let result = crate::handlers::did_save_text_document(
        &mut state,
        DidSaveTextDocumentParams {
            text_document: TextDocumentIdentifier::new(uri.clone()),
            text: None,
        },
    );
    assert!(matches!(result, ControlFlow::Continue(())));
    let tables = tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
        .await
        .expect("save should recover failed analysis")
        .unwrap();
    let tables = tables.read();
    assert!(tables.workspace_symbols("Old").is_empty());
    assert!(tables.workspace_symbols("Recovered").iter().any(|symbol| symbol.name == "Recovered"));
    drop(tables);
    assert!(!state.analysis_cache_invalidated());
    assert!(state.natspec_semantics_are_usable(&uri));
}

#[tokio::test(flavor = "current_thread")]
async fn cancelled_current_analysis_recovers_after_save() {
    let project = TestProject::from_fixture(
        r#"
        //- /Recovered.sol
        contract Recovered {}
        "#,
    );
    let uri = Url::from_file_path(project.path("/Recovered.sol")).unwrap();
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(project.config());

    let (version, progress) = state
        .begin_analysis(AnalysisMode::Recompute, Vec::new(), Vec::new(), AnalysisTrigger::Document)
        .unwrap();
    let task = tokio::spawn(std::future::pending::<AnalysisTaskOutcome>());
    task.abort();
    state.monitor_analysis_task(version, task, progress);

    let tables = tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
        .await
        .expect("cancelled analysis should release waiters")
        .unwrap();
    assert!(tables.read().workspace_symbols("").is_empty());
    assert!(state.analysis_cache_invalidated());
    assert!(!state.natspec_semantics_are_usable(&uri));

    let result = crate::handlers::did_save_text_document(
        &mut state,
        DidSaveTextDocumentParams {
            text_document: TextDocumentIdentifier::new(uri.clone()),
            text: None,
        },
    );
    assert!(matches!(result, ControlFlow::Continue(())));
    let tables = tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
        .await
        .expect("save should recover cancelled analysis")
        .unwrap();
    assert!(
        tables
            .read()
            .workspace_symbols("Recovered")
            .iter()
            .any(|symbol| symbol.name == "Recovered")
    );
    assert!(!state.analysis_cache_invalidated());
    assert!(state.natspec_semantics_are_usable(&uri));
}

#[tokio::test(flavor = "current_thread")]
async fn reindex_rediscovers_disk_files_without_preclearing_the_old_index() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"

        //- /src/Old.sol
        contract Old {}
        "#,
    );
    let config = project.config();
    let mut batches = snapshot_with_config(config.clone(), Vfs::default()).analysis_batches(vec![]);
    let old_tables = analyze(batches.pop().unwrap()).symbol_tables;
    project.remove_file("/src/Old.sol");
    project.write_file("/src/New.sol", "contract New {}");

    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(config);
    *state.symbol_tables.write() = old_tables;
    let tables = state.symbol_tables.clone();
    {
        let current_tables = tables.write();

        state.reindex();

        assert!(state.analysis_commit.lock().external_refresh.is_some());
        assert!(current_tables.workspace_symbols("Old").iter().any(|symbol| symbol.name == "Old"));
        assert!(current_tables.workspace_symbols("New").is_empty());
    }

    let new_tables = tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
        .await
        .expect("reindex should finish")
        .unwrap();
    let new_tables = new_tables.read();
    assert!(new_tables.workspace_symbols("Old").is_empty());
    assert!(new_tables.workspace_symbols("New").iter().any(|symbol| symbol.name == "New"));
}

#[tokio::test(flavor = "current_thread")]
async fn save_after_clear_rediscovers_disk_files_and_preserves_vfs_overlays() {
    let mut project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"

        //- /src/Open.sol
        contract DiskVersion {}
        "#,
    );
    project.open_file("/src/Open.sol", "contract Unsaved {}");
    let config = project.config();
    project.write_file("/src/New.sol", "contract New {}");

    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(config);
    state.vfs = Arc::new(RwLock::new(project.vfs()));
    state.clear_analysis_cache();

    let result = crate::handlers::did_save_text_document(
        &mut state,
        DidSaveTextDocumentParams {
            text_document: TextDocumentIdentifier::new(
                Url::from_file_path(project.path("/src/Open.sol")).unwrap(),
            ),
            text: None,
        },
    );
    assert!(matches!(result, ControlFlow::Continue(())));

    let tables = tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
        .await
        .expect("save should rebuild an invalidated cache")
        .unwrap();
    let tables = tables.read();
    assert!(tables.workspace_symbols("DiskVersion").is_empty());
    assert!(tables.workspace_symbols("Unsaved").iter().any(|symbol| symbol.name == "Unsaved"));
    assert!(tables.workspace_symbols("New").iter().any(|symbol| symbol.name == "New"));
}

#[tokio::test(flavor = "current_thread")]
async fn no_op_change_after_clear_recovers_the_invalidated_cache() {
    let mut project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"

        //- /src/Open.sol
        contract DiskVersion {}
        "#,
    );
    project.open_file("/src/Open.sol", "contract Unsaved {}");
    let config = project.config();
    project.write_file("/src/New.sol", "contract New {}");
    let uri = Url::from_file_path(project.path("/src/Open.sol")).unwrap();

    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(config);
    state.vfs = Arc::new(RwLock::new(project.vfs()));
    state.clear_analysis_cache();

    let result = crate::handlers::did_change_text_document(
        &mut state,
        DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier::new(uri, 1),
            content_changes: vec![TextDocumentContentChangeEvent {
                range: Some(Range::new(Position::new(0, 0), Position::new(0, 0))),
                range_length: Some(0),
                text: String::new(),
            }],
        },
    );
    assert!(matches!(result, ControlFlow::Continue(())));

    let tables = tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
        .await
        .expect("no-op change should rebuild an invalidated cache")
        .unwrap();
    let tables = tables.read();
    assert!(tables.workspace_symbols("DiskVersion").is_empty());
    assert!(tables.workspace_symbols("Unsaved").iter().any(|symbol| symbol.name == "Unsaved"));
    assert!(tables.workspace_symbols("New").iter().any(|symbol| symbol.name == "New"));
}

fn diagnostic(message: &str) -> Diagnostic {
    Diagnostic::new_simple(Range::new(Position::new(0, 0), Position::new(0, 1)), message.into())
}

fn diagnostic_uri() -> Url {
    Url::from_file_path(std::env::temp_dir().join("Diagnostics.sol")).unwrap()
}

fn document_diagnostic_params(
    uri: Url,
    previous_result_id: Option<String>,
) -> DocumentDiagnosticParams {
    DocumentDiagnosticParams {
        text_document: TextDocumentIdentifier::new(uri),
        identifier: None,
        previous_result_id,
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    }
}

fn expect_ready<F: Future>(future: F) -> F::Output {
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    let mut future = std::pin::pin!(future);
    let Poll::Ready(output) = future.as_mut().poll(&mut context) else {
        panic!("future should complete immediately");
    };
    output
}

fn flycheck_owner(workspace: impl Into<PathBuf>) -> DiagnosticOwner {
    DiagnosticOwner::Flycheck { id: "slow".into(), workspace: workspace.into() }
}

#[test]
fn watched_file_registration_has_global_fallback_patterns() {
    let [registration] =
        watched_file_registration_params(&Config::default()).registrations.try_into().unwrap();
    assert_eq!(registration.id, "solar-watched-files");
    assert_eq!(registration.method, lsp_types::notification::DidChangeWatchedFiles::METHOD);

    assert_eq!(
        registration.register_options,
        Some(serde_json::json!({
            "watchers": [
                { "globPattern": "**/*.sol", "kind": WatchKind::Create | WatchKind::Change | WatchKind::Delete },
                { "globPattern": "**/foundry.toml", "kind": WatchKind::Create | WatchKind::Change | WatchKind::Delete },
                { "globPattern": "**/remappings.txt", "kind": WatchKind::Create | WatchKind::Change | WatchKind::Delete },
            ],
        }))
    );
}

#[test]
fn watched_file_registration_uses_patterns_relative_to_each_workspace_root() {
    let project = TestProject::new();
    let mut params = project.initialize_params_with_roots(&["/one", "/two"]);
    params.capabilities.workspace = Some(WorkspaceClientCapabilities {
        did_change_watched_files: Some(DidChangeWatchedFilesClientCapabilities {
            dynamic_registration: Some(true),
            relative_pattern_support: Some(true),
        }),
        ..Default::default()
    });
    let (_, config) = negotiate_capabilities(params);
    let [registration] =
        watched_file_registration_params(&config).registrations.try_into().unwrap();
    let options = registration.register_options.unwrap();
    let watchers = options["watchers"].as_array().unwrap();

    for root in ["/one", "/two"] {
        let base_uri = Url::from_file_path(project.path(root)).unwrap().to_string();
        for pattern in ["**/*.sol", "**/foundry.toml", "**/remappings.txt"] {
            assert!(watchers.iter().any(|watcher| {
                watcher["globPattern"]["baseUri"] == base_uri
                    && watcher["globPattern"]["pattern"] == pattern
            }));
        }
    }
}

#[test]
fn watched_file_registration_includes_parent_config_and_external_source_specs() {
    let project = TestProject::from_fixture(
        r#"
        //- /repo/foundry.toml
        [profile.default]
        src = "../shared"

        //- /repo/workspace/.keep
        "#,
    );
    let mut params = project.initialize_params_with_roots(&["/repo/workspace"]);
    params.capabilities.workspace = Some(WorkspaceClientCapabilities {
        did_change_watched_files: Some(DidChangeWatchedFilesClientCapabilities {
            dynamic_registration: Some(true),
            relative_pattern_support: Some(true),
        }),
        ..Default::default()
    });
    let (_, mut config) = negotiate_capabilities(params);
    config.rediscover_workspaces();
    let [registration] =
        watched_file_registration_params(&config).registrations.try_into().unwrap();
    let options = registration.register_options.unwrap();
    let watchers = options["watchers"].as_array().unwrap();
    let parent_uri = Url::from_file_path(project.path("/repo")).unwrap().to_string();
    let external_uri = Url::from_file_path(project.path("/shared")).unwrap().to_string();

    assert!(watchers.iter().any(|watcher| {
        watcher["globPattern"]["baseUri"] == parent_uri
            && watcher["globPattern"]["pattern"] == "foundry.toml"
    }));
    assert!(watchers.iter().any(|watcher| {
        watcher["globPattern"]["baseUri"] == external_uri
            && watcher["globPattern"]["pattern"] == "**/*.sol"
    }));
}

#[test]
fn watched_file_specs_add_external_analysis_path_parents_once() {
    let project = TestProject::new();
    std::fs::create_dir(project.path("/workspace")).unwrap();
    let config = project.config_with_roots(&["/workspace"]);
    let dependency_parent = project.path("/outside");
    let other_parent = project.path("/other");
    let analysis_paths = AnalysisPathIndex {
        resolved_dependencies: FxHashSet::from_iter([dependency_parent.join("Dependency.sol")]),
        existing_unresolved_candidates: FxHashSet::from_iter([
            other_parent.join("Candidate.sol"),
            dependency_parent.join("nested/Deep.sol"),
        ]),
        missing_candidates: FxHashSet::from_iter([dependency_parent.join("Missing.sol")]),
    };

    let specs = watched_file_specs(&config, &analysis_paths);

    assert_eq!(
        specs
            .iter()
            .filter(|spec| spec.base == dependency_parent && spec.pattern == "**/*.sol")
            .count(),
        1
    );
    assert!(specs.iter().any(|spec| spec.base == other_parent && spec.pattern == "**/*.sol"));
    assert!(!specs.iter().any(|spec| spec.base == dependency_parent.join("nested")));
}

#[test]
fn concurrent_watched_file_updates_keep_desired_specs_and_generation_in_sync() {
    let project = TestProject::new();
    std::fs::create_dir(project.path("/workspace")).unwrap();
    let mut params = project.initialize_params_with_roots(&["/workspace"]);
    params.capabilities.workspace = Some(WorkspaceClientCapabilities {
        did_change_watched_files: Some(DidChangeWatchedFilesClientCapabilities {
            dynamic_registration: Some(true),
            relative_pattern_support: Some(true),
        }),
        ..Default::default()
    });
    let (_, config) = negotiate_capabilities(params);
    let config = Arc::new(config);
    let coordinator = Arc::new(WatchedFileRegistrationCoordinator::default());
    let barrier = Arc::new(Barrier::new(3));
    let specs = [
        vec![WatchedFileSpec { base: project.path("/first"), pattern: "**/*.sol" }],
        vec![WatchedFileSpec { base: project.path("/second"), pattern: "**/*.sol" }],
    ];

    let updates = std::thread::scope(|scope| {
        let handles = specs.map(|specs| {
            let config = config.clone();
            let coordinator = coordinator.clone();
            let barrier = barrier.clone();
            scope.spawn(move || {
                barrier.wait();
                prepare_watched_file_registration_update(&config, &coordinator, specs, true)
                    .unwrap()
            })
        });
        barrier.wait();
        handles.map(|handle| handle.join().unwrap())
    });

    let desired_specs = coordinator.desired_specs.lock().clone().unwrap();
    let generation = coordinator.generation.load(Ordering::Acquire);
    let current = updates.iter().find(|update| update.desired_specs == desired_specs).unwrap();
    assert_eq!(current.generation, generation);
}

#[test]
fn global_fallback_watched_file_update_ignores_spec_changes() {
    let project = TestProject::new();
    let mut params = project.initialize_params();
    params.capabilities.workspace = Some(WorkspaceClientCapabilities {
        did_change_watched_files: Some(DidChangeWatchedFilesClientCapabilities {
            dynamic_registration: Some(true),
            relative_pattern_support: Some(false),
        }),
        ..Default::default()
    });
    let (_, config) = negotiate_capabilities(params);
    let coordinator = WatchedFileRegistrationCoordinator::default();
    let first_specs = vec![WatchedFileSpec { base: project.path("/first"), pattern: "**/*.sol" }];
    let first = prepare_watched_file_registration_update(&config, &coordinator, first_specs, false)
        .unwrap();

    assert!(first.desired_specs.is_empty());
    let generation = first.generation;
    let second_specs = vec![WatchedFileSpec { base: project.path("/second"), pattern: "**/*.sol" }];
    assert!(
        prepare_watched_file_registration_update(&config, &coordinator, second_specs, true)
            .is_none()
    );
    assert_eq!(coordinator.generation.load(Ordering::Acquire), generation);
}

#[tokio::test(flavor = "current_thread")]
async fn failed_watched_file_registration_allows_the_same_specs_to_retry() {
    let project = TestProject::new();
    std::fs::create_dir(project.path("/workspace")).unwrap();
    let mut params = project.initialize_params_with_roots(&["/workspace"]);
    params.capabilities.workspace = Some(WorkspaceClientCapabilities {
        did_change_watched_files: Some(DidChangeWatchedFilesClientCapabilities {
            dynamic_registration: Some(true),
            relative_pattern_support: Some(true),
        }),
        ..Default::default()
    });
    let (_, config) = negotiate_capabilities(params);
    let coordinator = Arc::new(WatchedFileRegistrationCoordinator::default());
    let specs = config.watched_file_specs();
    let update =
        prepare_watched_file_registration_update(&config, &coordinator, specs.clone(), false)
            .unwrap();
    let first_generation = update.generation;

    spawn_watched_file_registration_update(&ClientSocket::new_closed(), &coordinator, Some(update));
    let deadline = Instant::now() + ASYNC_TEST_TIMEOUT;
    while coordinator.desired_specs.lock().is_some() && Instant::now() < deadline {
        tokio::task::yield_now().await;
    }
    assert!(coordinator.desired_specs.lock().is_none());

    let retry =
        prepare_watched_file_registration_update(&config, &coordinator, specs, false).unwrap();
    assert!(retry.generation > first_generation);
}

#[tokio::test(flavor = "current_thread")]
async fn discovery_and_analysis_refresh_bounded_watched_file_specs() {
    let project = TestProject::from_fixture(
        r#"
        //- /repo/foundry.toml
        [profile.default]
        src = "../shared"

        //- /repo/workspace/.keep
        "#,
    );
    let mut params = project.initialize_params_with_roots(&["/repo/workspace"]);
    params.capabilities.workspace = Some(WorkspaceClientCapabilities {
        did_change_watched_files: Some(DidChangeWatchedFilesClientCapabilities {
            dynamic_registration: Some(true),
            relative_pattern_support: Some(true),
        }),
        ..Default::default()
    });
    let (_, config) = negotiate_capabilities(params);
    let discovery = config.discover_workspaces(&IndexingCancellation::default()).unwrap();
    let (server_main, client_socket) = async_lsp::MainLoop::new_server(|_| {
        let mut router = Router::new(());
        router.notification::<notification::Exit>(|_, ()| ControlFlow::Break(Ok(())));
        router
    });
    let (events_tx, mut events_rx) = mpsc::unbounded_channel();
    let (client_main, server_socket) = async_lsp::MainLoop::new_client(move |_| {
        let mut router = Router::new(events_tx);
        router.request::<request::RegisterCapability, _>(|events, params| {
            events.send(WatchedFileClientEvent::Register(params)).unwrap();
            async { Ok(()) }
        });
        router.request::<request::UnregisterCapability, _>(|events, params| {
            events.send(WatchedFileClientEvent::Unregister(params)).unwrap();
            async { Ok(()) }
        });
        router
    });
    let (server_stream, client_stream) = tokio::io::duplex(64 << 10);
    let (server_rx, server_tx) = tokio::io::split(server_stream);
    let server_task =
        tokio::spawn(server_main.run_buffered(server_rx.compat(), server_tx.compat_write()));
    let (client_rx, client_tx) = tokio::io::split(client_stream);
    let client_task =
        tokio::spawn(client_main.run_buffered(client_rx.compat(), client_tx.compat_write()));

    let mut state = GlobalState::new(client_socket);
    state.config = Arc::new(config);
    let (version, progress) = state
        .begin_analysis(AnalysisMode::Rediscover, Vec::new(), Vec::new(), AnalysisTrigger::External)
        .unwrap();

    assert!(matches!(
        state.on_workspace_discovery_ready(WorkspaceDiscoveryReady {
            version,
            result: discovery,
            disk_paths: Vec::new(),
            progress,
            cancellation: IndexingCancellation::default(),
        }),
        ControlFlow::Continue(())
    ));
    let discovered_specs = state.watched_file_registration.desired_specs.lock().clone().unwrap();
    assert!(
        discovered_specs
            .iter()
            .any(|spec| { spec.base == project.path("/repo") && spec.pattern == "foundry.toml" })
    );
    assert!(
        discovered_specs
            .iter()
            .any(|spec| { spec.base == project.path("/shared") && spec.pattern == "**/*.sol" })
    );
    assert!(matches!(
        next_watched_file_client_event(&mut events_rx).await,
        WatchedFileClientEvent::Unregister(params)
            if params.unregisterations[0].id == "solar-watched-files"
    ));
    let WatchedFileClientEvent::Register(discovered_registration) =
        next_watched_file_client_event(&mut events_rx).await
    else {
        panic!("expected discovered watched-file registration")
    };
    assert!(watched_file_registration_has_spec(
        &discovered_registration,
        &project.path("/repo"),
        "foundry.toml"
    ));
    assert!(watched_file_registration_has_spec(
        &discovered_registration,
        &project.path("/shared"),
        "**/*.sol"
    ));

    state.analysis_scheduler.tasks.lock().cancel();
    let dependency_parent = project.path("/outside");
    let output = AnalysisOutput {
        result: AnalysisResult {
            analyzed_documents: AnalyzedDocuments::default(),
            diagnostics: DiagnosticMap::default(),
            symbol_tables: SymbolTables::default(),
        },
        analysis_paths: AnalysisPathIndex {
            resolved_dependencies: FxHashSet::from_iter([dependency_parent.join("Dependency.sol")]),
            ..Default::default()
        },
    };
    assert!(state.snapshot().publish_analysis_output(version, output));
    let published_specs = state.watched_file_registration.desired_specs.lock().clone().unwrap();
    assert!(
        published_specs
            .iter()
            .any(|spec| { spec.base == dependency_parent && spec.pattern == "**/*.sol" })
    );
    assert!(matches!(
        next_watched_file_client_event(&mut events_rx).await,
        WatchedFileClientEvent::Unregister(_)
    ));
    let WatchedFileClientEvent::Register(published_registration) =
        next_watched_file_client_event(&mut events_rx).await
    else {
        panic!("expected analysis watched-file registration")
    };
    assert!(watched_file_registration_has_spec(
        &published_registration,
        &dependency_parent,
        "**/*.sol"
    ));

    state.clear_analysis_cache();
    let cleared_specs = state.watched_file_registration.desired_specs.lock().clone().unwrap();
    assert!(!cleared_specs.iter().any(|spec| spec.base == dependency_parent));
    assert!(matches!(
        next_watched_file_client_event(&mut events_rx).await,
        WatchedFileClientEvent::Unregister(_)
    ));
    let WatchedFileClientEvent::Register(cleared_registration) =
        next_watched_file_client_event(&mut events_rx).await
    else {
        panic!("expected cache-clear watched-file registration")
    };
    assert!(!watched_file_registration_has_spec(
        &cleared_registration,
        &dependency_parent,
        "**/*.sol"
    ));

    server_socket.notify::<notification::Exit>(()).unwrap();
    assert!(server_task.await.unwrap().is_ok());
    assert!(matches!(client_task.await.unwrap(), Err(async_lsp::Error::Eof)));
}

#[test]
fn did_open_tracks_the_source_until_analysis_publishes() {
    let project = TestProject::from_fixture(
        r#"
        //- /Request.sol
        contract Request {}
        "#,
    );
    let path = project.path("/Request.sol");
    let uri = Url::from_file_path(&path).unwrap();
    let contents = project.read_file("/Request.sol");

    assert_source_notification_tracks_until_publish(&project, &path, false, |state| {
        let result = crate::handlers::did_open_text_document(
            state,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem::new(uri, "solidity".into(), 1, contents),
            },
        );
        assert!(matches!(result, ControlFlow::Continue(())));
    });
}

#[test]
fn did_close_tracks_the_source_until_analysis_publishes() {
    let project = TestProject::from_fixture(
        r#"
        //- /Request.sol open
        contract Request {}
        "#,
    );
    let path = project.path("/Request.sol");
    let uri = Url::from_file_path(&path).unwrap();

    assert_source_notification_tracks_until_publish(&project, &path, false, |state| {
        let result = crate::handlers::did_close_text_document(
            state,
            DidCloseTextDocumentParams { text_document: TextDocumentIdentifier::new(uri) },
        );
        assert!(matches!(result, ControlFlow::Continue(())));
    });
}

#[test]
fn watched_solidity_change_tracks_the_source_until_analysis_publishes() {
    let project = TestProject::from_fixture(
        r#"
        //- /Request.sol
        contract Request {}
        "#,
    );
    let path = project.path("/Request.sol");
    let uri = Url::from_file_path(&path).unwrap();

    assert_source_notification_tracks_until_publish(&project, &path, true, |state| {
        let result = crate::handlers::did_change_watched_files(
            state,
            DidChangeWatchedFilesParams {
                changes: vec![FileEvent { uri, typ: FileChangeType::CHANGED }],
            },
        );
        assert!(matches!(result, ControlFlow::Continue(())));
    });
}

#[test]
fn watched_manifest_change_tracks_external_refresh_until_analysis_publishes() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"

        //- /src/Request.sol
        contract Request {}
        "#,
    );
    let uri = Url::from_file_path(project.path("/foundry.toml")).unwrap();

    assert_external_context_notification_until_publish(&project, |state| {
        let result = crate::handlers::did_change_watched_files(
            state,
            DidChangeWatchedFilesParams {
                changes: vec![FileEvent { uri, typ: FileChangeType::CHANGED }],
            },
        );
        assert!(matches!(result, ControlFlow::Continue(())));
    });
}

#[test]
fn workspace_folder_change_tracks_external_refresh_until_analysis_publishes() {
    let project = TestProject::from_fixture(
        r#"
        //- /Request.sol
        contract Request {}
        "#,
    );
    let added_path = project.path("/added");
    std::fs::create_dir(&added_path).unwrap();
    let added =
        WorkspaceFolder { uri: Url::from_file_path(added_path).unwrap(), name: "added".into() };

    assert_external_context_notification_until_publish(&project, |state| {
        let result = crate::handlers::did_change_workspace_folders(
            state,
            DidChangeWorkspaceFoldersParams {
                event: WorkspaceFoldersChangeEvent { added: vec![added], removed: Vec::new() },
            },
        );
        assert!(matches!(result, ControlFlow::Continue(())));
    });
}

#[test]
fn watched_solidity_change_ignores_open_document() {
    let project = TestProject::from_fixture(
        r#"
        //- /Request.sol open
        contract Request {}
        "#,
    );
    let path = project.path("/Request.sol");
    let uri = Url::from_file_path(path).unwrap();
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(project.config());
    state.vfs = Arc::new(RwLock::new(project.vfs()));
    let version = state.analysis_version.load(Ordering::Acquire);

    let result = crate::handlers::did_change_watched_files(
        &mut state,
        DidChangeWatchedFilesParams {
            changes: vec![FileEvent { uri, typ: FileChangeType::CHANGED }],
        },
    );

    assert!(matches!(result, ControlFlow::Continue(())));
    assert_eq!(state.analysis_version.load(Ordering::Acquire), version);
    assert!(state.analysis_scheduler.tasks.lock().coordinator.is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn watched_solidity_change_ignores_unrelated_excluded_file() {
    let project = TestProject::from_fixture(
        r#"
        //- /Main.sol
        contract Main {}

        //- /generated/Unrelated.sol
        contract Unrelated {}
        "#,
    );
    let path = project.path("/generated/Unrelated.sol");
    let uri = Url::from_file_path(path).unwrap();
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(config_with_indexing_excludes(&project, &["generated/**"]));
    let version = state.analysis_version.load(Ordering::Acquire);

    let result = crate::handlers::did_change_watched_files(
        &mut state,
        DidChangeWatchedFilesParams {
            changes: vec![FileEvent { uri, typ: FileChangeType::CHANGED }],
        },
    );

    assert!(matches!(result, ControlFlow::Continue(())));
    assert_eq!(state.analysis_version.load(Ordering::Acquire), version);
    assert!(state.analysis_scheduler.tasks.lock().coordinator.is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn watched_excluded_manifest_events_do_not_schedule_analysis() {
    let project = TestProject::from_fixture(
        r#"
        //- /Main.sol
        contract Main {}

        //- /node_modules/package/foundry.toml
        [profile.default]
        src = "src"
        "#,
    );

    for typ in [FileChangeType::CREATED, FileChangeType::CHANGED, FileChangeType::DELETED] {
        let mut state = GlobalState::new(ClientSocket::new_closed());
        state.config = Arc::new(project.config());
        let version = state.analysis_version.load(Ordering::Acquire);
        let uri = Url::from_file_path(project.path("/node_modules/package/foundry.toml")).unwrap();

        let result = crate::handlers::did_change_watched_files(
            &mut state,
            DidChangeWatchedFilesParams { changes: vec![FileEvent { uri, typ }] },
        );

        assert!(matches!(result, ControlFlow::Continue(())));
        assert_eq!(state.analysis_version.load(Ordering::Acquire), version);
        assert!(state.analysis_scheduler.tasks.lock().coordinator.is_none());
    }
}

#[tokio::test(flavor = "current_thread")]
async fn watched_nested_manifest_create_discovers_the_project() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml

        //- /packages/app/foundry.toml

        //- /packages/app/generated/.keep
        "#,
    );
    let mut params = project.initialize_params();
    params.initialization_options = Some(serde_json::json!({
        "indexing": { "exclude": ["packages/app/generated/**"] }
    }));
    let (_, mut config) = negotiate_capabilities(params);
    config.rediscover_workspaces();
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(config);
    project.write_file(
        "/packages/app/generated/foundry.toml",
        r#"
        [profile.default]
        src = "src"
        "#,
    );
    project.write_file("/packages/app/generated/src/Nested.sol", "contract Nested {}");
    let uri = Url::from_file_path(project.path("/packages/app/generated/foundry.toml")).unwrap();

    let result = crate::handlers::did_change_watched_files(
        &mut state,
        DidChangeWatchedFilesParams {
            changes: vec![FileEvent { uri, typ: FileChangeType::CREATED }],
        },
    );

    assert!(matches!(result, ControlFlow::Continue(())));
    let tables = tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
        .await
        .expect("nested manifest analysis should finish")
        .unwrap();
    assert!(tables.read().workspace_symbols("Nested").iter().any(|symbol| symbol.name == "Nested"));
}

#[tokio::test(flavor = "current_thread")]
async fn created_directory_below_default_exclude_does_not_schedule_analysis() {
    let project = TestProject::from_fixture(
        r#"
        //- /Main.sol
        contract Main {}
        "#,
    );
    let path = project.path("/node_modules/package/generated");
    std::fs::create_dir_all(&path).unwrap();
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(project.config());
    let version = state.analysis_version.load(Ordering::Acquire);

    let result = crate::handlers::did_create_files(
        &mut state,
        CreateFilesParams {
            files: vec![FileCreate { uri: Url::from_file_path(path).unwrap().to_string() }],
        },
    );

    assert!(matches!(result, ControlFlow::Continue(())));
    let actual_version = state.analysis_version.load(Ordering::Acquire);
    state.analysis_scheduler.tasks.lock().cancel();
    assert_eq!(actual_version, version);
}

#[tokio::test(flavor = "current_thread")]
async fn watched_excluded_dependency_change_and_delete_schedule_analysis() {
    for typ in [FileChangeType::CHANGED, FileChangeType::DELETED] {
        let project = TestProject::from_fixture(
            r#"
            //- /Main.sol
            import "./generated/Dependency.sol";
            contract Main is Dependency {}

            //- /generated/Dependency.sol
            contract Dependency {}
            "#,
        );
        let config = config_with_indexing_excludes(&project, &["generated/**"]);
        let mut batches =
            snapshot_with_config(config.clone(), project.vfs()).analysis_batches(Vec::new());
        let output =
            analyze_cancellable(batches.pop().unwrap(), &IndexingCancellation::default()).unwrap();
        let mut state = GlobalState::new(ClientSocket::new_closed());
        state.config = Arc::new(config);
        state.snapshot().publish_analysis_output(0, output);
        let path = project.path("/generated/Dependency.sol");
        let uri = Url::from_file_path(path).unwrap();

        let result = crate::handlers::did_change_watched_files(
            &mut state,
            DidChangeWatchedFilesParams { changes: vec![FileEvent { uri, typ }] },
        );

        assert!(matches!(result, ControlFlow::Continue(())));
        assert_eq!(state.analysis_version.load(Ordering::Acquire), 1);
        state.analysis_scheduler.tasks.lock().cancel();
    }
}

#[tokio::test(flavor = "current_thread")]
async fn watched_external_source_create_change_and_delete_schedule_analysis() {
    for typ in [FileChangeType::CREATED, FileChangeType::CHANGED, FileChangeType::DELETED] {
        let project = TestProject::from_fixture(
            r#"
            //- /project/foundry.toml
            [profile.default]
            src = "../shared"
            "#,
        );
        let path = project.path("/shared/External.sol");
        if typ != FileChangeType::CREATED {
            project.write_file("/shared/External.sol", "contract External {}");
        }
        let config = project.config_with_roots(&["/project"]);
        if typ == FileChangeType::CREATED {
            project.write_file("/shared/External.sol", "contract External {}");
        } else if typ == FileChangeType::DELETED {
            std::fs::remove_file(&path).unwrap();
        }
        let mut state = GlobalState::new(ClientSocket::new_closed());
        state.config = Arc::new(config);

        assert!(matches!(
            crate::handlers::did_change_watched_files(
                &mut state,
                DidChangeWatchedFilesParams {
                    changes: vec![FileEvent { uri: Url::from_file_path(&path).unwrap(), typ }],
                },
            ),
            ControlFlow::Continue(())
        ));

        assert_eq!(state.analysis_version.load(Ordering::Acquire), 1);
        let tracked = state.config.tracked_source_files_under(&[project.path("/shared")]);
        if typ == FileChangeType::DELETED {
            assert!(tracked.is_empty());
        } else {
            assert_eq!(tracked, [path]);
        }
        state.analysis_scheduler.tasks.lock().cancel();
    }
}

#[tokio::test(flavor = "current_thread")]
async fn unknown_dependency_event_is_deferred_while_analysis_is_pending() {
    let project = TestProject::from_fixture(
        r#"
        //- /Main.sol
        contract Main {}

        //- /generated/Dependency.sol
        contract Dependency {}
        "#,
    );
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(config_with_indexing_excludes(&project, &["generated/**"]));
    state.mark_analysis_pending_for_test();
    let path = project.path("/generated/Dependency.sol");

    let result = crate::handlers::did_change_watched_files(
        &mut state,
        DidChangeWatchedFilesParams {
            changes: vec![FileEvent {
                uri: Url::from_file_path(&path).unwrap(),
                typ: FileChangeType::CHANGED,
            }],
        },
    );

    assert!(matches!(result, ControlFlow::Continue(())));
    assert_eq!(state.analysis_version.load(Ordering::Acquire), 1);
    assert_eq!(
        state.analysis_commit.lock().deferred_source_file_events.get(&path),
        Some(&FileChangeType::CHANGED)
    );
}

#[test]
fn did_create_defers_a_candidate_first_learned_by_pending_analysis() {
    let project = TestProject::from_fixture(
        r#"
        //- /Main.sol
        import "./generated/Dependency.sol";
        contract Main is Dependency {}
        "#,
    );
    let config = config_with_indexing_excludes(&project, &["generated/**"]);
    let mut batches =
        snapshot_with_config(config.clone(), project.vfs()).analysis_batches(Vec::new());
    let output =
        analyze_cancellable(batches.pop().unwrap(), &IndexingCancellation::default()).unwrap();
    let path = project.path("/generated/Dependency.sol");
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(config);
    state.mark_analysis_pending_for_test();
    let version = state.analysis_version.load(Ordering::Acquire);
    project.write_file("/generated/Dependency.sol", "contract Dependency {}");

    assert!(matches!(
        crate::handlers::did_create_files(
            &mut state,
            CreateFilesParams {
                files: vec![FileCreate { uri: Url::from_file_path(&path).unwrap().to_string() }],
            },
        ),
        ControlFlow::Continue(())
    ));
    assert_eq!(state.analysis_version.load(Ordering::Acquire), version);
    assert_eq!(
        state.analysis_commit.lock().deferred_source_file_events.get(&path),
        Some(&FileChangeType::CREATED)
    );
    assert!(!state.snapshot().publish_analysis_output(version, output));
}

#[test]
fn did_delete_defers_a_dependency_first_learned_by_pending_analysis() {
    let project = TestProject::from_fixture(
        r#"
        //- /Main.sol
        import "./generated/Dependency.sol";
        contract Main is Dependency {}

        //- /generated/Dependency.sol
        contract Dependency {}
        "#,
    );
    let config = config_with_indexing_excludes(&project, &["generated/**"]);
    let mut batches =
        snapshot_with_config(config.clone(), project.vfs()).analysis_batches(Vec::new());
    let output =
        analyze_cancellable(batches.pop().unwrap(), &IndexingCancellation::default()).unwrap();
    let path = project.path("/generated/Dependency.sol");
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(config);
    state.mark_analysis_pending_for_test();
    let version = state.analysis_version.load(Ordering::Acquire);
    std::fs::remove_file(&path).unwrap();

    assert!(matches!(
        crate::handlers::did_delete_files(
            &mut state,
            DeleteFilesParams {
                files: vec![FileDelete { uri: Url::from_file_path(&path).unwrap().to_string() }],
            },
        ),
        ControlFlow::Continue(())
    ));
    assert_eq!(state.analysis_version.load(Ordering::Acquire), version);
    assert_eq!(
        state.analysis_commit.lock().deferred_source_file_events.get(&path),
        Some(&FileChangeType::DELETED)
    );
    assert!(!state.snapshot().publish_analysis_output(version, output));
}

#[tokio::test(flavor = "current_thread")]
async fn source_events_during_initial_discovery_are_replayed_after_policy_is_known() {
    let project = TestProject::from_fixture(
        r#"
        //- /project/foundry.toml
        [profile.default]
        src = "lib"
        libs = ["vendor"]
        "#,
    );
    let (_, config) = negotiate_capabilities(project.initialize_params_with_roots(&["/project"]));
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(config);
    let (version, progress) = state
        .begin_analysis(AnalysisMode::Rediscover, Vec::new(), Vec::new(), AnalysisTrigger::External)
        .unwrap();
    let discovery = state.config.discover_workspaces(&IndexingCancellation::default()).unwrap();

    project.write_file("/project/lib/Active.sol", "contract Active {}");
    project.write_file("/project/node_modules/Ignored.sol", "contract Ignored {}");
    let result = crate::handlers::did_change_watched_files(
        &mut state,
        DidChangeWatchedFilesParams {
            changes: [
                project.path("/project/lib/Active.sol"),
                project.path("/project/node_modules/Ignored.sol"),
            ]
            .into_iter()
            .map(|path| FileEvent {
                uri: Url::from_file_path(path).unwrap(),
                typ: FileChangeType::CREATED,
            })
            .collect(),
        },
    );

    assert!(matches!(result, ControlFlow::Continue(())));
    assert_eq!(state.analysis_version.load(Ordering::Acquire), version);
    assert!(matches!(
        state.on_workspace_discovery_ready(WorkspaceDiscoveryReady {
            version,
            result: discovery,
            disk_paths: Vec::new(),
            progress,
            cancellation: IndexingCancellation::default(),
        }),
        ControlFlow::Continue(())
    ));

    let tables = tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
        .await
        .expect("initial discovery analysis should finish")
        .unwrap();
    assert!(tables.read().workspace_symbols("Active").iter().any(|symbol| symbol.name == "Active"));
    assert!(tables.read().workspace_symbols("Ignored").is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn source_events_during_discovery_are_deferred_with_existing_workspaces() {
    let project = TestProject::from_fixture(
        r#"
        //- /existing/Existing.sol
        contract Existing {}
        "#,
    );
    let new_root = project.path("/new");
    std::fs::create_dir(&new_root).unwrap();
    let (_, mut config) =
        negotiate_capabilities(project.initialize_params_with_roots(&["/existing"]));
    config.rediscover_workspaces();
    assert!(!config.workspaces().is_empty());
    config.add_workspaces([new_root.clone()]);
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(config);
    let (version, progress) = state
        .begin_analysis(AnalysisMode::Rediscover, Vec::new(), Vec::new(), AnalysisTrigger::External)
        .unwrap();
    let discovery = state.config.discover_workspaces(&IndexingCancellation::default()).unwrap();
    let path = project.path("/new/Active.sol");
    project.write_file("/new/Active.sol", "contract Active {}");

    assert!(matches!(
        crate::handlers::did_change_watched_files(
            &mut state,
            DidChangeWatchedFilesParams {
                changes: vec![
                    FileEvent {
                        uri: Url::from_file_path(&path).unwrap(),
                        typ: FileChangeType::CREATED,
                    },
                    FileEvent {
                        uri: Url::from_file_path(&path).unwrap(),
                        typ: FileChangeType::CHANGED,
                    },
                ],
            },
        ),
        ControlFlow::Continue(())
    ));
    assert_eq!(state.analysis_version.load(Ordering::Acquire), version);
    assert_eq!(
        state.analysis_commit.lock().deferred_source_file_events.get(&path),
        Some(&FileChangeType::CHANGED)
    );

    assert!(matches!(
        state.on_workspace_discovery_ready(WorkspaceDiscoveryReady {
            version,
            result: discovery,
            disk_paths: Vec::new(),
            progress,
            cancellation: IndexingCancellation::default(),
        }),
        ControlFlow::Continue(())
    ));
    assert_eq!(
        state.config.tracked_source_files_under(std::slice::from_ref(&new_root)),
        std::slice::from_ref(&path)
    );
    let tables = tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
        .await
        .expect("rediscovered source analysis should finish")
        .unwrap();
    assert!(tables.read().workspace_symbols("Active").iter().any(|symbol| symbol.name == "Active"));
    drop(tables);

    state.recompute_after_source_changes(vec![project.path("/existing/Existing.sol")]);
    let tables = tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
        .await
        .expect("subsequent analysis should finish")
        .unwrap();
    assert!(tables.read().workspace_symbols("Active").iter().any(|symbol| symbol.name == "Active"));
}

#[tokio::test(flavor = "current_thread")]
async fn watched_existing_unresolved_candidate_change_and_delete_schedule_analysis() {
    for typ in [FileChangeType::CHANGED, FileChangeType::DELETED] {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [profile.default]
            src = "src"
            libs = ["lib-one", "lib-two"]

            //- /src/Main.sol
            import "Dependency.sol";
            contract Main is Dependency {}

            //- /lib-one/Dependency.sol
            contract Dependency {}

            //- /lib-two/Dependency.sol
            contract Dependency {}
            "#,
        );
        let config = project.config();
        let mut batches =
            snapshot_with_config(config.clone(), project.vfs()).analysis_batches(Vec::new());
        let output =
            analyze_cancellable(batches.pop().unwrap(), &IndexingCancellation::default()).unwrap();
        let path = project.path("/lib-one/Dependency.sol");
        match typ {
            FileChangeType::CHANGED => {
                std::fs::write(&path, "contract Dependency { uint x; }").unwrap()
            }
            FileChangeType::DELETED => std::fs::remove_file(&path).unwrap(),
            _ => unreachable!(),
        }
        let uri = Url::from_file_path(path).unwrap();
        let mut state = GlobalState::new(ClientSocket::new_closed());
        state.config = Arc::new(config);
        state.snapshot().publish_analysis_output(0, output);

        let result = crate::handlers::did_change_watched_files(
            &mut state,
            DidChangeWatchedFilesParams { changes: vec![FileEvent { uri, typ }] },
        );

        assert!(matches!(result, ControlFlow::Continue(())));
        let actual_version = state.analysis_version.load(Ordering::Acquire);
        state.analysis_scheduler.tasks.lock().cancel();
        assert_eq!(actual_version, 1);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn watched_missing_excluded_dependency_recovers_only_on_create() {
    let project = TestProject::from_fixture(
        r#"
        //- /Main.sol
        import "./generated/Missing.sol";
        contract Main is Missing {}
        "#,
    );
    let config = config_with_indexing_excludes(&project, &["generated/**"]);
    let mut batches =
        snapshot_with_config(config.clone(), project.vfs()).analysis_batches(Vec::new());
    let output =
        analyze_cancellable(batches.pop().unwrap(), &IndexingCancellation::default()).unwrap();
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(config);
    state.snapshot().publish_analysis_output(0, output);
    let path = project.path("/generated/Missing.sol");
    let uri = Url::from_file_path(&path).unwrap();

    for typ in [FileChangeType::CHANGED, FileChangeType::DELETED] {
        let result = crate::handlers::did_change_watched_files(
            &mut state,
            DidChangeWatchedFilesParams { changes: vec![FileEvent { uri: uri.clone(), typ }] },
        );
        assert!(matches!(result, ControlFlow::Continue(())));
        assert_eq!(state.analysis_version.load(Ordering::Acquire), 0);
    }

    project.write_file("/generated/Missing.sol", "contract Missing {}");
    let result = crate::handlers::did_change_watched_files(
        &mut state,
        DidChangeWatchedFilesParams {
            changes: vec![FileEvent { uri, typ: FileChangeType::CREATED }],
        },
    );
    assert!(matches!(result, ControlFlow::Continue(())));
    assert_eq!(state.analysis_version.load(Ordering::Acquire), 1);

    let tables = tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
        .await
        .expect("created import candidate should be analyzed")
        .unwrap();
    assert!(
        tables.read().workspace_symbols("Missing").iter().any(|symbol| symbol.name == "Missing")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn watched_missing_candidate_change_supersedes_pending_create_analysis() {
    let project = TestProject::from_fixture(
        r#"
        //- /Main.sol
        import "./generated/Missing.sol";
        contract Main is Missing {}
        "#,
    );
    let config = config_with_indexing_excludes(&project, &["generated/**"]);
    let mut batches =
        snapshot_with_config(config.clone(), project.vfs()).analysis_batches(Vec::new());
    let output =
        analyze_cancellable(batches.pop().unwrap(), &IndexingCancellation::default()).unwrap();
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(config);
    state.snapshot().publish_analysis_output(0, output);
    let path = project.path("/generated/Missing.sol");
    let uri = Url::from_file_path(&path).unwrap();

    project.write_file("/generated/Missing.sol", "contract Missing {}");
    assert!(matches!(
        crate::handlers::did_change_watched_files(
            &mut state,
            DidChangeWatchedFilesParams {
                changes: vec![FileEvent { uri: uri.clone(), typ: FileChangeType::CREATED }],
            },
        ),
        ControlFlow::Continue(())
    ));
    assert_eq!(state.analysis_version.load(Ordering::Acquire), 1);

    project.write_file("/generated/Missing.sol", "contract Missing { uint latest; }");
    assert!(matches!(
        crate::handlers::did_change_watched_files(
            &mut state,
            DidChangeWatchedFilesParams {
                changes: vec![FileEvent { uri, typ: FileChangeType::CHANGED }],
            },
        ),
        ControlFlow::Continue(())
    ));
    assert_eq!(state.analysis_version.load(Ordering::Acquire), 2);

    tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
        .await
        .expect("replacement dependency analysis should finish")
        .unwrap();
}

#[test]
fn did_change_tracks_the_request_source_until_analysis_publishes() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .unwrap();
    runtime.block_on(async {
        let project = TestProject::from_fixture(
            r#"
            //- /Request.sol open
            contract Before {}

            //- /Other.sol open
            contract Other {}
            "#,
        );
        let mut batches = snapshot(&project).analysis_batches(Vec::new());
        let old_result = analyze(batches.pop().unwrap());
        assert!(old_result.diagnostics.is_empty(), "{:#?}", old_result.diagnostics);
        assert!(batches.is_empty());

        let request_path = project.path("/Request.sol");
        let request_uri = Url::from_file_path(&request_path).unwrap();
        let other_uri = Url::from_file_path(project.path("/Other.sol")).unwrap();
        let mut state = GlobalState::new(ClientSocket::new_closed());
        state.config = Arc::new(project.config());
        state.vfs = Arc::new(RwLock::new(project.vfs()));
        *state.symbol_tables.write() = old_result.symbol_tables;
        let (release_worker, worker) = pause_blocking_pool();

        let result = crate::handlers::did_change_text_document(
            &mut state,
            DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier::new(request_uri.clone(), 1),
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: "contract After {}".into(),
                }],
            },
        );

        assert!(matches!(result, ControlFlow::Continue(())));
        assert!(state.analysis_commit.lock().external_refresh.is_none());
        assert!(
            state.analysis_commit.lock().natspec_pending_source_changes.contains(&request_path)
        );
        assert!(state.natspec_semantics_are_usable(&request_uri));
        assert!(!state.natspec_semantics_are_usable(&other_uri));

        release_worker.send(()).unwrap();
        worker.await.unwrap();
        let tables = tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
            .await
            .expect("changed-source analysis should finish")
            .unwrap();
        let tables = tables.read();
        assert!(tables.workspace_symbols("Before").is_empty());
        assert!(tables.workspace_symbols("After").iter().any(|symbol| symbol.name == "After"));
        drop(tables);
        assert!(state.analysis_commit.lock().natspec_pending_source_changes.is_empty());
        assert!(state.natspec_semantics_are_usable(&other_uri));
    });
}

#[test]
fn configuration_change_invalidates_natspec_context_until_analysis_publishes() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .unwrap();
    runtime.block_on(async {
        let mut state = GlobalState::new(ClientSocket::new_closed());
        let request_uri = Url::parse("file:///Request.sol").unwrap();
        let (release_worker, worker) = pause_blocking_pool();

        let result = crate::handlers::did_change_configuration(
            &mut state,
            DidChangeConfigurationParams { settings: serde_json::Value::Null },
        );

        assert!(matches!(result, ControlFlow::Continue(())));
        assert!(state.analysis_commit.lock().external_refresh.is_some());
        assert!(!state.natspec_semantics_are_usable(&request_uri));

        release_worker.send(()).unwrap();
        worker.await.unwrap();
        tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
            .await
            .expect("configuration-change analysis should finish")
            .unwrap();
        assert!(state.natspec_semantics_are_usable(&request_uri));
    });
}

#[tokio::test(flavor = "current_thread")]
async fn source_change_debounce_does_not_count_toward_progress_delay() {
    let project = TestProject::from_fixture(
        r#"
        //- /Request.sol open
        contract Before {}
        "#,
    );
    let uri = Url::from_file_path(project.path("/Request.sol")).unwrap();
    let mut harness = work_done_harness();
    let client = harness.client.clone();
    let mut state = GlobalState::new(client.clone());
    state.config = Arc::new(project.config());
    state.vfs = Arc::new(RwLock::new(project.vfs()));
    state.analysis_progress =
        ProgressCoordinator::with_timing(client, true, Duration::ZERO, Duration::from_secs(1));

    let result = crate::handlers::did_change_text_document(
        &mut state,
        DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier::new(uri, 1),
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: "contract After {}".into(),
            }],
        },
    );
    assert!(matches!(result, ControlFlow::Continue(())));

    tokio::time::sleep(state.config.source_change_debounce() / 2).await;
    harness.probe().await;
    assert!(matches!(harness.events.try_recv(), Err(mpsc::error::TryRecvError::Empty)));

    state.clear_analysis_cache();
    harness.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn rapid_did_changes_debounce_to_the_latest_source() {
    let project = TestProject::from_fixture(
        r#"
        //- /Request.sol open
        contract Before {}
        "#,
    );
    let path = project.path("/Request.sol");
    let uri = Url::from_file_path(&path).unwrap();
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(project.config());
    state.vfs = Arc::new(RwLock::new(project.vfs()));

    let result = crate::handlers::did_change_text_document(
        &mut state,
        DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier::new(uri.clone(), 1),
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: "contract Intermediate {}".into(),
            }],
        },
    );
    assert!(matches!(result, ControlFlow::Continue(())));
    let first_coordinator =
        state.analysis_scheduler.tasks.lock().coordinator.as_ref().unwrap().1.clone();

    let result = crate::handlers::did_change_text_document(
        &mut state,
        DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier::new(uri, 2),
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: "contract Latest {}".into(),
            }],
        },
    );
    assert!(matches!(result, ControlFlow::Continue(())));
    let debounce = state.config.source_change_debounce();

    assert!(
        tokio::time::timeout(debounce / 2, state.latest_analysis()).await.is_err(),
        "analysis should remain pending during the debounce window"
    );
    tokio::time::timeout(ASYNC_TEST_TIMEOUT, async {
        while !first_coordinator.is_finished() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("replacement should cancel the first coordinator");

    let tables = tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
        .await
        .expect("latest source analysis should finish")
        .unwrap();
    let tables = tables.read();
    assert!(tables.workspace_symbols("Intermediate").is_empty());
    assert!(tables.workspace_symbols("Latest").iter().any(|symbol| symbol.name == "Latest"));
}

#[test]
fn pending_request_source_defers_to_target_specific_natspec_lookup() {
    let project = TestProject::new();
    let state = GlobalState::new(ClientSocket::new_closed());
    let path = project.path("/Request.sol");
    let uri = Url::from_file_path(&path).unwrap();
    let equivalent_uri =
        Url::parse(&uri.as_str().replacen("Request.sol", "%52equest.sol", 1)).unwrap();

    state.mark_source_analysis_pending_for_test(path);

    assert_ne!(uri, equivalent_uri);
    assert_eq!(uri.to_file_path(), equivalent_uri.to_file_path());
    assert!(state.natspec_semantics_are_usable(&equivalent_uri));
}

#[test]
fn pending_other_source_does_not_reuse_unknown_natspec_semantics() {
    let project = TestProject::new();
    let state = GlobalState::new(ClientSocket::new_closed());
    let request_uri = Url::from_file_path(project.path("/Request.sol")).unwrap();

    state.mark_source_analysis_pending_for_test(project.path("/Missing.sol"));

    assert!(!state.natspec_semantics_are_usable(&request_uri));
}

#[test]
fn pending_context_change_invalidates_natspec_semantics() {
    let state = GlobalState::new(ClientSocket::new_closed());
    let uri = Url::parse("file:///Request.sol").unwrap();
    state.mark_context_analysis_pending_for_test();

    assert!(!state.natspec_semantics_are_usable(&uri));
}

#[test]
fn publishing_current_epoch_clears_pending_source_changes() {
    let project = TestProject::new();
    let mut snapshot = snapshot(&project);
    let first_path = project.path("/First.sol");
    let second_path = project.path("/Second.sol");
    snapshot
        .analysis_commit
        .lock()
        .natspec_pending_source_changes
        .extend([first_path, second_path]);

    assert!(snapshot.publish_symbol_tables(1, SymbolTables::default()));

    let commit = snapshot.analysis_commit.lock();
    assert_eq!(commit.natspec_symbol_tables_version, 1);
    assert!(commit.natspec_pending_source_changes.is_empty());
}

#[test]
fn beginning_analysis_epoch_waits_for_analysis_commit() {
    let state = GlobalState::new(ClientSocket::new_closed());
    let commit = state.analysis_commit.lock();
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (finished_tx, finished_rx) = std::sync::mpsc::channel();

    std::thread::scope(|scope| {
        scope.spawn(|| {
            started_tx.send(()).unwrap();
            state.mark_analysis_pending_for_test();
            finished_tx.send(()).unwrap();
        });
        started_rx.recv().unwrap();
        let finished_while_locked = finished_rx.recv_timeout(Duration::from_millis(100)).is_ok();
        drop(commit);
        if !finished_while_locked {
            finished_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("analysis epoch should begin after commit unlocks");
        }
        assert!(!finished_while_locked, "analysis epoch bypassed analysis commit");
    });
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

#[tokio::test(flavor = "current_thread")]
async fn recomputing_for_removed_files_stales_all_flycheck_owners() {
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
    let mut snapshot = state.snapshot();
    let first_owner = flycheck_owner(project.path("/first"));
    let second_owner = flycheck_owner(project.path("/second"));

    assert!(snapshot.is_current_flycheck(&first_owner, 0));
    assert!(snapshot.is_current_flycheck(&second_owner, 0));

    let deleted_path = project.path("/first/src/Deleted.sol");
    let uri = Url::from_file_path(&deleted_path).unwrap();
    snapshot.publish_flycheck_diagnostics(
        second_owner.clone(),
        0,
        DiagnosticMap::from_iter([(uri.clone(), vec![diagnostic("existing")])]),
    );
    assert!(matches!(
        state.diagnostics.read().pull_report(&uri, None),
        PullReport::Full { diagnostics, .. } if diagnostics == vec![diagnostic("existing")]
    ));

    state.recompute_for_file_changes(vec![deleted_path.clone()], vec![deleted_path.clone()], false);

    assert!(!snapshot.is_current_flycheck(&first_owner, 0));
    assert!(!snapshot.is_current_flycheck(&second_owner, 0));
    assert!(matches!(
        state.diagnostics.read().pull_report(&uri, None),
        PullReport::Full { diagnostics, .. } if diagnostics.is_empty()
    ));

    snapshot.publish_flycheck_diagnostics(
        first_owner.clone(),
        0,
        DiagnosticMap::from_iter([(uri.clone(), vec![diagnostic("stale")])]),
    );
    snapshot.publish_flycheck_diagnostics(
        second_owner,
        0,
        DiagnosticMap::from_iter([(uri.clone(), vec![diagnostic("stale cross-workspace")])]),
    );
    assert!(matches!(
        state.diagnostics.read().pull_report(&uri, None),
        PullReport::Full { diagnostics, .. } if diagnostics.is_empty()
    ));
    tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
        .await
        .expect("file-change analysis should finish")
        .unwrap();
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

    assert_eq!(batch.files, vec![(path, "contract C { function f() public { number+; } }".into())]);
}

#[test]
fn goto_implementation_finds_unopened_naked_workspace_files() {
    let marked = MarkedProject::from_fixture(
        r#"
        //- /Base.sol open
        interface Runner {
            function $1run(uint256 input) external returns (uint256);
        }

        //- /First.sol
        import {Runner} from "./Base.sol";

        contract First is Runner {
            function run(uint256 input) external pure override returns (uint256) {
                return input + 1;
            }
        }

        //- /Second.sol
        import {Runner} from "./Base.sol";

        contract Second is Runner {
            function run(uint256 input) external pure override returns (uint256) {
                return input + 2;
            }
        }
        "#,
    );
    let snapshot = snapshot(marked.project());
    let mut results = AnalysisResultAccumulator::default();
    for batch in snapshot.analysis_batches(Vec::new()) {
        let result = analyze(batch);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        results.push(result);
    }
    let symbol_tables = results.finish().symbol_tables;

    let marker = marked.marker("$1");
    let uri = Url::from_file_path(marked.project().path(marker.path())).unwrap();
    let Some(lsp_types::GotoDefinitionResponse::Array(locations)) =
        symbol_tables.goto_implementation(&uri, marker.position())
    else {
        panic!("expected implementation locations");
    };
    let paths = locations
        .into_iter()
        .map(|location| {
            location.uri.to_file_path().unwrap().file_name().unwrap().to_str().unwrap().to_owned()
        })
        .collect::<Vec<_>>();

    assert_eq!(paths, ["First.sol", "Second.sol"]);
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
fn analysis_batches_include_created_naked_workspace_disk_files() {
    let project = TestProject::from_fixture(
        r#"
        //- /Open.sol open
        contract Open { function f() public { number+; } }
        "#,
    );
    let mut config = project.config();
    project.write_file("/Disk.sol", "contract Disk {}");
    let disk_path = project.path("/Disk.sol");
    let open_path = project.path("/Open.sol");
    config.add_source_file(disk_path.clone());
    let snapshot = snapshot_with_config(config, project.vfs());

    let mut batches = snapshot.analysis_batches(vec![disk_path.clone()]);
    let batch = batches.pop().unwrap();

    assert_eq!(
        batch.files,
        vec![
            (disk_path, "contract Disk {}".into()),
            (open_path, "contract Open { function f() public { number+; } }".into()),
        ]
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
fn document_links_use_vfs_overlay() {
    let mut project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"

        //- /src/A.sol
        import "./Old.sol";

        //- /src/Old.sol
        contract Old {}

        //- /src/New.sol
        contract New {}
        "#,
    );
    project.open_file("/src/A.sol", "import \"./New.sol\";");
    let snapshot = snapshot(&project);

    let mut batches = snapshot.analysis_batches(Vec::new());
    let result = analyze(batches.pop().unwrap());

    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let path = project.path("/src/A.sol");
    let links = result.symbol_tables.document_links(&path);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target, Some(Url::from_file_path(project.path("/src/New.sol")).unwrap()));
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
fn workspace_discovery_rechecks_sources_against_the_owning_workspace_policy() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "."

        //- /nested/foundry.toml
        [profile.default]
        src = "src"
        libs = ["src/vendor"]

        //- /nested/src/Included.sol
        contract Included {}

        //- /nested/src/generated/Excluded.sol
        contract Excluded {}

        //- /nested/src/vendor/Dependency.sol
        contract Dependency {}

        //- /nested/Outside.sol
        contract Outside {}
        "#,
    );
    let config = config_with_indexing_excludes(&project, &["src/generated/**"]);
    let nested_root = project.path("/nested");
    let outer_workspace = config
        .workspaces()
        .iter()
        .find(|workspace| workspace.compile_opts().base_path.as_deref() == Some(project.root()))
        .unwrap();
    let nested_workspace = config
        .workspaces()
        .iter()
        .find(|workspace| workspace.compile_opts().base_path.as_deref() == Some(&nested_root))
        .unwrap();
    assert!(outer_workspace.source_files().is_empty());
    assert_eq!(nested_workspace.source_files(), [project.path("/nested/src/Included.sol")]);
    assert_eq!(config.index_metrics().eager, 1);

    let batches = snapshot_with_config(config, Vfs::default()).analysis_batches(Vec::new());
    let outer_batch = batches
        .iter()
        .find(|batch| batch.opts.base_path.as_deref() == Some(project.root()))
        .unwrap();
    let nested_batch = batches
        .iter()
        .find(|batch| batch.opts.base_path.as_deref() == Some(nested_root.as_path()))
        .unwrap();

    assert!(outer_batch.files.iter().all(|(path, _)| !path.starts_with(&nested_root)));
    assert_eq!(
        nested_batch.files,
        vec![(project.path("/nested/src/Included.sol"), "contract Included {}".into())]
    );
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
    let path = project.path("/src/A.sol");
    let links = result.symbol_tables.document_links(&path);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target, Some(Url::from_file_path(project.path("/lib/B.sol")).unwrap()));
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
            function f(uint256 y) public view returns (uint256 z) {
                uint256 local = x + y;
                return local;
            }
        }
        enum E { A }
        "#,
    );
    let path = project.path("/Symbols.sol");
    let uri = Url::from_file_path(&path).unwrap();
    let result = analyze(AnalysisBatch::from_files(
        CompileOpts::default(),
        [(path, project.read_file("/Symbols.sol"))],
    ));

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
            function f(uint256 y) public pure returns (uint256 z) {
                uint256 local = y;
                return local;
            }
        }
        "#,
    );
    let path = project.path("/Symbols.sol");
    let uri = Url::from_file_path(&path).unwrap();
    let result = analyze(AnalysisBatch::from_files(
        CompileOpts::default(),
        [(path, project.read_file("/Symbols.sol"))],
    ));

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

fn assert_parent(declarations: &[&crate::symbols::DeclarationSymbol], name: &str, parent: &str) {
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
    symbol.children.as_deref().unwrap_or_default().iter().map(|child| child.name.as_str()).collect()
}

fn find_workspace_symbol<'a>(symbols: &'a [WorkspaceSymbol], name: &str) -> &'a WorkspaceSymbol {
    symbols
        .iter()
        .find(|symbol| symbol.name == name)
        .unwrap_or_else(|| panic!("missing workspace symbol `{name}` in {symbols:#?}"))
}
