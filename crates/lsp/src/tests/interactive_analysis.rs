use super::*;
use lsp_types::{
    DocumentSymbolParams, GotoDefinitionParams, HoverParams, ReferenceContext, ReferenceParams,
    RenameParams, TextDocumentPositionParams,
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
        .expect("the earlier foreground request should receive the newest analysis")
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
