use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RefreshEvent {
    Diagnostics,
    InlayHints,
}

struct RefreshHarness {
    client: ClientSocket,
    server: ServerSocket,
    events: mpsc::UnboundedReceiver<RefreshEvent>,
    server_task: tokio::task::JoinHandle<async_lsp::Result<()>>,
    client_task: tokio::task::JoinHandle<async_lsp::Result<()>>,
}

impl RefreshHarness {
    async fn next_event(&mut self) -> RefreshEvent {
        let event = tokio::time::timeout(ASYNC_TEST_TIMEOUT, self.events.recv())
            .await
            .expect("refresh request should arrive");
        let Some(event) = event else {
            let result = (&mut self.client_task).await;
            panic!("refresh event channel closed after client loop exited: {result:?}");
        };
        event
    }

    async fn expect_no_event(&mut self) {
        match tokio::time::timeout(Duration::from_millis(100), self.events.recv()).await {
            Err(_) => {}
            Ok(Some(event)) => panic!("unexpected refresh request: {event:?}"),
            Ok(None) => {
                let result = (&mut self.client_task).await;
                panic!("refresh event channel closed after client loop exited: {result:?}");
            }
        }
    }

    async fn expect_pull_result_refreshes(&mut self) {
        let first = self.next_event().await;
        let second = self.next_event().await;
        assert_ne!(first, second);
        assert!([first, second].contains(&RefreshEvent::Diagnostics));
        assert!([first, second].contains(&RefreshEvent::InlayHints));
    }

    async fn shutdown(self) {
        self.server.notify::<notification::Exit>(()).unwrap();
        assert!(self.server_task.await.unwrap().is_ok());
        assert!(matches!(self.client_task.await.unwrap(), Err(async_lsp::Error::Eof)));
    }
}

fn refresh_harness() -> RefreshHarness {
    let (server_main, client) = async_lsp::MainLoop::new_server(|_| {
        let mut router = Router::new(());
        router.notification::<notification::Exit>(|_, ()| ControlFlow::Break(Ok(())));
        router
    });
    let (events_tx, events) = mpsc::unbounded_channel();
    let (client_main, server) = async_lsp::MainLoop::new_client(move |_| {
        let mut router = Router::new(events_tx);
        router.request::<request::WorkspaceDiagnosticRefresh, _>(|events, ()| {
            events.send(RefreshEvent::Diagnostics).unwrap();
            async { Ok(()) }
        });
        router.request::<request::InlayHintRefreshRequest, _>(|events, ()| {
            events.send(RefreshEvent::InlayHints).unwrap();
            async { Ok(()) }
        });
        router.notification::<notification::PublishDiagnostics>(|_, _| ControlFlow::Continue(()));
        router
    });

    let (server_stream, client_stream) = tokio::io::duplex(64 << 10);
    let (server_rx, server_tx) = tokio::io::split(server_stream);
    let server_task =
        tokio::spawn(server_main.run_buffered(server_rx.compat(), server_tx.compat_write()));
    let (client_rx, client_tx) = tokio::io::split(client_stream);
    let client_task =
        tokio::spawn(client_main.run_buffered(client_rx.compat(), client_tx.compat_write()));

    RefreshHarness { client, server, events, server_task, client_task }
}

fn pull_refresh_config(diagnostics: bool, inlay_hints: bool) -> Config {
    let mut params = InitializeParams::default();
    params.capabilities.workspace = Some(WorkspaceClientCapabilities {
        diagnostic: Some(DiagnosticWorkspaceClientCapabilities {
            refresh_support: Some(diagnostics),
        }),
        inlay_hint: Some(InlayHintWorkspaceClientCapabilities {
            refresh_support: Some(inlay_hints),
        }),
        ..Default::default()
    });
    negotiate_capabilities(params).1
}

fn changed_pull_result() -> AnalysisResult {
    let path = std::env::temp_dir().join("Hints.sol");
    let uri = Url::from_file_path(&path).unwrap();
    let mut result = analyze(AnalysisBatch::from_files(
        CompileOpts::default(),
        [(
            path,
            "contract C { function target(uint amount) public pure returns (uint) { return amount; } function caller() public pure returns (uint) { return target(1); } }".into(),
        )],
    ));
    assert!(
        !result
            .symbol_tables
            .inlay_hints(&uri, Range::new(Position::new(0, 0), Position::new(u32::MAX, u32::MAX)),)
            .is_empty()
    );
    result.diagnostics.insert(uri, vec![diagnostic("changed")]);
    result
}

