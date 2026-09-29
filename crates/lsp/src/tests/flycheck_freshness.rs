use super::*;
use crate::vfs::VfsPath;
use crop::Rope;

const SOURCE: &str = "// SPDX-License-Identifier: MIT\npragma solidity >=0.0.0;\ncontract Test {}";
const EDITED_SOURCE: &str = "// SPDX-License-Identifier: MIT\npragma solidity >=0.0.0;\ncontract Test { function f() public pure { missingSymbol; } }";

fn freshness_project() -> TestProject {
    let mut project = TestProject::new();
    project.write_file("/foundry.toml", "[profile.default]\nsrc = \"src\"\n");
    project.write_file("/src/Test.sol", SOURCE);
    project.open_file("/src/Test.sol", SOURCE);
    project
}

async fn freshness_state(project: &TestProject) -> GlobalState {
    let mut params = project.initialize_params();
    params.initialization_options = Some(serde_json::json!({
        "sourceChangeDebounce": 0,
        "flychecks": [{ "id": "slow", "command": "unused-test-flycheck" }],
    }));
    let (_, mut config) = negotiate_capabilities(params);
    config.rediscover_workspaces();
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(config);
    state.vfs = Arc::new(RwLock::new(project.vfs()));
    state.vfs.write().set_file_version(VfsPath::from(project.path("/src/Test.sol")), 1);
    state.recompute_after_opening_source(vec![project.path("/src/Test.sol")]);
    tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
        .await
        .expect("initial source analysis should finish")
        .unwrap();
    state
}

fn change_document(state: &mut GlobalState, uri: &Url, version: i32, text: &str) {
    assert!(
        crate::handlers::did_change_text_document(
            state,
            DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier::new(uri.clone(), version),
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: text.into(),
                }],
            },
        )
        .is_continue()
    );
}

#[tokio::test(flavor = "current_thread")]
async fn document_changes_keep_or_clear_completed_flycheck_diagnostics() {
    let project = freshness_project();
    let mut state = freshness_state(&project).await;
    let uri = Url::from_file_path(project.path("/src/Test.sol")).unwrap();
    let owner = flycheck_owner(project.path("/"));
    let epoch = state.begin_flycheck_epoch(&owner);
    let stale = diagnostic("warning from the saved source");
    let publish = |state: &GlobalState| {
        state.snapshot().publish_flycheck_diagnostics(
            owner.clone(),
            epoch,
            DiagnosticMap::from_iter([(uri.clone(), vec![stale.clone()])]),
        );
    };
    let pulled = |state: &GlobalState| match state.diagnostics.read().pull_report(&uri, None) {
        PullReport::Full { diagnostics, .. } => diagnostics,
        _ => panic!("expected a full diagnostic report"),
    };
    publish(&state);

    change_document(&mut state, &uri, 2, SOURCE);
    assert_eq!(pulled(&state), std::slice::from_ref(&stale));
    assert!(state.snapshot().is_current_flycheck(&owner, epoch));

    change_document(&mut state, &uri, 3, EDITED_SOURCE);
    publish(&state);
    assert!(pulled(&state).is_empty());
    let reports =
        tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.workspace_diagnostic_reports(Vec::new()))
            .await
            .expect("workspace diagnostics should use the edited source")
            .unwrap();
    let report = reports.iter().find(|report| report.uri == uri).unwrap();
    assert_eq!(report.version, Some(3));
    let PullReport::Full { diagnostics, .. } = &report.report else {
        panic!("expected a full workspace diagnostic report");
    };
    assert!(!diagnostics.contains(&stale));
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Some(lsp_types::DiagnosticSeverity::ERROR))
    );
}

#[tokio::test(flavor = "current_thread")]
async fn flycheck_results_must_match_open_document_sources() {
    let project = freshness_project();
    let state = freshness_state(&project).await;
    let uri = Url::from_file_path(project.path("/src/Test.sol")).unwrap();
    let path = project.path("/src/Test.sol");

    for sources in
        [[(path, Rope::from("the saved source"))].into_iter().collect(), Default::default()]
    {
        let result = crate::flycheck::FlycheckResult {
            diagnostics: DiagnosticMap::from_iter([(
                uri.clone(),
                vec![diagnostic("saved warning")],
            )]),
            sources,
            sources_unchanged: true,
        };
        assert!(!state.snapshot().flycheck_sources_match_vfs(&result));
    }
}
