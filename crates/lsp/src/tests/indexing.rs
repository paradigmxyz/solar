use super::*;
use std::sync::atomic::AtomicBool;

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
async fn latest_analysis_uses_the_config_published_with_the_analysis() {
    let project = TestProject::new();
    let fallback_config = Arc::new(project.config_with_roots(&["/fallback"]));
    let published_config = Arc::new(project.config_with_roots(&["/published"]));
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = fallback_config;
    state.analysis_version.store(1, Ordering::Release);

    let latest = state.latest_analysis_with_config();
    {
        let mut commit = state.analysis_commit.lock();
        commit.symbol_tables_version = 1;
        commit.analysis_config = Some(published_config.clone());
    }
    state.published_analysis_version.send_replace(1);

    let (_, config) = latest.await.unwrap();
    assert!(Arc::ptr_eq(&config, &published_config));
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
    let worker = tokio::task::spawn_blocking(
        || -> Result<Option<WorkspaceDiscoveryResult>, WorkspaceError> {
            panic!("test workspace discovery failure")
        },
    );

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
async fn host_loader_failure_terminates_background_discovery_with_last_good_config() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"

        //- /src/Existing.sol
        contract Existing {}
        "#,
    );
    let fail = Arc::new(AtomicBool::new(false));
    let loader_fail = fail.clone();
    let launch_config =
        crate::LaunchConfig::default().with_foundry_workspace_config_loader(move |root| {
            if loader_fail.load(Ordering::Relaxed) {
                return Err("host config unavailable");
            }
            Ok(crate::FoundryWorkspaceConfig::new(root).with_source_roots(["src"]))
        });
    let (_, mut config) = crate::config::negotiate_capabilities_with_pull_diagnostic_data(
        project.initialize_params(),
        false,
        &launch_config,
    );
    config.rediscover_workspaces();
    assert_eq!(config.workspaces()[0].source_roots(), &[project.path("/src")]);

    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(config);
    fail.store(true, Ordering::Relaxed);
    let (version, progress) = state
        .begin_analysis(AnalysisMode::Rediscover, Vec::new(), Vec::new(), AnalysisTrigger::External)
        .unwrap();
    let cancellation = IndexingCancellation::default();
    let monitor = WorkspaceDiscoveryMonitor {
        version,
        disk_paths: Vec::new(),
        progress,
        cancellation: cancellation.clone(),
        analysis_version: state.analysis_version.clone(),
        published_analysis_version: state.published_analysis_version.clone(),
        analysis_commit: state.analysis_commit.clone(),
        client: state.client.clone(),
        config: state.config.clone(),
    };
    let discovery_config = state.config.clone();
    let worker = tokio::task::spawn_blocking(move || {
        discovery_config.try_discover_workspaces(&cancellation)
    });

    monitor.finish(worker).await;

    tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
        .await
        .expect("host loader failure should publish its terminal version")
        .unwrap();
    assert_eq!(state.config.workspaces()[0].source_roots(), &[project.path("/src")]);
    let commit = state.analysis_commit.lock();
    assert!(commit.cache_invalidated);
    assert!(!commit.discovery_pending);
}

#[test]
fn analysis_batches_index_closed_sources_when_source_root_overlaps_library() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "lib"

        //- /lib/Main.sol
        contract Main {}
        "#,
    );
    let snapshot = snapshot_with_config(project.config(), Vfs::default());

    let mut batches = snapshot.analysis_batches(Vec::new());
    let batch = batches.pop().unwrap();

    assert_eq!(
        batch.files,
        vec![(project.path("/lib/Main.sol"), Arc::new("contract Main {}".into()))]
    );
}

#[test]
fn analysis_batches_keep_sources_below_import_only_manifest_corridors() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "lib/contracts"

        //- /lib/foundry.toml
        [profile.default]
        src = "other"

        //- /lib/contracts/Main.sol
        contract Main {}

        //- /lib/dependency/Dependency.sol
        contract Dependency {}
        "#,
    );
    let config = project.config();
    assert!(!config.workspaces().iter().any(|workspace| {
        workspace.compile_opts().base_path.as_deref() == Some(project.path("/lib").as_path())
    }));
    let snapshot = snapshot_with_config(config, Vfs::default());

    let mut batches = snapshot.analysis_batches(Vec::new());
    let batch = batches.pop().unwrap();

    assert_eq!(
        batch.files,
        vec![(project.path("/lib/contracts/Main.sol"), Arc::new("contract Main {}".into()),)]
    );
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
        vec![(project.path("/nested/src/Included.sol"), Arc::new("contract Included {}".into()),)]
    );
}

