use super::*;
use lsp_types::{
    DiagnosticRelatedInformation, DiagnosticTag, Location, PublishDiagnosticsClientCapabilities,
    TagSupport,
};

fn pull_refresh_config(diagnostics: bool, inlay_hints: bool) -> Config {
    diagnostic_refresh_config(diagnostics, diagnostics, inlay_hints)
}

fn diagnostic_refresh_config(
    document_diagnostics: bool,
    diagnostic_refresh: bool,
    inlay_hints: bool,
) -> Config {
    let mut params = InitializeParams::default();
    params.capabilities.text_document.get_or_insert_default().diagnostic =
        document_diagnostics.then(DiagnosticClientCapabilities::default);
    params.capabilities.workspace = Some(WorkspaceClientCapabilities {
        diagnostic: Some(DiagnosticWorkspaceClientCapabilities {
            refresh_support: Some(diagnostic_refresh),
        }),
        inlay_hint: Some(InlayHintWorkspaceClientCapabilities {
            refresh_support: Some(inlay_hints),
        }),
        ..Default::default()
    });
    negotiate_capabilities(params).1
}

fn pull_refresh_state(
    harness: &ClientHarness,
    diagnostics: bool,
    inlay_hints: bool,
) -> GlobalState {
    let mut state = GlobalState::new(harness.client.clone());
    state.config = Arc::new(pull_refresh_config(diagnostics, inlay_hints));
    state
}

fn begin(state: &mut GlobalState, removed_paths: Vec<PathBuf>, trigger: AnalysisTrigger) -> usize {
    state.begin_analysis(AnalysisMode::Recompute, removed_paths, Vec::new(), trigger).unwrap().0
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
    result.diagnostics = diagnostics_for(&uri, "changed");
    result
}

fn diagnostic_with_details(uri: &Url, message: &str, related: &str) -> Diagnostic {
    let mut diagnostic = diagnostic(message);
    diagnostic.related_information = Some(vec![DiagnosticRelatedInformation {
        location: Location::new(uri.clone(), diagnostic.range),
        message: related.into(),
    }]);
    diagnostic.data = Some(serde_json::json!({ "retained": true }));
    diagnostic
}