#[tokio::test(flavor = "current_thread")]
async fn external_analysis_refreshes_changed_pull_results() {
    let mut harness = refresh_harness();
    let mut state = GlobalState::new(harness.client.clone());
    state.config = Arc::new(pull_refresh_config(true, true));

    let (version, _progress) = state
        .begin_analysis(AnalysisMode::Recompute, Vec::new(), Vec::new(), AnalysisTrigger::External)
        .unwrap();
    assert!(state.snapshot().publish_analysis(version, changed_pull_result()));

    harness.expect_pull_result_refreshes().await;

    harness.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn external_analysis_refreshes_changed_workspace_membership_once() {
    let mut harness = refresh_harness();
    let mut state = GlobalState::new(harness.client.clone());
    state.config = Arc::new(pull_refresh_config(true, false));
    let uri = diagnostic_uri();

    let (version, _progress) = state
        .begin_analysis(AnalysisMode::Recompute, Vec::new(), Vec::new(), AnalysisTrigger::External)
        .unwrap();
    assert!(state.snapshot().publish_analysis(
        version,
        AnalysisResult {
            analyzed_documents: AnalyzedDocuments::from_iter([(uri.clone(), Some(1))]),
            diagnostics: DiagnosticMap::default(),
            symbol_tables: SymbolTables::default(),
        },
    ));
    assert_eq!(harness.next_event().await, RefreshEvent::Diagnostics);
    harness.expect_no_event().await;

    let (version, _progress) = state
        .begin_analysis(AnalysisMode::Recompute, Vec::new(), Vec::new(), AnalysisTrigger::External)
        .unwrap();
    assert!(state.snapshot().publish_analysis(
        version,
        AnalysisResult {
            analyzed_documents: AnalyzedDocuments::from_iter([(uri, Some(2))]),
            diagnostics: DiagnosticMap::default(),
            symbol_tables: SymbolTables::default(),
        },
    ));
    harness.expect_no_event().await;

    harness.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn ordinary_and_unchanged_analyses_do_not_refresh_pull_results() {
    let mut harness = refresh_harness();
    let mut state = GlobalState::new(harness.client.clone());
    state.config = Arc::new(pull_refresh_config(true, true));
    SymbolTables::take_inlay_hint_comparisons();

    let (version, _progress) = state
        .begin_analysis(AnalysisMode::Recompute, Vec::new(), Vec::new(), AnalysisTrigger::Document)
        .unwrap();
    assert!(state.snapshot().publish_analysis(version, changed_pull_result()));
    assert_eq!(SymbolTables::take_inlay_hint_comparisons(), 0);
    assert_eq!(harness.next_event().await, RefreshEvent::Diagnostics);
    harness.expect_no_event().await;

    let (version, _progress) = state
        .begin_analysis(AnalysisMode::Recompute, Vec::new(), Vec::new(), AnalysisTrigger::External)
        .unwrap();
    assert!(state.snapshot().publish_analysis(version, changed_pull_result()));
    assert_eq!(SymbolTables::take_inlay_hint_comparisons(), 1);
    harness.expect_no_event().await;

    harness.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn unsupported_pull_refresh_capabilities_send_no_requests() {
    let mut harness = refresh_harness();
    let mut state = GlobalState::new(harness.client.clone());
    state.config = Arc::new(pull_refresh_config(false, false));
    SymbolTables::take_inlay_hint_comparisons();

    let (version, _progress) = state
        .begin_analysis(AnalysisMode::Recompute, Vec::new(), Vec::new(), AnalysisTrigger::External)
        .unwrap();
    assert!(state.snapshot().publish_analysis(version, changed_pull_result()));
    assert_eq!(SymbolTables::take_inlay_hint_comparisons(), 0);
    harness.expect_no_event().await;

    harness.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn pull_refresh_requests_honor_independent_capabilities() {
    for (diagnostics, inlay_hints, expected) in
        [(true, false, RefreshEvent::Diagnostics), (false, true, RefreshEvent::InlayHints)]
    {
        let mut harness = refresh_harness();
        let mut state = GlobalState::new(harness.client.clone());
        state.config = Arc::new(pull_refresh_config(diagnostics, inlay_hints));
        let (version, _progress) = state
            .begin_analysis(
                AnalysisMode::Recompute,
                Vec::new(),
                Vec::new(),
                AnalysisTrigger::External,
            )
            .unwrap();

        assert!(state.snapshot().publish_analysis(version, changed_pull_result()));

        assert_eq!(harness.next_event().await, expected);
        harness.expect_no_event().await;
        harness.shutdown().await;
    }
}

#[tokio::test(flavor = "current_thread")]
async fn external_analysis_preserves_early_diagnostic_changes_until_commit() {
    let mut harness = refresh_harness();
    let mut state = GlobalState::new(harness.client.clone());
    state.config = Arc::new(pull_refresh_config(true, true));
    let uri = diagnostic_uri();
    state.snapshot().publish_diagnostics(
        DiagnosticOwner::Compiler,
        DiagnosticMap::from_iter([(uri.clone(), vec![diagnostic("removed")])]),
    );

    let (version, _progress) = state
        .begin_analysis(
            AnalysisMode::Recompute,
            vec![uri.to_file_path().unwrap()],
            Vec::new(),
            AnalysisTrigger::External,
        )
        .unwrap();
    assert!(state.snapshot().publish_analysis(
        version,
        AnalysisResult {
            analyzed_documents: AnalyzedDocuments::default(),
            diagnostics: DiagnosticMap::default(),
            symbol_tables: SymbolTables::default(),
        },
    ));

    assert_eq!(harness.next_event().await, RefreshEvent::Diagnostics);
    harness.expect_no_event().await;
    harness.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn removed_flycheck_diagnostics_coalesce_with_external_analysis_refresh() {
    let mut harness = refresh_harness();
    let mut state = GlobalState::new(harness.client.clone());
    state.config = Arc::new(pull_refresh_config(true, true));
    let owner = flycheck_owner("/workspace");
    let uri = diagnostic_uri();
    state.snapshot().publish_diagnostics(
        owner.clone(),
        DiagnosticMap::from_iter([(uri, vec![diagnostic("removed")])]),
    );
    let (version, _progress) = state
        .begin_analysis(AnalysisMode::Recompute, Vec::new(), Vec::new(), AnalysisTrigger::External)
        .unwrap();

    state.clear_removed_flycheck_diagnostics([owner]);

    harness.expect_no_event().await;
    assert!(state.snapshot().publish_analysis(
        version,
        AnalysisResult {
            analyzed_documents: AnalyzedDocuments::default(),
            diagnostics: DiagnosticMap::default(),
            symbol_tables: SymbolTables::default(),
        },
    ));
    assert_eq!(harness.next_event().await, RefreshEvent::Diagnostics);
    harness.expect_no_event().await;
    harness.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn removed_flycheck_diagnostics_refresh_without_external_analysis() {
    let mut harness = refresh_harness();
    let mut state = GlobalState::new(harness.client.clone());
    state.config = Arc::new(pull_refresh_config(true, true));
    let owner = flycheck_owner("/workspace");
    state.snapshot().publish_diagnostics(
        owner.clone(),
        DiagnosticMap::from_iter([(diagnostic_uri(), vec![diagnostic("removed")])]),
    );

    state.clear_removed_flycheck_diagnostics([owner]);

    assert_eq!(harness.next_event().await, RefreshEvent::Diagnostics);
    harness.expect_no_event().await;
    harness.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn external_refresh_intent_survives_superseded_analysis() {
    let mut harness = refresh_harness();
    let mut state = GlobalState::new(harness.client.clone());
    state.config = Arc::new(pull_refresh_config(true, true));
    let uri = diagnostic_uri();
    state.snapshot().publish_diagnostics(
        DiagnosticOwner::Compiler,
        DiagnosticMap::from_iter([(uri.clone(), vec![diagnostic("removed")])]),
    );

    let (stale_version, _progress) = state
        .begin_analysis(
            AnalysisMode::Recompute,
            vec![uri.to_file_path().unwrap()],
            Vec::new(),
            AnalysisTrigger::External,
        )
        .unwrap();
    let mut stale_snapshot = state.snapshot();
    let (current_version, _progress) = state
        .begin_analysis(AnalysisMode::Recompute, Vec::new(), Vec::new(), AnalysisTrigger::Document)
        .unwrap();
    let mut current_snapshot = state.snapshot();
    let unchanged_result = || AnalysisResult {
        analyzed_documents: AnalyzedDocuments::default(),
        diagnostics: DiagnosticMap::default(),
        symbol_tables: SymbolTables::default(),
    };

    assert!(!stale_snapshot.publish_analysis(stale_version, unchanged_result()));
    harness.expect_no_event().await;
    assert!(current_snapshot.publish_analysis(current_version, unchanged_result()));
    assert_eq!(harness.next_event().await, RefreshEvent::Diagnostics);
    harness.expect_no_event().await;
    {
        let commit = state.analysis_commit.lock();
        assert!(commit.external_refresh.is_none());
    }

    harness.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn external_refresh_intent_survives_failed_analysis() {
    let mut harness = refresh_harness();
    let mut state = GlobalState::new(harness.client.clone());
    state.config = Arc::new(pull_refresh_config(true, true));
    let uri = diagnostic_uri();
    state.snapshot().publish_diagnostics(
        DiagnosticOwner::Compiler,
        DiagnosticMap::from_iter([(uri.clone(), vec![diagnostic("removed")])]),
    );

    let (failed_version, progress) = state
        .begin_analysis(
            AnalysisMode::Recompute,
            vec![uri.to_file_path().unwrap()],
            Vec::new(),
            AnalysisTrigger::External,
        )
        .unwrap();
    let task = tokio::spawn(async { panic!("test analysis failure") });
    state.monitor_analysis_task(failed_version, task, progress);
    tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
        .await
        .expect("failed analysis should release waiters")
        .unwrap();
    {
        let commit = state.analysis_commit.lock();
        let refresh = commit.external_refresh.as_ref().expect("external refresh intent");
        assert!(!refresh.diagnostics_changed);
    }
    assert_eq!(harness.next_event().await, RefreshEvent::Diagnostics);
    harness.expect_no_event().await;

    let (recovery_version, _progress) = state
        .begin_analysis(AnalysisMode::Recompute, Vec::new(), Vec::new(), AnalysisTrigger::Document)
        .unwrap();
    let mut recovery_result = changed_pull_result();
    recovery_result.diagnostics.clear();
    assert!(state.snapshot().publish_analysis(recovery_version, recovery_result));
    harness.expect_pull_result_refreshes().await;
    harness.expect_no_event().await;
    {
        let commit = state.analysis_commit.lock();
        assert!(commit.external_refresh.is_none());
    }

    harness.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn clearing_analysis_cache_refreshes_only_changed_pull_results() {
    let mut harness = refresh_harness();
    let mut state = GlobalState::new(harness.client.clone());
    state.config = Arc::new(pull_refresh_config(true, true));
    assert!(state.snapshot().publish_analysis(0, changed_pull_result()));
    assert_eq!(harness.next_event().await, RefreshEvent::Diagnostics);
    harness.expect_no_event().await;

    state.clear_analysis_cache();
    harness.expect_pull_result_refreshes().await;
    harness.expect_no_event().await;

    state.clear_analysis_cache();
    harness.expect_no_event().await;
    harness.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn invalidated_analysis_refreshes_restored_pull_results() {
    let mut harness = refresh_harness();
    let mut state = GlobalState::new(harness.client.clone());
    state.config = Arc::new(pull_refresh_config(true, true));
    assert!(state.snapshot().publish_analysis(0, changed_pull_result()));
    assert_eq!(harness.next_event().await, RefreshEvent::Diagnostics);
    harness.expect_no_event().await;

    state.clear_analysis_cache();
    harness.expect_pull_result_refreshes().await;
    harness.expect_no_event().await;

    let (version, _progress) = state
        .begin_analysis(
            AnalysisMode::IfInvalidated,
            Vec::new(),
            Vec::new(),
            AnalysisTrigger::Document,
        )
        .unwrap();
    assert!(state.snapshot().publish_analysis(version, changed_pull_result()));

    harness.expect_pull_result_refreshes().await;
    harness.expect_no_event().await;
    harness.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn current_flycheck_refreshes_only_changed_diagnostics() {
    let mut harness = refresh_harness();
    let mut state = GlobalState::new(harness.client.clone());
    state.config = Arc::new(pull_refresh_config(true, true));
    let owner = flycheck_owner("/workspace");
    let uri = diagnostic_uri();

    let version = state.begin_flycheck_epoch(&owner);
    state.snapshot().publish_flycheck_diagnostics(
        owner.clone(),
        version,
        DiagnosticMap::from_iter([(uri.clone(), vec![diagnostic("flycheck")])]),
    );
    assert_eq!(harness.next_event().await, RefreshEvent::Diagnostics);
    harness.expect_no_event().await;

    let version = state.begin_flycheck_epoch(&owner);
    state.snapshot().publish_flycheck_diagnostics(
        owner.clone(),
        version,
        DiagnosticMap::from_iter([(uri.clone(), vec![diagnostic("flycheck")])]),
    );
    harness.expect_no_event().await;

    let stale_version = state.begin_flycheck_epoch(&owner);
    let current_version = state.begin_flycheck_epoch(&owner);
    state.snapshot().publish_flycheck_diagnostics(
        owner.clone(),
        stale_version,
        DiagnosticMap::from_iter([(uri.clone(), vec![diagnostic("stale")])]),
    );
    harness.expect_no_event().await;
    state.snapshot().publish_flycheck_diagnostics(
        owner.clone(),
        current_version,
        DiagnosticMap::from_iter([(uri, vec![diagnostic("flycheck")])]),
    );
    harness.expect_no_event().await;

    let version = state.begin_flycheck_epoch(&owner);
    state.snapshot().publish_flycheck_diagnostics(owner.clone(), version, DiagnosticMap::default());
    assert_eq!(harness.next_event().await, RefreshEvent::Diagnostics);
    harness.expect_no_event().await;

    let version = state.begin_flycheck_epoch(&owner);
    state.snapshot().publish_flycheck_diagnostics(owner, version, DiagnosticMap::default());
    harness.expect_no_event().await;

    harness.shutdown().await;
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn failed_save_flycheck_refreshes_cleared_diagnostics() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"

        //- /src/Test.sol
        contract Test {}
        "#,
    );
    let path = project.path("/src/Test.sol");
    let uri = Url::from_file_path(&path).unwrap();
    let mut params = project.initialize_params();
    params.capabilities.workspace = Some(WorkspaceClientCapabilities {
        diagnostic: Some(DiagnosticWorkspaceClientCapabilities { refresh_support: Some(true) }),
        ..Default::default()
    });
    params.initialization_options = Some(serde_json::json!({
        "flychecks": [{
            "id": "save-error",
            "command": "/bin/sh",
            "args": ["-c", "exit 1"]
        }]
    }));
    let (_, mut config) = negotiate_capabilities(params);
    config.rediscover_workspaces();
    let [flycheck] = config.flychecks_for_path(&path).try_into().unwrap();

    let mut harness = refresh_harness();
    let mut state = GlobalState::new(harness.client.clone());
    state.config = Arc::new(config);
    state.snapshot().publish_diagnostics(
        flycheck.owner(),
        DiagnosticMap::from_iter([(uri.clone(), vec![diagnostic("stale flycheck")])]),
    );

    let result = crate::handlers::did_save_text_document(
        &mut state,
        DidSaveTextDocumentParams { text_document: TextDocumentIdentifier::new(uri), text: None },
    );

    assert!(matches!(result, ControlFlow::Continue(())));
    assert_eq!(harness.next_event().await, RefreshEvent::Diagnostics);
    harness.expect_no_event().await;
    harness.shutdown().await;
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn invalidated_save_refreshes_removed_flycheck_diagnostics() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"

        //- /src/Test.sol
        contract Test {}
        "#,
    );
    let path = project.path("/src/Test.sol");
    let uri = Url::from_file_path(&path).unwrap();
    let mut params = project.initialize_params();
    params.capabilities.workspace = Some(WorkspaceClientCapabilities {
        diagnostic: Some(DiagnosticWorkspaceClientCapabilities { refresh_support: Some(true) }),
        ..Default::default()
    });
    params.initialization_options = Some(serde_json::json!({ "forgePath": "/usr/bin/true" }));
    let (_, mut config) = negotiate_capabilities(params);
    config.rediscover_workspaces();
    let [flycheck] = config.flychecks_for_path(&path).try_into().unwrap();

    let mut harness = refresh_harness();
    let mut state = GlobalState::new(harness.client.clone());
    state.config = Arc::new(config);
    state.snapshot().publish_diagnostics(
        flycheck.owner(),
        DiagnosticMap::from_iter([(uri.clone(), vec![diagnostic("removed flycheck")])]),
    );
    state.clear_analysis_cache();
    harness.expect_no_event().await;
    project.remove_file("/foundry.toml");

    let result = crate::handlers::did_save_text_document(
        &mut state,
        DidSaveTextDocumentParams { text_document: TextDocumentIdentifier::new(uri), text: None },
    );

    assert!(matches!(result, ControlFlow::Continue(())));
    tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
        .await
        .expect("invalidated save analysis should finish")
        .unwrap();
    assert_eq!(harness.next_event().await, RefreshEvent::Diagnostics);
    harness.expect_no_event().await;
    harness.shutdown().await;
}
