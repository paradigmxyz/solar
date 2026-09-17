use super::*;
use lsp_types::{
    CompletionParams, CompletionResponse, DocumentSymbolParams, GotoDefinitionParams, HoverParams,
    ReferenceContext, ReferenceParams, RenameParams, SignatureHelpParams,
    TextDocumentPositionParams,
};

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

#[tokio::test(flavor = "current_thread")]
async fn completes_using_for_after_adding_a_local_variable() {
    for (using, local) in [("using Math for uint256;", "// local"), ("// using", "uint256 x;")] {
        for expression in ["x.", "x.tw"] {
            let (_project, mut state, uri) = fixture();
            let source = format!(
                r#"library Math {{
    function twice(uint256 value) internal pure returns (uint256) {{ return value * 2; }}
}}
contract C {{
    {using}
    function f() public pure {{
        {local}
        // completion
    }}
}}"#,
            );
            change(&mut state, &uri, 1, &source);
            state.prioritize_pending_analysis();
            tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
                .await
                .unwrap()
                .unwrap();

            let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
            let changed = source
                .replace("// using", "using Math for uint256;")
                .replace("// local", "uint256 x;")
                .replace("// completion", expression);
            change(&mut state, &uri, 2, &changed);
            let mut completion = std::pin::pin!(crate::handlers::completion(
                &mut state,
                CompletionParams {
                    text_document_position: TextDocumentPositionParams {
                        text_document: TextDocumentIdentifier::new(uri),
                        position: Position::new(7, 8 + expression.len() as u32),
                    },
                    work_done_progress_params: Default::default(),
                    partial_result_params: Default::default(),
                    context: None,
                },
            ));
            assert!(completion.as_mut().poll(&mut Context::from_waker(Waker::noop())).is_pending());
            drop(gate);
            let response = tokio::time::timeout(ASYNC_TEST_TIMEOUT, completion)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            let CompletionResponse::Array(items) = response else {
                panic!("expected completion array")
            };
            assert_eq!(items.iter().map(|item| item.label.as_str()).collect::<Vec<_>>(), ["twice"]);
        }
    }
}

async fn using_fixture() -> (TestProject, GlobalState, Url, String) {
    let (project, mut state, uri) = fixture();
    let source = r#"library Math {
    function twice(uint256 value) internal pure returns (uint256) { return value * 2; }
    function triple(uint256 value) internal pure returns (uint256) { return value * 3; }
}
contract C {
    using {Math.twice} for uint256;
    function f() public pure {
        uint256 x;
        // completion
    }
}"#
    .to_owned();
    change(&mut state, &uri, 1, &source);
    state.prioritize_pending_analysis();
    tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis()).await.unwrap().unwrap();
    (project, state, uri, source)
}

fn completion_params(uri: &Url, character: u32) -> CompletionParams {
    CompletionParams {
        text_document_position: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier::new(uri.clone()),
            position: Position::new(8, character),
        },
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
        context: None,
    }
}

