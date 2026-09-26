use super::*;
use lsp_types::{
    CodeLens, CodeLensParams, DiagnosticSeverity, DocumentSymbolParams, GotoDefinitionParams,
    HoverParams, InlayHint, InlayHintParams, ReferenceContext, ReferenceParams, RenameParams,
    TextDocumentPositionParams,
};
use serde_json::json;

async fn annotations(state: &mut GlobalState, uri: &Url) -> (Vec<CodeLens>, Vec<InlayHint>) {
    let lenses = crate::handlers::code_lens(
        state,
        CodeLensParams {
            text_document: TextDocumentIdentifier::new(uri.clone()),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        },
    );
    let hints = crate::handlers::inlay_hints(
        state,
        InlayHintParams {
            text_document: TextDocumentIdentifier::new(uri.clone()),
            range: Range::new(Position::new(0, 0), Position::new(u32::MAX, u32::MAX)),
            work_done_progress_params: Default::default(),
        },
    );
    tokio::time::timeout(ASYNC_TEST_TIMEOUT, async {
        (lenses.await.unwrap().unwrap(), hints.await.unwrap().unwrap())
    })
    .await
    .unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn annotations_survive_errors_and_follow_edits() {
    let (_project, mut state, uri) = fixture();
    let source = "contract C {\n\
        function target(uint amount) public pure returns (uint) { return amount; }\n\
        function caller() public pure {\n\
        // editing\n\
        target(1);\n\
        }\n\
        function afterError() external {}\n\
        }\n";
    change(&mut state, &uri, 1, source);
    let (mut expected_lenses, mut expected_hints) = annotations(&mut state, &uri).await;
    assert!(!expected_lenses.is_empty());
    assert_eq!(expected_hints.len(), 2);

    let broken = source.replace("// editing", "uint x = 1 + * 2;");
    change(&mut state, &uri, 2, &broken);
    assert_eq!(
        json!(annotations(&mut state, &uri).await),
        json!((&expected_lenses, &expected_hints))
    );
    let diagnostics = state.diagnostics.read().code_action_diagnostics(
        &uri,
        Range::new(Position::new(0, 0), Position::new(u32::MAX, u32::MAX)),
    );
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic.severity == Some(DiagnosticSeverity::ERROR))
    );

    let shifted = format!("// 🦀\n\n{broken}");
    assert!(
        crate::handlers::did_change_text_document(
            &mut state,
            DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier::new(uri.clone(), 3),
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: Some(Range::new(Position::new(0, 0), Position::new(0, 0))),
                    range_length: None,
                    text: "// 🦀\n\n".into(),
                }],
            },
        )
        .is_continue()
    );
    for lens in &mut expected_lenses {
        lens.range.start.line += 2;
        lens.range.end.line += 2;
    }
    for hint in &mut expected_hints {
        hint.position.line += 2;
    }
    assert_eq!(
        json!(annotations(&mut state, &uri).await),
        json!((&expected_lenses, &expected_hints))
    );

    let deleted = shifted.replace("function afterError() external {}", "");
    change(&mut state, &uri, 4, &deleted);
    expected_lenses.retain(|lens| lens.range.start.line != 8);
    assert_eq!(
        json!(annotations(&mut state, &uri).await),
        json!((&expected_lenses, &expected_hints))
    );

    let fixed = deleted.replace("uint x = 1 + * 2;", "// fixed");
    change(&mut state, &uri, 5, &fixed);
    assert_eq!(
        json!(annotations(&mut state, &uri).await),
        json!((&expected_lenses, &expected_hints))
    );
    let diagnostics = state.diagnostics.read().code_action_diagnostics(
        &uri,
        Range::new(Position::new(0, 0), Position::new(u32::MAX, u32::MAX)),
    );
    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Some(DiagnosticSeverity::ERROR))
    );
}

fn fixture() -> (TestProject, GlobalState, Url) {
    let project = TestProject::from_fixture("//- /Request.sol open\ncontract Before {}\n");
    let mut batches = snapshot(&project).analysis_batches(Vec::new());
    let result = analyze(batches.pop().unwrap());
    assert!(batches.is_empty());
    assert!(result.diagnostics.is_empty());
    let uri = Url::from_file_path(project.path("/Request.sol")).unwrap();
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(project.config());
    state.vfs = Arc::new(RwLock::new(project.vfs()));
    state.symbol_tables.store(Arc::new(result.symbol_tables));
    (project, state, uri)
}

