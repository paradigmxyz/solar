use super::*;
use crate::test_support::start_request;
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

const USING_SOURCE: &str = r#"library Math {
    function twice(uint256 value) internal pure returns (uint256) { return value * 2; }
    function triple(uint256 value) internal pure returns (uint256) { return value * 3; }
}
contract C {
    using {Math.twice} for uint256;
    function f() public pure {
        uint256 x;
        // completion
    }
}"#;

async fn using_fixture() -> (TestProject, GlobalState, Url) {
    let (project, mut state, uri) = fixture();
    change(&mut state, &uri, 1, USING_SOURCE);
    state.prioritize_pending_analysis();
    tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis()).await.unwrap().unwrap();
    (project, state, uri)
}

fn completion_params(uri: &Url, position: Position) -> CompletionParams {
    CompletionParams {
        text_document_position: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier::new(uri.clone()),
            position,
        },
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
        context: None,
    }
}

fn signature_params(uri: &Url, position: Position) -> SignatureHelpParams {
    SignatureHelpParams {
        text_document_position_params: completion_params(uri, position).text_document_position,
        work_done_progress_params: Default::default(),
        context: None,
    }
}

fn completion_labels(response: Option<CompletionResponse>) -> Vec<String> {
    let Some(CompletionResponse::Array(items)) = response else {
        panic!("expected completion array")
    };
    items.into_iter().map(|item| item.label).collect()
}