#[tokio::test(flavor = "current_thread")]
async fn completions_refresh_nonempty_lists_and_local_names() {
    for (using, local, expression, expected) in [
        ("using Math for uint256;", "uint256 x;", "x.", vec!["triple", "twice"]),
        ("using {Math.triple} for uint256;", "uint256 x;", "x.", vec!["triple"]),
        ("using {Math.twice} for uint256;", "uint256 newLocal;", "newL", vec!["newLocal"]),
    ] {
        let (_project, mut state, uri, source) = using_fixture().await;
        let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
        let changed = source
            .replace("using {Math.twice} for uint256;", using)
            .replace("uint256 x;", local)
            .replace("// completion", expression);
        change(&mut state, &uri, 2, &changed);
        let mut completion = std::pin::pin!(crate::handlers::completion(
            &mut state,
            completion_params(&uri, 8 + expression.len() as u32),
        ));
        assert!(completion.as_mut().poll(&mut Context::from_waker(Waker::noop())).is_pending());
        drop(gate);
        let response =
            tokio::time::timeout(ASYNC_TEST_TIMEOUT, completion).await.unwrap().unwrap().unwrap();
        let CompletionResponse::Array(items) = response else {
            panic!("expected completion array")
        };
        assert_eq!(items.iter().map(|item| item.label.as_str()).collect::<Vec<_>>(), expected);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn signature_help_waits_for_a_new_attached_call() {
    let (_project, mut state, uri, source) = using_fixture().await;
    let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
    change(&mut state, &uri, 2, &source.replace("// completion", "x.twice("));
    let mut signature = std::pin::pin!(crate::handlers::signature_help(
        &mut state,
        SignatureHelpParams {
            text_document_position_params: completion_params(&uri, 16).text_document_position,
            work_done_progress_params: Default::default(),
            context: None,
        },
    ));
    assert!(signature.as_mut().poll(&mut Context::from_waker(Waker::noop())).is_pending());
    drop(gate);
    let response =
        tokio::time::timeout(ASYNC_TEST_TIMEOUT, signature).await.unwrap().unwrap().unwrap();
    snapbox::assert_data_eq!(
        response.signatures[0].label.as_str(),
        snapbox::str!["function twice() internal pure returns (uint256)"]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn completion_and_signature_help_reject_superseding_edits() {
    let (_project, mut state, uri, source) = using_fixture().await;
    let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
    change(&mut state, &uri, 2, &source.replace("// completion", "x.twice("));
    let mut completion =
        std::pin::pin!(crate::handlers::completion(&mut state, completion_params(&uri, 10)));
    let mut signature = std::pin::pin!(crate::handlers::signature_help(
        &mut state,
        SignatureHelpParams {
            text_document_position_params: completion_params(&uri, 16).text_document_position,
            work_done_progress_params: Default::default(),
            context: None,
        },
    ));
    let mut cx = Context::from_waker(Waker::noop());
    assert!(completion.as_mut().poll(&mut cx).is_pending());
    assert!(signature.as_mut().poll(&mut cx).is_pending());
    change(&mut state, &uri, 3, &source.replace("// completion", "x.twice();"));
    let Poll::Ready(Err(error)) = completion.as_mut().poll(&mut cx) else {
        panic!("stale completion must be rejected")
    };
    assert_eq!(error.code, ErrorCode::CONTENT_MODIFIED);
    let Poll::Ready(Err(error)) = signature.as_mut().poll(&mut cx) else {
        panic!("stale signature help must be rejected")
    };
    assert_eq!(error.code, ErrorCode::CONTENT_MODIFIED);
    drop(gate);
}

#[tokio::test(flavor = "current_thread")]
async fn natspec_and_import_completions_wait_for_analysis() {
    for (source, position) in
        [("///\ncontract C {}", Position::new(0, 3)), ("import \"./\";", Position::new(0, 10))]
    {
        let (_project, mut state, uri) = fixture();
        Arc::make_mut(&mut state.config).enable_completion_snippets();
        let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
        change(&mut state, &uri, 1, source);
        let mut params = completion_params(&uri, 0);
        params.text_document_position.position = position;
        let mut completion = std::pin::pin!(crate::handlers::completion(&mut state, params));
        assert!(completion.as_mut().poll(&mut Context::from_waker(Waker::noop())).is_pending());
        drop(gate);
        let response = tokio::time::timeout(ASYNC_TEST_TIMEOUT, completion).await.unwrap().unwrap();
        assert!(response.is_some());
    }
}

#[tokio::test(flavor = "current_thread")]
async fn completion_rejects_results_after_analysis_failure() {
    let (_project, mut state, uri) = fixture();
    let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
    change(&mut state, &uri, 1, "///\ncontract C {}");
    let mut params = completion_params(&uri, 0);
    params.text_document_position.position = Position::new(0, 3);
    handle_analysis_failure(
        state.analysis_version.load(Ordering::Acquire),
        "test analysis failure",
        &state.analysis_version,
        &state.published_analysis_version,
        &state.analysis_commit,
    );
    let error = crate::handlers::completion(&mut state, params).await.unwrap_err();
    assert_eq!(error.code, ErrorCode::CONTENT_MODIFIED);
    drop(gate);
}

#[tokio::test(flavor = "current_thread")]
async fn completion_uses_changed_import_without_publishing_unrelated_files() {
    let marked = MarkedProject::from_fixture(
        r#"
        //- /Library.sol open
        library L { function oldMethod(uint x) internal pure returns (uint) { return x; } }
        //- /Request.sol open
        import {L} from "./Library.sol";
        contract C {
            using L for uint;
            function f() public pure {
                uint x;
                x.$1;
            }
        }
        //- /Unrelated.sol
        this is not Solidity
        "#,
    );
    let project = marked.project();
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(project.config());
    *state.vfs.write() = project.vfs();
    let uri = Url::from_file_path(project.path("/Request.sol")).unwrap();
    let params = lsp_types::CompletionParams {
        text_document_position: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier::new(uri.clone()),
            position: marked.marker("$1").position(),
        },
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
        context: None,
    };
    let before = state.analysis_for_request(&uri, None).await.unwrap();
    state.symbol_tables.store(before.clone());
    state.analysis_commit.lock().vfs_content_revision = state.vfs.read().content_revision();
    let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
    let library = Url::from_file_path(project.path("/Library.sol")).unwrap();
    for (version, name) in [(1, "oldMethod"), (2, "newMethod")] {
        let source = project.read_file("/Library.sol").replace("oldMethod", name);
        change(&mut state, &library, version, &source);
        let response = tokio::time::timeout(
            ASYNC_TEST_TIMEOUT,
            crate::handlers::completion(&mut state, params.clone()),
        )
        .await
        .expect("completion must not wait for the workspace gate")
        .unwrap()
        .unwrap();
        let lsp_types::CompletionResponse::Array(items) = response else {
            panic!("expected items")
        };
        assert_eq!(items.iter().map(|item| item.label.as_str()).collect::<Vec<_>>(), [name]);
        assert!(Arc::ptr_eq(&before, &state.symbol_tables.load_full()));
        assert_eq!(*state.published_analysis_version.borrow(), 0);
        assert!(state.diagnostics.read().workspace_pull_reports(Vec::new()).is_empty());
    }
    state.clear_analysis_cache();
    drop(gate);
}

#[tokio::test(flavor = "current_thread")]
async fn signature_help_uses_new_parameters_without_workspace_publication() {
    let marked = MarkedProject::from_fixture(
        r#"
        //- /Request.sol open
        contract C {
            function target(uint oldValue) internal pure {}
            function f() public pure { target($1); }
        }
        "#,
    );
    let project = marked.project();
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(project.config());
    *state.vfs.write() = project.vfs();
    let uri = Url::from_file_path(project.path("/Request.sol")).unwrap();
    let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
    change(&mut state, &uri, 1, &project.read_file("/Request.sol").replace("oldValue", "newValue"));
    let response = tokio::time::timeout(
        ASYNC_TEST_TIMEOUT,
        crate::handlers::signature_help(
            &mut state,
            lsp_types::SignatureHelpParams {
                text_document_position_params: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier::new(uri),
                    position: marked.marker("$1").position(),
                },
                work_done_progress_params: Default::default(),
                context: None,
            },
        ),
    )
    .await
    .expect("signature help must not wait for the workspace gate")
    .unwrap()
    .unwrap();
    assert_eq!(response.signatures[0].label, "function target(uint256 newValue) internal pure");
    assert_eq!(*state.published_analysis_version.borrow(), 0);
    state.clear_analysis_cache();
    drop(gate);
}

#[test]
fn request_analysis_rejects_edits_while_queued() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .unwrap();
    runtime.block_on(async {
        let (_project, state, uri) = fixture();
        let (release, worker) = pause_blocking_pool();
        let mut request = std::pin::pin!(state.analysis_for_request(&uri, None));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(request.as_mut().poll(&mut cx).is_pending());
        state.mark_analysis_pending_for_test();
        let result = tokio::time::timeout(ASYNC_TEST_TIMEOUT, request).await;
        release.send(()).unwrap();
        worker.await.unwrap();
        let error = result.expect("invalidation should not wait for the worker").unwrap_err();
        assert_eq!(error.code, ErrorCode::CONTENT_MODIFIED);
    });
}

#[test]
fn request_analysis_rejects_failed_configuration_epoch() {
    let (_project, state, uri) = fixture();
    state.mark_analysis_pending_for_test();
    state.analysis_commit.lock().cache_invalidated = true;
    state.published_analysis_version.send_replace(state.analysis_version.load(Ordering::Acquire));
    let error = expect_ready(state.analysis_for_request(&uri, None)).unwrap_err();
    assert_eq!(error.code, ErrorCode::CONTENT_MODIFIED);
}
