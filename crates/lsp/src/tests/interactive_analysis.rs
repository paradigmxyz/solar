use super::*;
use lsp_types::{
    DocumentSymbolParams, GotoDefinitionParams, HoverParams, ReferenceContext, ReferenceParams,
    RenameParams, TextDocumentPositionParams,
};

fn fixture() -> (TestProject, GlobalState, Url) {
    let project = TestProject::from_fixture("//- /Request.sol open\ncontract Before {}\n");
    let result = analyze_single_batch(&snapshot(&project));
    assert!(result.diagnostics.is_empty());
    let uri = Url::from_file_path(project.path("/Request.sol")).unwrap();
    let state = project_state(&project);
    state.symbol_tables.store(Arc::new(result.symbol_tables));
    (project, state, uri)
}

fn position(uri: &Url) -> TextDocumentPositionParams {
    TextDocumentPositionParams {
        text_document: TextDocumentIdentifier::new(uri.clone()),
        position: Position::new(0, 9),
    }
}

fn hover_params(uri: &Url) -> HoverParams {
    HoverParams {
        text_document_position_params: position(uri),
        work_done_progress_params: Default::default(),
    }
}

fn rename_params(uri: &Url) -> RenameParams {
    RenameParams {
        text_document_position: position(uri),
        new_name: "Renamed".into(),
        work_done_progress_params: Default::default(),
    }
}