#[tokio::test(flavor = "current_thread")]
async fn completion_refreshes_local_names_and_using_methods_after_edits() {
    for (changed, expression, expected) in [
        (
            USING_SOURCE.replace("uint256 x;", "uint256 x; uint256 addedLocal;"),
            "addedL",
            vec!["addedLocal"],
        ),
        (USING_SOURCE.replace("uint256 x;", "uint256 newLocal;"), "newL", vec!["newLocal"]),
        (USING_SOURCE.replace("{Math.twice}", "Math"), "x.", vec!["triple", "twice"]),
        (USING_SOURCE.replace("{Math.twice}", "{Math.triple}"), "x.", vec!["triple"]),
        (USING_SOURCE.replace("using {Math.twice} for uint256;", ""), "x.", vec![]),
    ] {
        let (_project, mut state, uri) = using_fixture().await;
        let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
        change(&mut state, &uri, 2, &changed.replace("// completion", expression));
        let mut request = std::pin::pin!(crate::handlers::completion(
            &mut state,
            completion_params(&uri, Position::new(8, 8 + expression.len() as u32)),
        ));
        assert!(request.as_mut().poll(&mut Context::from_waker(Waker::noop())).is_pending());
        assert!(state.analysis_scheduler.tasks.lock().debounce.is_none());
        drop(gate);
        let response = tokio::time::timeout(ASYNC_TEST_TIMEOUT, request).await.unwrap().unwrap();
        assert_eq!(completion_labels(response), expected);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn signature_help_waits_for_a_new_attached_call() {
    let (_project, mut state, uri) = using_fixture().await;
    let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
    change(&mut state, &uri, 2, &USING_SOURCE.replace("// completion", "x.twice("));
    let mut request = std::pin::pin!(crate::handlers::signature_help(
        &mut state,
        signature_params(&uri, Position::new(8, 16)),
    ));
    assert!(request.as_mut().poll(&mut Context::from_waker(Waker::noop())).is_pending());
    assert!(state.analysis_scheduler.tasks.lock().debounce.is_none());
    drop(gate);
    let response =
        tokio::time::timeout(ASYNC_TEST_TIMEOUT, request).await.unwrap().unwrap().unwrap();
    snapbox::assert_data_eq!(
        response.signatures[0].label.as_str(),
        snapbox::str!["function twice() internal pure returns (uint256)"]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn completion_and_signature_help_reject_failed_analysis() {
    let (_project, mut state, uri) = using_fixture().await;
    let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
    change(&mut state, &uri, 2, &USING_SOURCE.replace("// completion", "x.twice("));
    let completion = start_request(crate::handlers::completion(
        &mut state,
        completion_params(&uri, Position::new(8, 10)),
    ));
    let signature = start_request(crate::handlers::signature_help(
        &mut state,
        signature_params(&uri, Position::new(8, 16)),
    ));
    handle_analysis_failure(
        state.analysis_version.load(Ordering::Acquire),
        "test analysis failure",
        &state.analysis_version,
        &state.published_analysis_version,
        &state.analysis_commit,
    );
    assert_eq!(completion.await.unwrap_err().code, ErrorCode::REQUEST_FAILED);
    assert_eq!(signature.await.unwrap_err().code, ErrorCode::REQUEST_FAILED);
    for clear in [false, true] {
        if clear {
            // Clearing publishes the current epoch, but does not make its empty table usable.
            state.clear_analysis_cache();
        }
        let completion = expect_ready(crate::handlers::completion(
            &mut state,
            completion_params(&uri, Position::new(8, 10)),
        ));
        let signature = expect_ready(crate::handlers::signature_help(
            &mut state,
            signature_params(&uri, Position::new(8, 16)),
        ));
        assert_eq!(completion.unwrap_err().code, ErrorCode::REQUEST_FAILED);
        assert_eq!(signature.unwrap_err().code, ErrorCode::REQUEST_FAILED);
    }
    drop(gate);
}

#[tokio::test(flavor = "current_thread")]
async fn completion_and_signature_help_reject_superseded_inputs() {
    for configuration in [false, true] {
        for published in [false, true] {
            let (_project, mut state, uri) = using_fixture().await;
            let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
            change(&mut state, &uri, 2, &USING_SOURCE.replace("// completion", "x.twice("));
            let completion = start_request(crate::handlers::completion(
                &mut state,
                completion_params(&uri, Position::new(8, 10)),
            ));
            let signature = start_request(crate::handlers::signature_help(
                &mut state,
                signature_params(&uri, Position::new(8, 16)),
            ));
            if published {
                let mut snapshot = state.snapshot();
                let result = analyze(snapshot.analysis_batches(Vec::new()).pop().unwrap());
                assert!(
                    snapshot
                        .publish_analysis(state.analysis_version.load(Ordering::Acquire), result)
                );
            }
            if configuration {
                let _ = crate::handlers::did_change_configuration(
                    &mut state,
                    DidChangeConfigurationParams { settings: serde_json::Value::Null },
                );
            } else {
                change(&mut state, &uri, 3, &USING_SOURCE.replace("// completion", "x.triple("));
            }
            assert_eq!(expect_ready(completion).unwrap_err().code, ErrorCode::CONTENT_MODIFIED);
            assert_eq!(expect_ready(signature).unwrap_err().code, ErrorCode::CONTENT_MODIFIED);
            drop(gate);
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn completion_and_signature_help_refresh_changed_imports() {
    let fixture = support::RequestFixture::new(
        r#"
        //- /Math.sol open
        library Math {
            function twice(uint256 value, uint256 amount) internal pure returns (uint256) {
                return value + amount;
            }
        }
        //- /Request.sol open
        import {Math} from "./Math.sol";
        contract C {
            using Math for uint256;
            function f(uint256 x) public pure {
                x.$1twice($2 1);
            }
        }
        "#,
        "/Request.sol",
    );
    let mut state = fixture.state();
    let (uri, completion_position) = fixture.marker_location("$1");
    let (_, signature_position) = fixture.marker_location("$2");
    let imported = Url::from_file_path(fixture.project_path("/Math.sol")).unwrap();
    let changed = fixture.project_contents("/Math.sol").replace("amount", "count").replace(
        "library Math {",
        "library Math { function triple(uint256 value) internal pure returns (uint256) { return value * 3; }",
    );
    let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
    change(&mut state, &imported, 1, &changed);
    let completion = start_request(crate::handlers::completion(
        &mut state,
        completion_params(&uri, completion_position),
    ));
    let signature = start_request(crate::handlers::signature_help(
        &mut state,
        signature_params(&uri, signature_position),
    ));
    change(&mut state, &imported, 2, &changed.replace("count", "increment"));
    assert_eq!(expect_ready(completion).unwrap_err().code, ErrorCode::CONTENT_MODIFIED);
    assert_eq!(expect_ready(signature).unwrap_err().code, ErrorCode::CONTENT_MODIFIED);
    let completion = start_request(crate::handlers::completion(
        &mut state,
        completion_params(&uri, completion_position),
    ));
    let signature = start_request(crate::handlers::signature_help(
        &mut state,
        signature_params(&uri, signature_position),
    ));
    drop(gate);
    let response = tokio::time::timeout(ASYNC_TEST_TIMEOUT, completion).await.unwrap().unwrap();
    assert_eq!(completion_labels(response), ["triple", "twice"]);
    let response =
        tokio::time::timeout(ASYNC_TEST_TIMEOUT, signature).await.unwrap().unwrap().unwrap();
    snapbox::assert_data_eq!(
        response.signatures[0].label.as_str(),
        snapbox::str!["function twice(uint256 increment) internal pure returns (uint256)"]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn completion_and_signature_help_share_analysis_through_identical_edits() {
    let (_project, mut state, uri) = using_fixture().await;
    let source = USING_SOURCE.replace("// completion", "x.twice(");
    let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
    change(&mut state, &uri, 2, &source);
    let coordinator = state.analysis_scheduler.tasks.lock().coordinator.as_ref().unwrap().1.clone();
    let completion = start_request(crate::handlers::completion(
        &mut state,
        completion_params(&uri, Position::new(8, 10)),
    ));
    let signature = start_request(crate::handlers::signature_help(
        &mut state,
        signature_params(&uri, Position::new(8, 16)),
    ));
    change(&mut state, &uri, 3, &source);
    let second = start_request(crate::handlers::completion(
        &mut state,
        completion_params(&uri, Position::new(8, 10)),
    ));
    assert_eq!(
        state.analysis_scheduler.tasks.lock().coordinator.as_ref().unwrap().1.id(),
        coordinator.id()
    );
    drop(gate);
    let response = tokio::time::timeout(ASYNC_TEST_TIMEOUT, completion).await.unwrap().unwrap();
    assert_eq!(completion_labels(response), ["twice"]);
    assert_eq!(completion_labels(second.await.unwrap()), ["twice"]);
    let response =
        tokio::time::timeout(ASYNC_TEST_TIMEOUT, signature).await.unwrap().unwrap().unwrap();
    let ready_completion = expect_ready(crate::handlers::completion(
        &mut state,
        completion_params(&uri, Position::new(8, 10)),
    ))
    .unwrap();
    assert_eq!(completion_labels(ready_completion), ["twice"]);
    let ready_signature = expect_ready(crate::handlers::signature_help(
        &mut state,
        signature_params(&uri, Position::new(8, 16)),
    ))
    .unwrap()
    .unwrap();
    assert_eq!(response, ready_signature);
}

#[tokio::test(flavor = "current_thread")]
async fn ready_completion_and_signature_help_recheck_inputs_when_polled() {
    let (_project, mut state, uri) = using_fixture().await;
    let completion =
        crate::handlers::completion(&mut state, completion_params(&uri, Position::new(8, 8)));
    let signature =
        crate::handlers::signature_help(&mut state, signature_params(&uri, Position::new(8, 8)));
    let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
    change(&mut state, &uri, 2, &USING_SOURCE.replace("uint256 x;", "uint256 other;"));
    assert_eq!(expect_ready(completion).unwrap_err().code, ErrorCode::CONTENT_MODIFIED);
    assert_eq!(expect_ready(signature).unwrap_err().code, ErrorCode::CONTENT_MODIFIED);
    drop(gate);
}

#[tokio::test(flavor = "current_thread")]
async fn completion_syntax_and_invalid_positions_do_not_wait_for_analysis() {
    let (_project, mut state, uri) = using_fixture().await;
    Arc::make_mut(&mut state.config).enable_completion_snippets();
    let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
    for (version, source, position) in [
        (2, "///\ncontract C {}", Position::new(0, 3)),
        (3, "import \"./\";", Position::new(0, 10)),
    ] {
        change(&mut state, &uri, version, source);
        let response = expect_ready(crate::handlers::completion(
            &mut state,
            completion_params(&uri, position),
        ))
        .unwrap();
        assert!(!completion_labels(response).is_empty());
        assert!(state.analysis_scheduler.tasks.lock().debounce.is_some());
    }
    change(&mut state, &uri, 4, USING_SOURCE);
    for trigger in ["/", "*", "\"", "'"] {
        let mut params = completion_params(&uri, Position::new(7, 17));
        params.context = Some(lsp_types::CompletionContext {
            trigger_kind: lsp_types::CompletionTriggerKind::TRIGGER_CHARACTER,
            trigger_character: Some(trigger.into()),
        });
        let response = expect_ready(crate::handlers::completion(&mut state, params)).unwrap();
        assert!(completion_labels(response).is_empty());
        assert!(state.analysis_scheduler.tasks.lock().debounce.is_some());
    }
    for (request_uri, position) in [
        (uri.clone(), Position::new(u32::MAX, u32::MAX)),
        (uri.join("Missing.sol").unwrap(), Position::new(0, 0)),
    ] {
        let response = expect_ready(crate::handlers::completion(
            &mut state,
            completion_params(&request_uri, position),
        ))
        .unwrap();
        assert!(completion_labels(response).is_empty());
        let response = expect_ready(crate::handlers::signature_help(
            &mut state,
            signature_params(&request_uri, position),
        ))
        .unwrap();
        assert!(response.is_none());
    }
    drop(gate);
}