#[test]
fn nested_external_source_root_outranks_an_outer_workspace_base() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"

        //- /src/Outer.sol
        contract Outer {}

        //- /packages/app/foundry.toml
        [profile.default]
        src = "../../shared"

        //- /shared/Shared.sol
        contract Shared {}
        "#,
    );
    let config = project.config();
    let shared = project.path("/shared/Shared.sol");
    let nested_root = project.path("/packages/app");
    let nested = config
        .workspaces()
        .iter()
        .find(|workspace| workspace.compile_opts().base_path.as_deref() == Some(&nested_root))
        .unwrap();

    assert_eq!(nested.source_roots(), [project.path("/shared")]);
    assert!(nested.source_files().contains(&shared));
    assert!(config.tracks_source_file(&shared));
}

#[test]
fn nested_external_flycheck_root_outranks_an_outer_workspace_base() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"

        //- /packages/app/foundry.toml
        [profile.default]
        src = "src"
        test = "../../checks"

        //- /checks/Nested.t.sol
        contract NestedTest {}
        "#,
    );
    let config = project.config();
    let nested_root = project.path("/packages/app");
    let path = project.path("/checks/Nested.t.sol");
    let nested = config
        .workspaces()
        .iter()
        .find(|workspace| workspace.compile_opts().base_path.as_deref() == Some(&nested_root))
        .unwrap();

    assert!(nested.flycheck_source_files().contains(&path));
    assert!(config.tracks_flycheck_file(&path));
}

#[test]
fn discovery_finds_nested_projects_under_dedicated_flycheck_roots() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"
        test = "out/checks"

        //- /out/checks/deep/app/foundry.toml
        [profile.default]
        src = "src"

        //- /out/checks/deep/app/src/Check.sol
        contract Check {}
        "#,
    );
    let config = project.config();
    let root = project.path("/out/checks/deep/app");
    let source = project.path("/out/checks/deep/app/src/Check.sol");
    let workspace = config
        .workspaces()
        .iter()
        .find(|workspace| workspace.compile_opts().base_path.as_deref() == Some(&root))
        .unwrap();

    assert_eq!(workspace.source_files(), [source]);
}

#[test]
fn discovery_reaches_nested_manifest_fixed_point() {
    let project = TestProject::from_fixture(
        r#"
        //- /workspace/foundry.toml
        [profile.default]
        src = "../shared/contracts"

        //- /shared/contracts/first/foundry.toml
        [profile.default]
        src = "src"

        //- /shared/contracts/first/src/First.sol
        contract First {}

        //- /shared/contracts/first/src/second/foundry.toml
        [profile.default]
        src = "src"

        //- /shared/contracts/first/src/second/src/Second.sol
        contract Second {}
        "#,
    );
    let config = project.config();
    let first_root = project.path("/shared/contracts/first");
    let second_root = project.path("/shared/contracts/first/src/second");
    let second_source = project.path("/shared/contracts/first/src/second/src/Second.sol");

    assert!(
        config.workspaces().iter().any(|workspace| {
            workspace.compile_opts().base_path.as_deref() == Some(&first_root)
        })
    );
    let second = config
        .workspaces()
        .iter()
        .find(|workspace| workspace.compile_opts().base_path.as_deref() == Some(&second_root))
        .unwrap();
    assert_eq!(second.source_files(), [second_source]);
}

#[test]
fn discovery_and_updates_share_the_most_specific_flycheck_owner() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"

        //- /packages/app/foundry.toml
        [profile.default]
        src = "src"
        test = "../../src/shared"

        //- /src/shared/Shared.t.sol
        contract SharedTest {}
        "#,
    );
    let mut config = project.config();
    let path = project.path("/src/shared/Shared.t.sol");
    let outer_root = project.root();
    let nested_root = project.path("/packages/app");
    fn workspace<'a>(root: &Path, config: &'a Config) -> &'a crate::workspace::Workspace {
        config
            .workspaces()
            .iter()
            .find(|workspace| workspace.compile_opts().base_path.as_deref() == Some(root))
            .unwrap()
    }

    let outer = workspace(outer_root, &config);
    let nested = workspace(&nested_root, &config);
    assert!(outer.source_files().contains(&path));
    assert!(!outer.flycheck_source_files().contains(&path));
    assert!(nested.flycheck_source_files().contains(&path));

    config.remove_source_file(&path);
    assert!(!workspace(outer_root, &config).source_files().contains(&path));
    assert!(!workspace(outer_root, &config).flycheck_source_files().contains(&path));
    assert!(!workspace(&nested_root, &config).flycheck_source_files().contains(&path));

    config.add_source_file(path.clone());
    assert!(workspace(outer_root, &config).source_files().contains(&path));
    assert!(!workspace(outer_root, &config).flycheck_source_files().contains(&path));
    assert!(workspace(&nested_root, &config).flycheck_source_files().contains(&path));
}