#[tokio::test(flavor = "current_thread")]
async fn published_diagnostics_preserve_related_text_without_client_support() {
    let mut harness = ClientHarness::new();
    let state = GlobalState::new(harness.client.clone());
    let uri = diagnostic_uri();
    let original = diagnostic_with_details(
        &uri,
        "cannot override non-virtual function",
        "note: overriding function is here",
    );
    state.snapshot().publish_diagnostics(
        DiagnosticOwner::Compiler,
        DiagnosticMap::from_iter([(uri.clone(), vec![original.clone()])]),
    );

    let published = harness.next_published().await;
    let [diagnostic] = published.diagnostics.as_slice() else {
        panic!("expected one published diagnostic");
    };
    snapbox::assert_data_eq!(
        diagnostic.message.as_str(),
        snapbox::str![[r#"
cannot override non-virtual function
note: overriding function is here
"#]],
    );
    assert_eq!(diagnostic.related_information, None);
    assert_eq!(diagnostic.data, None);
    assert_eq!(
        state.diagnostics.read().code_action_diagnostics(&uri, original.range),
        vec![original],
        "outgoing presentation must not change cached code-action diagnostics",
    );
    harness.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn published_diagnostics_honor_supported_tags_and_related_information() {
    for supported in [
        None,
        Some(vec![]),
        Some(vec![DiagnosticTag::DEPRECATED]),
        Some(vec![DiagnosticTag::UNNECESSARY, DiagnosticTag::DEPRECATED]),
    ] {
        let mut harness = ClientHarness::new();
        let mut state = GlobalState::new(harness.client.clone());
        let mut params = InitializeParams::default();
        params.capabilities.text_document.get_or_insert_default().publish_diagnostics =
            Some(PublishDiagnosticsClientCapabilities {
                related_information: Some(true),
                tag_support: supported.clone().map(|value_set| TagSupport { value_set }),
                data_support: Some(true),
                ..Default::default()
            });
        state.config = Arc::new(negotiate_capabilities(params).1);
        let uri = diagnostic_uri();
        let mut original =
            diagnostic_with_details(&uri, "deprecated unused declaration", "declaration is here");
        original.tags = Some(vec![DiagnosticTag::UNNECESSARY, DiagnosticTag::DEPRECATED]);
        state.snapshot().publish_diagnostics(
            DiagnosticOwner::Compiler,
            DiagnosticMap::from_iter([(uri.clone(), vec![original.clone()])]),
        );

        let published = harness.next_published().await;
        let mut expected = original.clone();
        expected.tags = supported.filter(|tags| !tags.is_empty());
        assert_eq!(published.diagnostics, [expected]);
        assert_eq!(
            state.diagnostics.read().code_action_diagnostics(&uri, original.range),
            vec![original],
            "tag negotiation must not remove metadata from cached diagnostics",
        );
        harness.shutdown().await;
    }
}

#[tokio::test(flavor = "current_thread")]
async fn pulled_diagnostics_preserve_details_without_publish_capabilities() {
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(pull_refresh_config(true, false));
    let uri = diagnostic_uri();
    let mut original =
        diagnostic_with_details(&uri, "deprecated declaration", "declaration is here");
    original.tags = Some(vec![DiagnosticTag::DEPRECATED]);
    state.snapshot().publish_diagnostics(
        DiagnosticOwner::Compiler,
        DiagnosticMap::from_iter([(uri.clone(), vec![original.clone()])]),
    );
    let mut expected = original.clone();
    expected.data = None;

    let PullReport::Full { diagnostics, .. } =
        state.pull_diagnostic_report(uri.clone(), None).await.unwrap()
    else {
        panic!("expected a full document report");
    };
    assert_eq!(diagnostics, [expected.clone()]);
    let reports = state.workspace_diagnostic_reports(Vec::new()).await.unwrap();
    let [report] = reports.as_slice() else {
        panic!("expected one workspace report");
    };
    let PullReport::Full { diagnostics, .. } = &report.report else {
        panic!("expected a full workspace report");
    };
    assert_eq!(diagnostics, &[expected]);
    assert_eq!(state.code_action_diagnostics(uri, original.range).await.unwrap(), [original]);
}

#[tokio::test(flavor = "current_thread")]
async fn diagnostic_updates_use_only_the_negotiated_delivery() {
    for document_diagnostics in [false, true] {
        let mut harness = ClientHarness::new();
        let mut state = GlobalState::new(harness.client.clone());
        state.config = Arc::new(diagnostic_refresh_config(document_diagnostics, true, false));
        assert_eq!(state.config.uses_pull_diagnostics(), document_diagnostics);
        let version = begin(&mut state, Vec::new(), AnalysisTrigger::External);

        assert!(state.snapshot().publish_analysis(version, changed_pull_result()));

        if document_diagnostics {
            assert_eq!(harness.next_event().await, ClientEvent::DiagnosticRefresh);
        } else {
            assert!(!harness.next_published().await.diagnostics.is_empty());
        }
        harness.expect_no_event().await;
        harness.shutdown().await;
    }
}

#[tokio::test(flavor = "current_thread")]
async fn external_analysis_refreshes_changed_pull_results_per_capability() {
    for (diagnostics, inlay_hints) in [(true, true), (true, false), (false, true), (false, false)] {
        let mut harness = ClientHarness::new();
        let mut state = pull_refresh_state(&harness, diagnostics, inlay_hints);
        SymbolTables::take_inlay_hint_comparisons();

        let version = begin(&mut state, Vec::new(), AnalysisTrigger::External);
        assert!(state.snapshot().publish_analysis(version, changed_pull_result()));

        assert_eq!(SymbolTables::take_inlay_hint_comparisons(), usize::from(inlay_hints));
        if !diagnostics {
            // Without pull support, the changed diagnostics are pushed instead.
            assert_eq!(
                diagnostic_messages(&harness.next_published().await.diagnostics),
                ["changed"]
            );
        }
        harness.expect_refreshes(diagnostics, inlay_hints).await;
        harness.shutdown().await;
    }
}

#[tokio::test(flavor = "current_thread")]
async fn external_analysis_refreshes_changed_workspace_membership_once() {
    let mut harness = ClientHarness::new();
    let mut state = pull_refresh_state(&harness, true, false);
    let uri = diagnostic_uri();

    for (analyzed_version, refresh) in [(1, true), (2, false)] {
        let version = begin(&mut state, Vec::new(), AnalysisTrigger::External);
        let analyzed_documents =
            AnalyzedDocuments::from_iter([(uri.clone(), Some(analyzed_version))]);
        let result = AnalysisResult { analyzed_documents, ..Default::default() };
        assert!(state.snapshot().publish_analysis(version, result));
        harness.expect_refreshes(refresh, false).await;
    }

    harness.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn ordinary_and_unchanged_analyses_do_not_refresh_pull_results() {
    let mut harness = ClientHarness::new();
    let mut state = pull_refresh_state(&harness, true, true);
    SymbolTables::take_inlay_hint_comparisons();

    let version = begin(&mut state, Vec::new(), AnalysisTrigger::Document);
    assert!(state.snapshot().publish_analysis(version, changed_pull_result()));
    assert_eq!(SymbolTables::take_inlay_hint_comparisons(), 0);
    harness.expect_refreshes(true, false).await;

    let version = begin(&mut state, Vec::new(), AnalysisTrigger::External);
    assert!(state.snapshot().publish_analysis(version, changed_pull_result()));
    assert_eq!(SymbolTables::take_inlay_hint_comparisons(), 1);
    harness.expect_no_event().await;

    harness.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn external_analysis_preserves_early_diagnostic_changes_until_commit() {
    let mut harness = ClientHarness::new();
    let mut state = pull_refresh_state(&harness, true, true);
    let uri = diagnostic_uri();
    state
        .snapshot()
        .publish_diagnostics(DiagnosticOwner::Compiler, diagnostics_for(&uri, "removed"));

    let version = begin(&mut state, vec![uri.to_file_path().unwrap()], AnalysisTrigger::External);
    assert!(state.snapshot().publish_analysis(version, AnalysisResult::default()));

    harness.expect_refreshes(true, false).await;
    harness.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn removed_flycheck_diagnostics_refresh_immediately_or_with_external_analysis() {
    let mut harness = ClientHarness::new();
    let mut state = pull_refresh_state(&harness, true, true);
    let owner = flycheck_owner("/workspace");
    let uri = diagnostic_uri();

    state.snapshot().publish_diagnostics(owner.clone(), diagnostics_for(&uri, "removed"));
    state.clear_removed_flycheck_diagnostics([owner.clone()]);
    harness.expect_refreshes(true, false).await;

    state.snapshot().publish_diagnostics(owner.clone(), diagnostics_for(&uri, "removed"));
    let version = begin(&mut state, Vec::new(), AnalysisTrigger::External);
    state.clear_removed_flycheck_diagnostics([owner]);
    harness.expect_no_event().await;
    assert!(state.snapshot().publish_analysis(version, AnalysisResult::default()));
    harness.expect_refreshes(true, false).await;

    harness.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn external_refresh_intent_survives_superseded_analysis() {
    let mut harness = ClientHarness::new();
    let mut state = pull_refresh_state(&harness, true, true);
    let uri = diagnostic_uri();
    state
        .snapshot()
        .publish_diagnostics(DiagnosticOwner::Compiler, diagnostics_for(&uri, "removed"));

    let stale_version =
        begin(&mut state, vec![uri.to_file_path().unwrap()], AnalysisTrigger::External);
    let mut stale_snapshot = state.snapshot();
    let current_version = begin(&mut state, Vec::new(), AnalysisTrigger::Document);
    let mut current_snapshot = state.snapshot();

    assert!(!stale_snapshot.publish_analysis(stale_version, AnalysisResult::default()));
    harness.expect_no_event().await;
    assert!(current_snapshot.publish_analysis(current_version, AnalysisResult::default()));
    harness.expect_refreshes(true, false).await;
    assert!(state.analysis_commit.lock().external_refresh.is_none());

    harness.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn external_refresh_intent_survives_failed_analysis() {
    let mut harness = ClientHarness::new();
    let mut state = pull_refresh_state(&harness, true, true);
    let uri = diagnostic_uri();
    state
        .snapshot()
        .publish_diagnostics(DiagnosticOwner::Compiler, diagnostics_for(&uri, "removed"));

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
    wait_for_analysis(&state).await;
    assert_eq!(state.analysis_commit.lock().external_refresh, Some(false));
    harness.expect_refreshes(true, false).await;

    let recovery_version = begin(&mut state, Vec::new(), AnalysisTrigger::Document);
    let mut recovery_result = changed_pull_result();
    recovery_result.diagnostics.clear();
    assert!(state.snapshot().publish_analysis(recovery_version, recovery_result));
    harness.expect_refreshes(true, true).await;
    assert!(state.analysis_commit.lock().external_refresh.is_none());

    harness.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn clearing_and_restoring_the_analysis_cache_refresh_only_changed_pull_results() {
    let mut harness = ClientHarness::new();
    let mut state = pull_refresh_state(&harness, true, true);
    assert!(state.snapshot().publish_analysis(0, changed_pull_result()));
    harness.expect_refreshes(true, false).await;

    state.clear_analysis_cache();
    harness.expect_refreshes(true, true).await;
    state.clear_analysis_cache();
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
    harness.expect_refreshes(true, true).await;

    harness.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn current_flycheck_refreshes_only_changed_diagnostics() {
    let mut harness = ClientHarness::new();
    let mut state = pull_refresh_state(&harness, true, true);
    let owner = flycheck_owner("/workspace");
    let uri = diagnostic_uri();

    let version = state.begin_flycheck_epoch(&owner);
    state.snapshot().publish_flycheck_diagnostics(
        owner.clone(),
        version,
        diagnostics_for(&uri, "flycheck"),
    );
    harness.expect_refreshes(true, false).await;

    let version = state.begin_flycheck_epoch(&owner);
    state.snapshot().publish_flycheck_diagnostics(
        owner.clone(),
        version,
        diagnostics_for(&uri, "flycheck"),
    );
    harness.expect_no_event().await;

    let stale_version = state.begin_flycheck_epoch(&owner);
    let current_version = state.begin_flycheck_epoch(&owner);
    state.snapshot().publish_flycheck_diagnostics(
        owner.clone(),
        stale_version,
        diagnostics_for(&uri, "stale"),
    );
    harness.expect_no_event().await;
    state.snapshot().publish_flycheck_diagnostics(
        owner.clone(),
        current_version,
        diagnostics_for(&uri, "flycheck"),
    );
    harness.expect_no_event().await;

    let version = state.begin_flycheck_epoch(&owner);
    state.snapshot().publish_flycheck_diagnostics(owner.clone(), version, DiagnosticMap::default());
    harness.expect_refreshes(true, false).await;

    let version = state.begin_flycheck_epoch(&owner);
    state.snapshot().publish_flycheck_diagnostics(owner, version, DiagnosticMap::default());
    harness.expect_no_event().await;

    harness.shutdown().await;
}

/// Returns a pull-diagnostics state for a Foundry project with one flycheck on `/src/Test.sol`.
#[cfg(unix)]
fn flycheck_refresh_state(
    harness: &ClientHarness,
    options: serde_json::Value,
) -> (TestProject, GlobalState, Url) {
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
    params.capabilities.text_document.get_or_insert_default().diagnostic =
        Some(DiagnosticClientCapabilities::default());
    params.capabilities.workspace = Some(WorkspaceClientCapabilities {
        diagnostic: Some(DiagnosticWorkspaceClientCapabilities { refresh_support: Some(true) }),
        ..Default::default()
    });
    let config = rediscovered_config(params, options);
    let [flycheck] = config.flychecks_for_path(&path).try_into().unwrap();
    let mut state = GlobalState::new(harness.client.clone());
    state.config = Arc::new(config);
    state.snapshot().publish_diagnostics(flycheck.owner(), diagnostics_for(&uri, "stale flycheck"));
    (project, state, uri)
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn failed_save_flycheck_refreshes_cleared_diagnostics() {
    let mut harness = ClientHarness::new();
    let (_project, mut state, uri) = flycheck_refresh_state(
        &harness,
        serde_json::json!({
            "flychecks": [{ "id": "save-error", "command": "/bin/sh", "args": ["-c", "exit 1"] }]
        }),
    );

    save_document(&mut state, &uri);

    harness.expect_refreshes(true, false).await;
    harness.shutdown().await;
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn invalidated_save_refreshes_removed_flycheck_diagnostics() {
    let mut harness = ClientHarness::new();
    let (project, mut state, uri) =
        flycheck_refresh_state(&harness, serde_json::json!({ "forgePath": "/usr/bin/true" }));
    state.clear_analysis_cache();
    harness.expect_no_event().await;
    project.remove_file("/foundry.toml");

    save_document(&mut state, &uri);

    wait_for_analysis(&state).await;
    harness.expect_refreshes(true, false).await;
    harness.shutdown().await;
}