fn request_foreground(state: &mut GlobalState, uri: &Url, method: &str) {
    let goto = GotoDefinitionParams {
        text_document_position_params: position(uri),
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    };
    match method {
        "hover" => drop(crate::handlers::hover(state, hover_params(uri))),
        "definition" => drop(crate::handlers::goto_definition(state, goto)),
        "typeDefinition" => drop(crate::handlers::goto_type_definition(state, goto)),
        "declaration" => drop(crate::handlers::goto_declaration(state, goto)),
        "implementation" => drop(crate::handlers::goto_implementation(state, goto)),
        "references" => drop(crate::handlers::references(
            state,
            ReferenceParams {
                text_document_position: position(uri),
                context: ReferenceContext { include_declaration: true },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            },
        )),
        "prepareRename" => drop(crate::handlers::prepare_rename(state, position(uri))),
        "rename" => drop(crate::handlers::rename(state, rename_params(uri))),
        _ => unreachable!(),
    }
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn foreground_requests_end_the_pending_source_change_debounce() {
    for method in [
        "hover",
        "definition",
        "typeDefinition",
        "declaration",
        "implementation",
        "references",
        "prepareRename",
        "rename",
    ] {
        let (_project, mut state, uri) = fixture();
        change_document(&mut state, &uri, 1, "contract After {}");
        let coordinator = analysis_coordinator(&state);
        state.analysis_scheduler.gate.close();
        tokio::time::sleep(state.config.source_change_debounce() / 2).await;
        let start = tokio::time::Instant::now();
        let tick = Duration::from_millis(1);

        request_foreground(&mut state, &uri, method);
        tokio::time::sleep(tick).await;

        assert_eq!(tokio::time::Instant::now(), start + tick);
        assert!(coordinator.is_finished(), "{method} should reach the scheduler immediately");
        assert_eq!(*state.published_analysis_version.borrow(), 0);
    }
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn document_symbols_and_diagnostics_retain_the_source_change_debounce() {
    let (_project, mut state, uri) = fixture();
    change_document(&mut state, &uri, 1, "contract After {}");
    let mut symbols = std::pin::pin!(crate::handlers::document_symbol(
        &mut state,
        DocumentSymbolParams {
            text_document: TextDocumentIdentifier::new(uri.clone()),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        },
    ));
    let mut diagnostics = std::pin::pin!(crate::handlers::document_diagnostic(
        &mut state,
        document_diagnostic_params(uri, None),
    ));
    let mut cx = Context::from_waker(Waker::noop());
    assert!(symbols.as_mut().poll(&mut cx).is_pending());
    assert!(diagnostics.as_mut().poll(&mut cx).is_pending());
    let coordinator = analysis_coordinator(&state);
    let tick = Duration::from_millis(1);
    state.analysis_scheduler.gate.close();

    tokio::time::sleep(state.config.source_change_debounce() - tick).await;
    assert!(!coordinator.is_finished(), "background requests should preserve the debounce");

    tokio::time::sleep(tick * 2).await;
    assert!(coordinator.is_finished());
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn repeated_foreground_requests_wait_for_one_fresh_analysis() {
    let (_project, mut state, uri) = fixture();
    let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
    change_document(&mut state, &uri, 1, "contract After {}");
    let version = state.analysis_version.load(Ordering::Acquire);
    let coordinator = analysis_coordinator(&state);
    let mut hover = std::pin::pin!(crate::handlers::hover(&mut state, hover_params(&uri)));
    let pending = hover.as_mut().poll(&mut Context::from_waker(Waker::noop())).is_pending();
    for _ in 0..8 {
        request_foreground(&mut state, &uri, "definition");
    }
    tokio::time::sleep(Duration::from_millis(1)).await;
    let (waiting_at_gate, same_coordinator, debounce_ended) = {
        let tasks = state.analysis_scheduler.tasks.lock();
        (
            tasks.worker.is_none() && !coordinator.is_finished(),
            tasks.coordinator.as_ref().is_some_and(|(_, task)| task.id() == coordinator.id()),
            tasks.debounce.is_none(),
        )
    };
    let unpublished = *state.published_analysis_version.borrow() == 0;
    let old_symbols_visible = !state.symbol_tables.load().workspace_symbols("Before").is_empty();
    let unchanged_epoch = state.analysis_version.load(Ordering::Acquire) == version;
    drop(gate);
    tokio::time::resume();

    let response = tokio::time::timeout(ASYNC_TEST_TIMEOUT, hover)
        .await
        .expect("foreground analysis should finish after the worker gate opens")
        .unwrap();
    assert!(pending && waiting_at_gate && same_coordinator && debounce_ended);
    assert!(unpublished && old_symbols_visible && unchanged_epoch);
    assert!(response.is_some());
    let tables = state.symbol_tables.load();
    assert_eq!(workspace_symbol_names(&tables), ["After"]);
    assert_eq!(response, tables.hover(&uri, position(&uri).position));
    assert_eq!(*state.published_analysis_version.borrow(), version);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn edits_after_foreground_urgency_restart_the_full_debounce() {
    let (_project, mut state, uri) = fixture();
    let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
    change_document(&mut state, &uri, 1, "contract Intermediate {}");
    let first_coordinator = analysis_coordinator(&state);
    let hover = crate::handlers::hover(&mut state, hover_params(&uri));
    let tick = Duration::from_millis(1);
    tokio::time::sleep(tick).await;

    change_document(&mut state, &uri, 2, "contract Latest {}");
    let latest_coordinator = analysis_coordinator(&state);
    drop(gate);
    tokio::time::sleep(state.config.source_change_debounce() - tick).await;

    assert!(first_coordinator.is_finished());
    assert!(!latest_coordinator.is_finished(), "the new edit should receive a full debounce");
    assert!(state.analysis_scheduler.tasks.lock().worker.is_none());
    assert_eq!(*state.published_analysis_version.borrow(), 0);
    assert!(state.symbol_tables.load().workspace_symbols("Intermediate").is_empty());

    tokio::time::sleep(tick * 2).await;
    tokio::time::resume();
    let response = tokio::time::timeout(ASYNC_TEST_TIMEOUT, hover)
        .await
        .expect("the earlier foreground request should be invalidated")
        .unwrap_err();
    assert_eq!(response.code, ErrorCode::CONTENT_MODIFIED);
    let response = tokio::time::timeout(
        ASYNC_TEST_TIMEOUT,
        crate::handlers::hover(&mut state, hover_params(&uri)),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(response.is_some());
    let tables = state.symbol_tables.load();
    assert_eq!(workspace_symbol_names(&tables), ["Latest"]);
    assert_eq!(response, tables.hover(&uri, position(&uri).position));
    assert_eq!(
        *state.published_analysis_version.borrow(),
        state.analysis_version.load(Ordering::Acquire),
    );
}

#[tokio::test(flavor = "current_thread")]
async fn requests_recheck_freshness_after_analysis_wakes_them() {
    let (_project, mut state, uri) = fixture();
    let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
    change_document(&mut state, &uri, 1, "contract Intermediate {}");
    let mut hover = std::pin::pin!(crate::handlers::hover(&mut state, hover_params(&uri)));
    let mut diagnostics = std::pin::pin!(crate::handlers::document_diagnostic(
        &mut state,
        document_diagnostic_params(uri.clone(), None),
    ));
    let mut cx = Context::from_waker(Waker::noop());
    assert!(hover.as_mut().poll(&mut cx).is_pending());
    assert!(diagnostics.as_mut().poll(&mut cx).is_pending());

    let mut snapshot = state.snapshot();
    let result = analyze(snapshot.analysis_batches(Vec::new()).pop().unwrap());
    assert!(snapshot.publish_analysis(state.analysis_version.load(Ordering::Acquire), result));
    change_document(&mut state, &uri, 2, "contract Latest {}");

    let Poll::Ready(Err(error)) = hover.as_mut().poll(&mut cx) else {
        panic!("a superseded hover must finish without waiting for more edits");
    };
    assert_eq!(error.code, ErrorCode::CONTENT_MODIFIED);
    let Poll::Ready(Err(error)) = diagnostics.as_mut().poll(&mut cx) else {
        panic!("a superseded diagnostic request must finish without waiting for more edits");
    };
    assert_eq!(error.code, ErrorCode::SERVER_CANCELLED);
    let hover = crate::handlers::hover(&mut state, hover_params(&uri));
    let diagnostics = crate::handlers::document_diagnostic(
        &mut state,
        document_diagnostic_params(uri.clone(), None),
    );
    drop(gate);
    state.prioritize_pending_analysis();
    let response = tokio::time::timeout(ASYNC_TEST_TIMEOUT, hover).await.unwrap().unwrap();
    tokio::time::timeout(ASYNC_TEST_TIMEOUT, diagnostics).await.unwrap().unwrap();
    assert!(response.is_some());
    assert_eq!(response, state.symbol_tables.load().hover(&uri, position(&uri).position));
    assert!(state.symbol_tables.load().workspace_symbols("Intermediate").is_empty());
    assert_eq!(
        *state.published_analysis_version.borrow(),
        state.analysis_version.load(Ordering::Acquire),
    );
}

#[tokio::test(flavor = "current_thread")]
async fn rapid_edits_and_saves_in_a_large_workspace_publish_the_latest_analysis() {
    let mut source = String::from("//- /Request.sol open\ncontract Before {}\n");
    for index in 0..256 {
        source.push_str(&format!(
            "//- /Contract{index}.sol\ncontract Contract{index} {{ function value() external pure returns (uint) {{ return {index}; }} }}\n"
        ));
    }
    let project = TestProject::from_fixture(&source);
    let uri = Url::from_file_path(project.path("/Request.sol")).unwrap();
    let mut state = project_state(&project);
    change_document(&mut state, &uri, 1, "contract Intermediate {}");
    state.prioritize_pending_analysis();
    wait_for_analysis(&state).await;

    // Keep the ready request unpolled while the editor sends another burst of changes.
    let diagnostics = crate::handlers::document_diagnostic(
        &mut state,
        document_diagnostic_params(uri.clone(), None),
    );
    for version in 2..=201 {
        let text = if version % 2 == 0 { "contract Broken {" } else { "contract Latest {}" };
        change_document(&mut state, &uri, version, text);
        project.write_file("/Request.sol", text);
        save_document(&mut state, &uri);
        if version % 10 == 0 {
            state.prioritize_pending_analysis();
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }
    let mut diagnostics = std::pin::pin!(diagnostics);
    let Poll::Ready(Err(error)) =
        diagnostics.as_mut().poll(&mut Context::from_waker(Waker::noop()))
    else {
        panic!("the earlier request must finish despite continued edits");
    };
    assert_eq!(error.code, ErrorCode::SERVER_CANCELLED);
    let diagnostics = crate::handlers::document_diagnostic(
        &mut state,
        document_diagnostic_params(uri.clone(), None),
    );
    state.prioritize_pending_analysis();
    let report = tokio::time::timeout(ASYNC_TEST_TIMEOUT, diagnostics).await.unwrap().unwrap();
    let DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(report)) = report
    else {
        panic!("expected a full diagnostic report");
    };
    assert!(report.full_document_diagnostic_report.items.is_empty());
    assert_eq!(
        *state.published_analysis_version.borrow(),
        state.analysis_version.load(Ordering::Acquire),
    );
    let tables = state.symbol_tables.load();
    assert!(tables.workspace_symbols("Intermediate").is_empty());
    assert!(tables.workspace_symbols("Broken").is_empty());
    assert!(tables.workspace_symbols("Latest").iter().any(|symbol| symbol.name == "Latest"));
    assert_eq!(tables.workspace_symbols("Contract").len(), 256);
}

#[tokio::test(flavor = "current_thread")]
async fn pending_rename_rejects_a_different_identifier_at_the_same_position() {
    let (_project, mut state, uri) = fixture();
    let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
    change_document(&mut state, &uri, 1, "contract Original {}");
    let mut rename = std::pin::pin!(crate::handlers::rename(&mut state, rename_params(&uri)));
    let mut prepare = std::pin::pin!(crate::handlers::prepare_rename(&mut state, position(&uri)));
    let mut cx = Context::from_waker(Waker::noop());
    assert!(rename.as_mut().poll(&mut cx).is_pending());
    assert!(prepare.as_mut().poll(&mut cx).is_pending());
    change_document(&mut state, &uri, 2, "contract Replaced {}");
    drop(gate);
    state.prioritize_pending_analysis();
    wait_for_analysis(&state).await;

    let error = tokio::time::timeout(ASYNC_TEST_TIMEOUT, rename).await.unwrap().unwrap_err();
    assert_eq!(error.code, ErrorCode::CONTENT_MODIFIED);
    let error = tokio::time::timeout(ASYNC_TEST_TIMEOUT, prepare).await.unwrap().unwrap_err();
    assert_eq!(error.code, ErrorCode::CONTENT_MODIFIED);
}

#[tokio::test(flavor = "current_thread")]
async fn superseded_requests_wake_without_analysis_publication() {
    let (_project, mut state, uri) = fixture();
    let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
    change_document(&mut state, &uri, 1, "contract First {}");
    let hover = tokio::spawn(crate::handlers::hover(&mut state, hover_params(&uri)));
    tokio::task::yield_now().await;
    assert!(!hover.is_finished());

    change_document(&mut state, &uri, 2, "contract Second {}");
    let result = tokio::time::timeout(ASYNC_TEST_TIMEOUT, hover).await;
    assert_eq!(*state.published_analysis_version.borrow(), 0);
    drop(gate);
    let error = result
        .expect("invalidation must wake the request without publication")
        .unwrap()
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::CONTENT_MODIFIED);
}

#[tokio::test(flavor = "current_thread")]
async fn pending_rename_rejects_disk_changes_after_analysis_catches_up() {
    let (project, mut state, uri) = fixture();
    let path = project.path("/Request.sol");
    state.vfs.write().set_file_contents(path.clone().into(), None);
    project.write_file("/Request.sol", "contract Original {}");
    let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
    state.recompute_with_disk_files(vec![path.clone()]);
    let rename = crate::handlers::rename(&mut state, rename_params(&uri));
    let revision = state.vfs.read().content_revision();
    project.write_file("/Request.sol", "contract Replaced {}");
    state.recompute_with_disk_files(vec![path]);
    drop(gate);
    state.prioritize_pending_analysis();
    wait_for_analysis(&state).await;
    assert_eq!(state.vfs.read().content_revision(), revision);
    let error = tokio::time::timeout(ASYNC_TEST_TIMEOUT, rename).await.unwrap().unwrap_err();
    assert_eq!(error.code, ErrorCode::CONTENT_MODIFIED);
}

#[tokio::test(flavor = "current_thread")]
async fn content_identical_edits_preserve_pending_requests() {
    let (_project, mut state, uri) = fixture();
    let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
    change_document(&mut state, &uri, 1, "contract Latest {}");
    let hover = crate::handlers::hover(&mut state, hover_params(&uri));
    change_document(&mut state, &uri, 2, "contract Latest {}");
    drop(gate);
    let response = tokio::time::timeout(ASYNC_TEST_TIMEOUT, hover).await.unwrap().unwrap();
    assert!(response.is_some());
}