fn change(state: &mut GlobalState, uri: &Url, version: i32, source: &str) {
    assert!(
        crate::handlers::did_change_text_document(
            state,
            DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier::new(uri.clone(), version),
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: source.into(),
                }],
            },
        )
        .is_continue()
    );
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
        "rename" => drop(crate::handlers::rename(
            state,
            RenameParams {
                text_document_position: position(uri),
                new_name: "Renamed".into(),
                work_done_progress_params: Default::default(),
            },
        )),
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
        change(&mut state, &uri, 1, "contract After {}");
        let coordinator =
            state.analysis_scheduler.tasks.lock().coordinator.as_ref().unwrap().1.clone();
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
    change(&mut state, &uri, 1, "contract After {}");
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
    let coordinator = state.analysis_scheduler.tasks.lock().coordinator.as_ref().unwrap().1.clone();
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
    change(&mut state, &uri, 1, "contract After {}");
    let version = state.analysis_version.load(Ordering::Acquire);
    let coordinator = state.analysis_scheduler.tasks.lock().coordinator.as_ref().unwrap().1.clone();
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
    assert!(tables.workspace_symbols("Before").is_empty());
    assert!(tables.workspace_symbols("After").iter().any(|symbol| symbol.name == "After"));
    assert_eq!(response, tables.hover(&uri, position(&uri).position));
    assert_eq!(*state.published_analysis_version.borrow(), version);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn edits_after_foreground_urgency_restart_the_full_debounce() {
    let (_project, mut state, uri) = fixture();
    let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
    change(&mut state, &uri, 1, "contract Intermediate {}");
    let first_coordinator =
        state.analysis_scheduler.tasks.lock().coordinator.as_ref().unwrap().1.clone();
    let hover = crate::handlers::hover(&mut state, hover_params(&uri));
    let tick = Duration::from_millis(1);
    tokio::time::sleep(tick).await;

    change(&mut state, &uri, 2, "contract Latest {}");
    let latest_coordinator =
        state.analysis_scheduler.tasks.lock().coordinator.as_ref().unwrap().1.clone();
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
    assert!(tables.workspace_symbols("Before").is_empty());
    assert!(tables.workspace_symbols("Intermediate").is_empty());
    assert!(tables.workspace_symbols("Latest").iter().any(|symbol| symbol.name == "Latest"));
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
    change(&mut state, &uri, 1, "contract Intermediate {}");
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
    change(&mut state, &uri, 2, "contract Latest {}");

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
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(project.config());
    state.vfs = Arc::new(RwLock::new(project.vfs()));
    change(&mut state, &uri, 1, "contract Intermediate {}");
    state.prioritize_pending_analysis();
    tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis()).await.unwrap().unwrap();

    // Keep the ready request unpolled while the editor sends another burst of changes.
    let diagnostics = crate::handlers::document_diagnostic(
        &mut state,
        document_diagnostic_params(uri.clone(), None),
    );
    for version in 2..=201 {
        let text = if version % 2 == 0 { "contract Broken {" } else { "contract Latest {}" };
        change(&mut state, &uri, version, text);
        project.write_file("/Request.sol", text);
        assert!(
            crate::handlers::did_save_text_document(
                &mut state,
                DidSaveTextDocumentParams {
                    text_document: TextDocumentIdentifier::new(uri.clone()),
                    text: None,
                },
            )
            .is_continue()
        );
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
    change(&mut state, &uri, 1, "contract Original {}");
    let mut rename = std::pin::pin!(crate::handlers::rename(
        &mut state,
        RenameParams {
            text_document_position: position(&uri),
            new_name: "Renamed".into(),
            work_done_progress_params: Default::default(),
        },
    ));
    let mut prepare = std::pin::pin!(crate::handlers::prepare_rename(&mut state, position(&uri)));
    let mut cx = Context::from_waker(Waker::noop());
    assert!(rename.as_mut().poll(&mut cx).is_pending());
    assert!(prepare.as_mut().poll(&mut cx).is_pending());
    change(&mut state, &uri, 2, "contract Replaced {}");
    drop(gate);
    state.prioritize_pending_analysis();
    tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis()).await.unwrap().unwrap();

    let error = tokio::time::timeout(ASYNC_TEST_TIMEOUT, rename).await.unwrap().unwrap_err();
    assert_eq!(error.code, ErrorCode::CONTENT_MODIFIED);
    let error = tokio::time::timeout(ASYNC_TEST_TIMEOUT, prepare).await.unwrap().unwrap_err();
    assert_eq!(error.code, ErrorCode::CONTENT_MODIFIED);
}

#[tokio::test(flavor = "current_thread")]
async fn superseded_requests_wake_without_analysis_publication() {
    let (_project, mut state, uri) = fixture();
    let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
    change(&mut state, &uri, 1, "contract First {}");
    let hover = tokio::spawn(crate::handlers::hover(&mut state, hover_params(&uri)));
    tokio::task::yield_now().await;
    assert!(!hover.is_finished());

    change(&mut state, &uri, 2, "contract Second {}");
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
    let rename = crate::handlers::rename(
        &mut state,
        RenameParams {
            text_document_position: position(&uri),
            new_name: "Renamed".into(),
            work_done_progress_params: Default::default(),
        },
    );
    let revision = state.vfs.read().content_revision();
    project.write_file("/Request.sol", "contract Replaced {}");
    state.recompute_with_disk_files(vec![path]);
    drop(gate);
    state.prioritize_pending_analysis();
    tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis()).await.unwrap().unwrap();
    assert_eq!(state.vfs.read().content_revision(), revision);
    let error = tokio::time::timeout(ASYNC_TEST_TIMEOUT, rename).await.unwrap().unwrap_err();
    assert_eq!(error.code, ErrorCode::CONTENT_MODIFIED);
}

#[tokio::test(flavor = "current_thread")]
async fn content_identical_edits_preserve_pending_requests() {
    let (_project, mut state, uri) = fixture();
    let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
    change(&mut state, &uri, 1, "contract Latest {}");
    let hover = crate::handlers::hover(&mut state, hover_params(&uri));
    change(&mut state, &uri, 2, "contract Latest {}");
    drop(gate);
    let response = tokio::time::timeout(ASYNC_TEST_TIMEOUT, hover).await.unwrap().unwrap();
    assert!(response.is_some());
}
