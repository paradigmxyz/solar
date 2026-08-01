use super::*;
use crate::test_support::TestProject;
use async_lsp::{
    AnyEvent, AnyNotification, AnyRequest, LanguageServer, LspService, ResponseError,
    router::Router,
};
use lsp_types::{
    CallHierarchyIncomingCallsParams, CallHierarchyItem, CallHierarchyOutgoingCallsParams,
    CallHierarchyPrepareParams, CancelParams, CompletionParams, CompletionResponse,
    DidChangeWatchedFilesClientCapabilities, DidChangeWatchedFilesParams,
    DidSaveTextDocumentParams, DocumentFormattingParams, DocumentHighlightParams,
    DocumentLinkParams, DocumentSymbolParams, ExecuteCommandParams, FileChangeType, FileEvent,
    FoldingRangeParams, FormattingOptions, HoverParams, InitializeParams, InitializedParams,
    LogTraceParams, NumberOrString, PartialResultParams, Position, ProgressParams,
    ProgressParamsValue, PublishDiagnosticsParams, Range, SelectionRangeParams, SetTraceParams,
    SignatureHelpParams, SymbolKind, TextDocumentIdentifier, TextDocumentPositionParams,
    TextDocumentSaveReason, TraceValue, WillSaveTextDocumentParams, WindowClientCapabilities,
    WorkDoneProgress, WorkDoneProgressCancelParams, WorkDoneProgressCreateParams,
    WorkDoneProgressParams, WorkspaceClientCapabilities, WorkspaceSymbolParams,
    notification as notif, notification::Notification, request, request::Request,
};
use solar_interface::data_structures::sync::RwLock;
use std::{
    future::Future,
    ops::ControlFlow,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll, Waker},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    sync::{mpsc, oneshot},
};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use tower::Service;

struct ObservedRouter {
    inner: Router<GlobalState>,
    accepted: mpsc::UnboundedSender<String>,
}

impl Service<AnyRequest> for ObservedRouter {
    type Response = serde_json::Value;
    type Error = ResponseError;
    type Future = <Router<GlobalState> as Service<AnyRequest>>::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: AnyRequest) -> Self::Future {
        self.accepted.send(request.method.clone()).unwrap();
        self.inner.call(request)
    }
}

impl LspService for ObservedRouter {
    fn notify(&mut self, notification: AnyNotification) -> ControlFlow<async_lsp::Result<()>> {
        self.inner.notify(notification)
    }

    fn emit(&mut self, event: AnyEvent) -> ControlFlow<async_lsp::Result<()>> {
        self.inner.emit(event)
    }
}

fn assert_request_cancelled<T>(result: async_lsp::Result<T>) {
    let Err(error) = result else { panic!("expected request cancellation") };
    let async_lsp::Error::Response(error) = error else {
        panic!("expected request cancellation, got {error:?}");
    };
    assert_eq!(error.code, async_lsp::ErrorCode::REQUEST_CANCELLED);
}

fn start_request<F: Future>(future: F) -> Pin<Box<F>> {
    let mut future = Box::pin(future);
    let mut cx = Context::from_waker(Waker::noop());
    assert!(future.as_mut().poll(&mut cx).is_pending());
    future
}

#[derive(Debug)]
enum AnalysisClientEvent {
    Create(WorkDoneProgressCreateParams),
    Progress(ProgressParams),
    Diagnostics(PublishDiagnosticsParams),
}

async fn next_analysis_event(
    events: &mut mpsc::UnboundedReceiver<AnalysisClientEvent>,
) -> AnalysisClientEvent {
    tokio::time::timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("analysis client event should arrive")
        .expect("analysis client event channel should stay open")
}

#[tokio::test(flavor = "current_thread")]
async fn router_handles_watched_file_changes() {
    let mut router = new_router(ClientSocket::new_closed());
    let params = DidChangeWatchedFilesParams {
        changes: vec![FileEvent::new(
            lsp_types::Url::parse("file:///workspace/src/Test.sol").unwrap(),
            FileChangeType::CHANGED,
        )],
    };
    let notification = serde_json::from_value::<AnyNotification>(serde_json::json!({
        "method": notif::DidChangeWatchedFiles::METHOD,
        "params": params,
    }))
    .unwrap();

    assert!(matches!(router.notify(notification), ControlFlow::Continue(())));
}

#[tokio::test(flavor = "current_thread")]
async fn router_handles_will_file_operation_requests() {
    let mut router = new_router(ClientSocket::new_closed());

    for method in [
        request::WillCreateFiles::METHOD,
        request::WillRenameFiles::METHOD,
        request::WillDeleteFiles::METHOD,
    ] {
        let request = serde_json::from_value::<AnyRequest>(serde_json::json!({
            "id": 1,
            "method": method,
            "params": { "files": [] },
        }))
        .unwrap();

        assert_eq!(router.call(request).await.unwrap(), serde_json::Value::Null);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn router_handles_did_file_operation_notifications() {
    let mut router = new_router(ClientSocket::new_closed());

    for method in [
        notif::DidCreateFiles::METHOD,
        notif::DidRenameFiles::METHOD,
        notif::DidDeleteFiles::METHOD,
    ] {
        let notification = serde_json::from_value::<AnyNotification>(serde_json::json!({
            "method": method,
            "params": { "files": [] },
        }))
        .unwrap();

        assert!(matches!(router.notify(notification), ControlFlow::Continue(())));
    }
}

#[tokio::test(flavor = "current_thread")]
async fn router_handles_document_saves() {
    let mut router = new_router(ClientSocket::new_closed());
    let params = DidSaveTextDocumentParams {
        text_document: TextDocumentIdentifier {
            uri: lsp_types::Url::parse("file:///workspace/src/Test.sol").unwrap(),
        },
        text: None,
    };
    let notification = serde_json::from_value::<AnyNotification>(serde_json::json!({
        "method": notif::DidSaveTextDocument::METHOD,
        "params": params,
    }))
    .unwrap();

    assert!(matches!(router.notify(notification), ControlFlow::Continue(())));
}

#[tokio::test(flavor = "current_thread")]
async fn router_handles_will_save_notifications() {
    let mut router = new_router(ClientSocket::new_closed());
    let params = WillSaveTextDocumentParams {
        text_document: TextDocumentIdentifier {
            uri: lsp_types::Url::parse("file:///workspace/src/Test.sol").unwrap(),
        },
        reason: TextDocumentSaveReason::MANUAL,
    };
    let notification = serde_json::from_value::<AnyNotification>(serde_json::json!({
        "method": notif::WillSaveTextDocument::METHOD,
        "params": params,
    }))
    .unwrap();

    assert!(matches!(router.notify(notification), ControlFlow::Continue(())));
}

#[tokio::test(flavor = "current_thread")]
async fn router_handles_work_done_progress_cancellation() {
    let mut router = new_router(ClientSocket::new_closed());
    let params = WorkDoneProgressCancelParams {
        token: NumberOrString::String("solar/workspace-index/1".into()),
    };
    let notification = serde_json::from_value::<AnyNotification>(serde_json::json!({
        "method": notif::WorkDoneProgressCancel::METHOD,
        "params": params,
    }))
    .unwrap();

    assert!(matches!(router.notify(notification), ControlFlow::Continue(())));
}

#[tokio::test(flavor = "current_thread")]
async fn router_handles_cache_commands() {
    let mut router = new_router(ClientSocket::new_closed());

    for command in ["solar.clearCache", "solar.reindex"] {
        let params = ExecuteCommandParams { command: command.into(), ..Default::default() };
        let request = serde_json::from_value::<AnyRequest>(serde_json::json!({
            "id": 1,
            "method": request::ExecuteCommand::METHOD,
            "params": params,
        }))
        .unwrap();

        let response = router.call(request).await.unwrap();

        assert_eq!(response, serde_json::json!({ "success": true }));
    }
}

#[tokio::test(flavor = "current_thread")]
async fn router_rejects_unknown_cache_command() {
    let mut router = new_router(ClientSocket::new_closed());
    let params = ExecuteCommandParams { command: "solar.unknown".into(), ..Default::default() };
    let request = serde_json::from_value::<AnyRequest>(serde_json::json!({
        "id": 1,
        "method": request::ExecuteCommand::METHOD,
        "params": params,
    }))
    .unwrap();

    let error = router.call(request).await.unwrap_err();

    assert_eq!(error.code, async_lsp::ErrorCode::INVALID_PARAMS);
}

#[tokio::test(flavor = "current_thread")]
async fn router_handles_document_link_requests() {
    let mut router = new_router(ClientSocket::new_closed());
    let params = DocumentLinkParams {
        text_document: TextDocumentIdentifier {
            uri: lsp_types::Url::parse("file:///workspace/src/Test.sol").unwrap(),
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };
    let request = serde_json::from_value::<AnyRequest>(serde_json::json!({
        "id": 1,
        "method": request::DocumentLinkRequest::METHOD,
        "params": params,
    }))
    .unwrap();

    let response = router.call(request).await.unwrap();

    assert_eq!(response, serde_json::json!([]));
}

#[tokio::test(flavor = "current_thread")]
async fn router_handles_code_lens_requests() {
    let mut router = new_router(ClientSocket::new_closed());
    let params = serde_json::json!({
        "textDocument": { "uri": "file:///workspace/src/Test.sol" },
    });
    let request = serde_json::from_value::<AnyRequest>(serde_json::json!({
        "id": 1,
        "method": request::CodeLensRequest::METHOD,
        "params": params,
    }))
    .unwrap();

    let response = router.call(request).await.unwrap();

    assert_eq!(response, serde_json::json!([]));
}

#[tokio::test(flavor = "current_thread")]
async fn router_handles_folding_range_requests() {
    let project = TestProject::from_fixture("//- /Test.sol open\n");
    let state = GlobalState::new(ClientSocket::new_closed());
    *state.vfs.write() = project.vfs();
    let mut router = new_router_with_state(state);
    let params = FoldingRangeParams {
        text_document: TextDocumentIdentifier {
            uri: lsp_types::Url::from_file_path(project.path("/Test.sol")).unwrap(),
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };
    let request = serde_json::from_value::<AnyRequest>(serde_json::json!({
        "id": 1,
        "method": request::FoldingRangeRequest::METHOD,
        "params": params,
    }))
    .unwrap();

    let response = router.call(request).await.unwrap();

    assert_eq!(response, serde_json::json!([]));
}

#[tokio::test(flavor = "current_thread")]
async fn router_handles_selection_range_requests() {
    let project = TestProject::from_fixture("//- /Test.sol open\n");
    let state = GlobalState::new(ClientSocket::new_closed());
    *state.vfs.write() = project.vfs();
    let mut router = new_router_with_state(state);
    let params = SelectionRangeParams {
        text_document: TextDocumentIdentifier {
            uri: lsp_types::Url::from_file_path(project.path("/Test.sol")).unwrap(),
        },
        positions: Vec::new(),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };
    let request = serde_json::from_value::<AnyRequest>(serde_json::json!({
        "id": 1,
        "method": request::SelectionRangeRequest::METHOD,
        "params": params,
    }))
    .unwrap();

    let response = router.call(request).await.unwrap();

    assert_eq!(response, serde_json::json!([]));
}

#[tokio::test(flavor = "current_thread")]
async fn router_handles_document_diagnostic_requests() {
    let mut router = new_router(ClientSocket::new_closed());
    let uri = lsp_types::Url::parse("untitled:Diagnostics.sol").unwrap();
    let request = serde_json::from_value::<AnyRequest>(serde_json::json!({
        "id": 1,
        "method": request::DocumentDiagnosticRequest::METHOD,
        "params": {
            "textDocument": { "uri": uri },
        },
    }))
    .unwrap();

    let response = router.call(request).await.unwrap();

    assert_eq!(response["kind"], "full");
    assert_eq!(response["items"], serde_json::json!([]));
    let result_id = response["resultId"].as_str().expect("full report should have a result ID");

    let request = serde_json::from_value::<AnyRequest>(serde_json::json!({
        "id": 2,
        "method": request::DocumentDiagnosticRequest::METHOD,
        "params": {
            "textDocument": { "uri": uri },
            "previousResultId": result_id,
        },
    }))
    .unwrap();
    let response = router.call(request).await.unwrap();

    assert_eq!(
        response,
        serde_json::json!({
            "kind": "unchanged",
            "resultId": result_id,
        })
    );
}

#[tokio::test(flavor = "current_thread")]
async fn router_handles_document_highlight_requests() {
    let mut router = new_router(ClientSocket::new_closed());
    let params = DocumentHighlightParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: lsp_types::Url::parse("file:///workspace/src/Test.sol").unwrap(),
            },
            position: Position::new(0, 0),
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };
    let request = serde_json::from_value::<AnyRequest>(serde_json::json!({
        "id": 1,
        "method": request::DocumentHighlightRequest::METHOD,
        "params": params,
    }))
    .unwrap();

    let response = router.call(request).await.unwrap();

    assert_eq!(response, serde_json::Value::Null);
}

#[tokio::test(flavor = "current_thread")]
async fn router_handles_type_hierarchy_requests() {
    let mut router = new_router(ClientSocket::new_closed());
    let uri = lsp_types::Url::parse("untitled:Test.sol").unwrap();
    let item = serde_json::json!({
        "name": "C",
        "kind": SymbolKind::CLASS,
        "uri": uri,
        "range": {
            "start": { "line": 0, "character": 0 },
            "end": { "line": 0, "character": 1 },
        },
        "selectionRange": {
            "start": { "line": 0, "character": 0 },
            "end": { "line": 0, "character": 1 },
        },
    });
    let requests = [
        (
            request::TypeHierarchyPrepare::METHOD,
            serde_json::json!({
                "textDocument": { "uri": uri },
                "position": { "line": 0, "character": 0 },
            }),
        ),
        (request::TypeHierarchySupertypes::METHOD, serde_json::json!({ "item": item })),
        (request::TypeHierarchySubtypes::METHOD, serde_json::json!({ "item": item })),
    ];

    for (id, (method, params)) in requests.into_iter().enumerate() {
        let request = serde_json::from_value::<AnyRequest>(serde_json::json!({
            "id": id,
            "method": method,
            "params": params,
        }))
        .unwrap();
        let response = router.call(request).await.unwrap();
        assert_eq!(response, serde_json::Value::Null, "request `{method}`");
    }
}

#[tokio::test(flavor = "current_thread")]
async fn router_handles_hover_requests() {
    let mut router = new_router(ClientSocket::new_closed());
    let params = HoverParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: lsp_types::Url::parse("file:///workspace/src/Test.sol").unwrap(),
            },
            position: Position::new(0, 0),
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
    };
    let request = serde_json::from_value::<AnyRequest>(serde_json::json!({
        "id": 1,
        "method": request::HoverRequest::METHOD,
        "params": params,
    }))
    .unwrap();

    let response = router.call(request).await.unwrap();

    assert_eq!(response, serde_json::Value::Null);
}

#[tokio::test(flavor = "current_thread")]
async fn router_handles_call_hierarchy_requests() {
    let mut router = new_router(ClientSocket::new_closed());
    let uri = lsp_types::Url::parse("file:///workspace/src/Test.sol").unwrap();
    let range = Range::new(Position::new(0, 0), Position::new(0, 1));
    let item = CallHierarchyItem {
        name: "f".into(),
        kind: SymbolKind::FUNCTION,
        tags: None,
        detail: None,
        uri: uri.clone(),
        range,
        selection_range: range,
        data: None,
    };
    let requests = [
        serde_json::json!({
            "id": 1,
            "method": request::CallHierarchyPrepare::METHOD,
            "params": CallHierarchyPrepareParams {
                text_document_position_params: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri },
                    position: Position::new(0, 0),
                },
                work_done_progress_params: WorkDoneProgressParams::default(),
            },
        }),
        serde_json::json!({
            "id": 2,
            "method": request::CallHierarchyIncomingCalls::METHOD,
            "params": CallHierarchyIncomingCallsParams {
                item: item.clone(),
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            },
        }),
        serde_json::json!({
            "id": 3,
            "method": request::CallHierarchyOutgoingCalls::METHOD,
            "params": CallHierarchyOutgoingCallsParams {
                item,
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            },
        }),
    ];

    for request in requests {
        let request = serde_json::from_value::<AnyRequest>(request).unwrap();
        let response = router.call(request).await.unwrap();
        assert_eq!(response, serde_json::Value::Null);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn router_handles_signature_help_requests() {
    let mut router = new_router(ClientSocket::new_closed());
    let params = SignatureHelpParams {
        context: None,
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: lsp_types::Url::parse("file:///workspace/src/Test.sol").unwrap(),
            },
            position: Position::new(0, 0),
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
    };
    let request = serde_json::from_value::<AnyRequest>(serde_json::json!({
        "id": 1,
        "method": request::SignatureHelpRequest::METHOD,
        "params": params,
    }))
    .unwrap();

    let response = router.call(request).await.unwrap();

    assert_eq!(response, serde_json::Value::Null);
}

#[tokio::test(flavor = "current_thread")]
async fn router_handles_goto_implementation_requests() {
    let mut router = new_router(ClientSocket::new_closed());
    let params = request::GotoImplementationParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: lsp_types::Url::parse("file:///workspace/src/Test.sol").unwrap(),
            },
            position: Position::new(0, 0),
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: lsp_types::PartialResultParams::default(),
    };
    let request = serde_json::from_value::<AnyRequest>(serde_json::json!({
        "id": 1,
        "method": request::GotoImplementation::METHOD,
        "params": params,
    }))
    .unwrap();

    let response = router.call(request).await.unwrap();

    assert_eq!(response, serde_json::Value::Null);
}
#[tokio::test(flavor = "current_thread")]
async fn router_handles_type_definition_requests() {
    let mut router = new_router(ClientSocket::new_closed());
    let params = TextDocumentPositionParams {
        text_document: TextDocumentIdentifier {
            uri: lsp_types::Url::parse("file:///workspace/src/Test.sol").unwrap(),
        },
        position: Position::new(0, 0),
    };
    let request = serde_json::from_value::<AnyRequest>(serde_json::json!({
        "id": 1,
        "method": request::GotoTypeDefinition::METHOD,
        "params": params,
    }))
    .unwrap();

    let response = router.call(request).await.unwrap();

    assert_eq!(response, serde_json::Value::Null);
}

#[tokio::test(flavor = "current_thread")]
async fn router_handles_document_formatting_requests() {
    let mut router = new_router(ClientSocket::new_closed());
    let params = DocumentFormattingParams {
        text_document: TextDocumentIdentifier {
            uri: lsp_types::Url::parse("file:///missing/Test.sol").unwrap(),
        },
        options: FormattingOptions::default(),
        work_done_progress_params: WorkDoneProgressParams::default(),
    };
    let request = serde_json::from_value::<AnyRequest>(serde_json::json!({
        "id": 1,
        "method": request::Formatting::METHOD,
        "params": params,
    }))
    .unwrap();

    let error = router.call(request).await.unwrap_err();

    assert_eq!(error.code, async_lsp::ErrorCode::REQUEST_FAILED);
    assert!(!error.message.ends_with('.'));
}

#[tokio::test(flavor = "current_thread")]
async fn pending_analysis_requests_do_not_block_completion_or_cancellation() {
    const TIMEOUT: Duration = Duration::from_secs(1);

    let project = TestProject::from_fixture(
        r#"
            //- /Completion.sol open
            ///
            contract C {}
            "#,
    );
    let uri = lsp_types::Url::from_file_path(project.path("/Completion.sol")).unwrap();
    let vfs = project.vfs();
    let mut config = project.config();
    config.enable_completion_snippets();
    let (accepted_tx, mut accepted_rx) = mpsc::unbounded_channel();

    let (server_main, _client) = async_lsp::MainLoop::new_server(move |client| {
        let mut state = GlobalState::new(client);
        state.vfs = Arc::new(RwLock::new(vfs));
        state.config = Arc::new(config);
        state.mark_analysis_pending_for_test();
        let router = ObservedRouter { inner: new_router_with_state(state), accepted: accepted_tx };
        ServiceBuilder::new().layer(request_layer()).service(router)
    });
    let (client_main, mut server) = async_lsp::MainLoop::new_client(|_| Router::new(()));

    let (server_stream, client_stream) = tokio::io::duplex(64 << 10);
    let (server_rx, server_tx) = tokio::io::split(server_stream);
    let (server_rx, server_tx) = (server_rx.compat(), server_tx.compat_write());
    let server_main =
        tokio::spawn(async move { server_main.run_buffered(server_rx, server_tx).await });
    let (client_rx, client_tx) = tokio::io::split(client_stream);
    let (client_rx, client_tx) = (client_rx.compat(), client_tx.compat_write());
    let client_main =
        tokio::spawn(async move { client_main.run_buffered(client_rx, client_tx).await });

    let document_symbols =
        start_request(server.request::<request::DocumentSymbolRequest>(DocumentSymbolParams {
            text_document: TextDocumentIdentifier::new(uri.clone()),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        }));
    assert_eq!(
        tokio::time::timeout(TIMEOUT, accepted_rx.recv()).await.unwrap().unwrap(),
        request::DocumentSymbolRequest::METHOD
    );

    let document_links =
        start_request(server.request::<request::DocumentLinkRequest>(DocumentLinkParams {
            text_document: TextDocumentIdentifier::new(uri.clone()),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        }));
    assert_eq!(
        tokio::time::timeout(TIMEOUT, accepted_rx.recv()).await.unwrap().unwrap(),
        request::DocumentLinkRequest::METHOD
    );

    let completion_params = CompletionParams {
        text_document_position: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier::new(uri),
            position: Position::new(0, 3),
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
        context: None,
    };
    let completion = start_request(server.request::<request::Completion>(completion_params));
    server.notify::<notif::Cancel>(CancelParams { id: NumberOrString::Number(0) }).unwrap();
    server.notify::<notif::Cancel>(CancelParams { id: NumberOrString::Number(1) }).unwrap();

    let response = tokio::time::timeout(TIMEOUT, completion)
        .await
        .expect("completion should not wait for analysis")
        .unwrap();
    let Some(CompletionResponse::Array(items)) = response else {
        panic!("expected completion items, got {response:?}");
    };
    assert!(items.iter().any(|item| item.label == "NatSpec contract documentation"));

    assert_request_cancelled(
        tokio::time::timeout(TIMEOUT, document_symbols)
            .await
            .expect("document symbols should be cancelled"),
    );
    assert_request_cancelled(
        tokio::time::timeout(TIMEOUT, document_links)
            .await
            .expect("document links should be cancelled"),
    );

    server.shutdown(()).await.unwrap();
    server.exit(()).unwrap();
    assert!(server_main.await.unwrap().is_ok());
    assert!(matches!(client_main.await.unwrap(), Err(async_lsp::Error::Eof)));
}

#[test]
fn reindex_progress_honors_client_cancellation() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .unwrap();
    runtime.block_on(async {
        let project = TestProject::from_fixture(
            r#"
                //- /Broken.sol open
                contract Broken {
                    function broken() external { uint value = ; }
                }
                "#,
        );
        let broken_uri = lsp_types::Url::from_file_path(project.path("/Broken.sol")).unwrap();
        let vfs = project.vfs();
        let mut initialize = project.initialize_params();
        initialize.capabilities.window =
            Some(WindowClientCapabilities { work_done_progress: Some(true), ..Default::default() });

        let (server_main, _client) = async_lsp::MainLoop::new_server(move |client| {
            let mut state = GlobalState::new(client);
            state.vfs = Arc::new(RwLock::new(vfs));
            new_router_with_state(state)
        });
        let (events_tx, mut events_rx) = mpsc::unbounded_channel();
        let (client_main, mut server) = async_lsp::MainLoop::new_client(move |_| {
            let mut router = Router::new(events_tx);
            router.request::<request::WorkDoneProgressCreate, _>(|events, params| {
                events.send(AnalysisClientEvent::Create(params)).unwrap();
                async { Ok(()) }
            });
            router.notification::<notif::Progress>(|events, params| {
                events.send(AnalysisClientEvent::Progress(params)).unwrap();
                ControlFlow::Continue(())
            });
            router.notification::<notif::PublishDiagnostics>(|events, params| {
                events.send(AnalysisClientEvent::Diagnostics(params)).unwrap();
                ControlFlow::Continue(())
            });
            router.notification::<notif::LogMessage>(|_, _| ControlFlow::Continue(()));
            router
        });

        let (server_stream, client_stream) = tokio::io::duplex(64 << 10);
        let (server_rx, server_tx) = tokio::io::split(server_stream);
        let server_task =
            tokio::spawn(server_main.run_buffered(server_rx.compat(), server_tx.compat_write()));
        let (client_rx, client_tx) = tokio::io::split(client_stream);
        let client_task =
            tokio::spawn(client_main.run_buffered(client_rx.compat(), client_tx.compat_write()));

        server.initialize(initialize).await.unwrap();
        server.initialized(InitializedParams {}).unwrap();

        // Process and drain the automatic initialization reindex before testing cancellation.
        let _ = server
            .request::<request::WorkspaceSymbolRequest>(WorkspaceSymbolParams {
                query: "initialization barrier".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        while !matches!(
            next_analysis_event(&mut events_rx).await,
            AnalysisClientEvent::Diagnostics(params) if params.uri == broken_uri
        ) {}

        let (blocker_started_tx, blocker_started_rx) = std::sync::mpsc::channel();
        let (release_blocker_tx, release_blocker_rx) = std::sync::mpsc::channel();
        let blocker = tokio::task::spawn_blocking(move || {
            blocker_started_tx.send(()).unwrap();
            release_blocker_rx.recv().unwrap();
        });
        blocker_started_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("blocking worker should be occupied");

        let first_response = tokio::time::timeout(
            Duration::from_secs(1),
            server.request::<request::ExecuteCommand>(ExecuteCommandParams {
                command: commands::REINDEX.into(),
                ..Default::default()
            }),
        )
        .await
        .expect("reindex acknowledgement should not wait for analysis")
        .unwrap();
        assert_eq!(first_response, Some(serde_json::json!({ "success": true })));

        let AnalysisClientEvent::Create(create) = next_analysis_event(&mut events_rx).await else {
            panic!("expected progress creation")
        };
        let token = create.token;
        match next_analysis_event(&mut events_rx).await {
            AnalysisClientEvent::Progress(ProgressParams {
                token: actual,
                value: ProgressParamsValue::WorkDone(WorkDoneProgress::Begin(begin)),
            }) => {
                assert_eq!(actual, token);
                assert_eq!(begin.title, "Indexing workspace");
                assert_eq!(begin.cancellable, Some(false));
            }
            event => panic!("expected progress begin, got {event:?}"),
        }

        let second_response = tokio::time::timeout(
            Duration::from_secs(1),
            server.request::<request::ExecuteCommand>(ExecuteCommandParams {
                command: commands::REINDEX.into(),
                ..Default::default()
            }),
        )
        .await
        .expect("replacement reindex acknowledgement should not wait for analysis")
        .unwrap();
        assert_eq!(second_response, Some(serde_json::json!({ "success": true })));

        match next_analysis_event(&mut events_rx).await {
            AnalysisClientEvent::Progress(ProgressParams {
                token: actual,
                value: ProgressParamsValue::WorkDone(WorkDoneProgress::Report(report)),
            }) => {
                assert_eq!(actual, token);
                assert_eq!(
                    report.message.as_deref(),
                    Some("Workspace changed, restarting analysis")
                );
            }
            event => panic!("expected replacement report, got {event:?}"),
        }

        server
            .notify::<notif::WorkDoneProgressCancel>(WorkDoneProgressCancelParams {
                token: token.clone(),
            })
            .unwrap();
        let _ = server
            .request::<request::WorkspaceSymbolRequest>(WorkspaceSymbolParams {
                query: "cancel barrier".into(),
                ..Default::default()
            })
            .await
            .unwrap();

        match next_analysis_event(&mut events_rx).await {
            AnalysisClientEvent::Progress(ProgressParams {
                token: actual,
                value: ProgressParamsValue::WorkDone(WorkDoneProgress::End(end)),
            }) => {
                assert_eq!(actual, token);
                assert!(end.message.is_none());
            }
            event => panic!("expected cancellation end, got {event:?}"),
        }

        release_blocker_tx.send(()).unwrap();
        blocker.await.unwrap();

        let mut saw_latest_diagnostics = false;
        while !saw_latest_diagnostics {
            match next_analysis_event(&mut events_rx).await {
                AnalysisClientEvent::Create(create) => {
                    panic!("replacement created a second token: {create:?}")
                }
                AnalysisClientEvent::Diagnostics(params) => {
                    if params.uri == broken_uri && !params.diagnostics.is_empty() {
                        saw_latest_diagnostics = true;
                    }
                }
                AnalysisClientEvent::Progress(progress) => {
                    panic!("progress continued after cancellation: {progress:?}")
                }
            }
        }

        server.shutdown(()).await.unwrap();
        server.exit(()).unwrap();
        assert!(server_task.await.unwrap().is_ok());
        assert!(matches!(client_task.await.unwrap(), Err(async_lsp::Error::Eof)));
    });
}

#[tokio::test(flavor = "current_thread")]
async fn initialize_advertises_type_hierarchy_statically() {
    let mut router = new_router(ClientSocket::new_closed());
    let request = serde_json::from_value::<AnyRequest>(serde_json::json!({
        "id": 1,
        "method": request::Initialize::METHOD,
        "params": InitializeParams::default(),
    }))
    .unwrap();

    let response = router.call(request).await.unwrap();

    assert_eq!(response["capabilities"]["typeHierarchyProvider"], true);
    assert_eq!(response["capabilities"]["completionProvider"]["resolveProvider"], true);
    assert_eq!(response["capabilities"]["codeLensProvider"]["resolveProvider"], false);
    assert_eq!(response["capabilities"]["hoverProvider"], true);
    assert_eq!(response["serverInfo"]["name"], "solar");
}

#[tokio::test(flavor = "current_thread")]
async fn initialized_registers_watched_files_when_client_supports_dynamic_registration() {
    let (server_main, _client) = async_lsp::MainLoop::new_server(new_router);
    let (registration_tx, registration_rx) = oneshot::channel();
    let (client_main, mut server) = async_lsp::MainLoop::new_client(|_| {
        let mut router = Router::new(Some(registration_tx));
        router.request::<request::RegisterCapability, _>(|state, params| {
            state.take().unwrap().send(params).unwrap();
            async move { Ok(()) }
        });
        router.notification::<notif::LogMessage>(|_, _| ControlFlow::Continue(()));
        router
    });

    let (server_stream, client_stream) = tokio::io::duplex(64 << 10);
    let (server_rx, server_tx) = tokio::io::split(server_stream);
    let (server_rx, server_tx) = (server_rx.compat(), server_tx.compat_write());
    let server_main =
        tokio::spawn(async move { server_main.run_buffered(server_rx, server_tx).await });
    let (client_rx, client_tx) = tokio::io::split(client_stream);
    let (client_rx, client_tx) = (client_rx.compat(), client_tx.compat_write());
    let client_main =
        tokio::spawn(async move { client_main.run_buffered(client_rx, client_tx).await });

    let mut params = InitializeParams::default();
    params.capabilities.workspace = Some(WorkspaceClientCapabilities {
        did_change_watched_files: Some(DidChangeWatchedFilesClientCapabilities {
            dynamic_registration: Some(true),
            ..Default::default()
        }),
        ..Default::default()
    });
    server.initialize(params).await.unwrap();
    server.initialized(InitializedParams {}).unwrap();

    let registrations = tokio::time::timeout(std::time::Duration::from_secs(1), registration_rx)
        .await
        .unwrap()
        .unwrap();
    let [registration] = registrations.registrations.try_into().unwrap();
    assert_eq!(registration.method, notif::DidChangeWatchedFiles::METHOD);

    server.shutdown(()).await.unwrap();
    server.exit(()).unwrap();
    assert!(server_main.await.unwrap().is_ok());
    assert!(matches!(client_main.await.unwrap(), Err(async_lsp::Error::Eof)));
}

struct ProtocolTraceHarness {
    client: ClientSocket,
    server: async_lsp::ServerSocket,
    traces: mpsc::UnboundedReceiver<LogTraceParams>,
    server_task: tokio::task::JoinHandle<async_lsp::Result<()>>,
    client_task: tokio::task::JoinHandle<async_lsp::Result<()>>,
}

enum SensitiveTraceRequest {}

impl Request for SensitiveTraceRequest {
    type Params = serde_json::Value;
    type Result = serde_json::Value;

    const METHOD: &'static str = "/workspace/Secret.sol";
}

enum SensitiveTraceResultRequest {}

impl Request for SensitiveTraceResultRequest {
    type Params = serde_json::Value;
    type Result = serde_json::Value;

    const METHOD: &'static str = "test/sensitiveResult";
}

enum PendingTraceRequest {}

impl Request for PendingTraceRequest {
    type Params = ();
    type Result = ();

    const METHOD: &'static str = "test/pendingTrace";
}

enum TraceBarrierRequest {}

impl Request for TraceBarrierRequest {
    type Params = ();
    type Result = ();

    const METHOD: &'static str = "test/traceBarrier";
}

enum TraceInitializeRequest {}

impl Request for TraceInitializeRequest {
    type Params = ();
    type Result = ();

    const METHOD: &'static str = request::Initialize::METHOD;
}

struct PendingTraceControl {
    entered: oneshot::Sender<NumberOrString>,
    release: oneshot::Receiver<()>,
}

struct ProtocolTraceTestRouter {
    inner: Router<GlobalState>,
    pending: Option<PendingTraceControl>,
}

impl Service<AnyRequest> for ProtocolTraceTestRouter {
    type Response = serde_json::Value;
    type Error = ResponseError;
    type Future =
        Pin<Box<dyn Future<Output = Result<serde_json::Value, ResponseError>> + Send + 'static>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: AnyRequest) -> Self::Future {
        match &*request.method {
            SensitiveTraceRequest::METHOD => {
                Box::pin(std::future::ready(Err(ResponseError::new_with_data(
                    async_lsp::ErrorCode::REQUEST_FAILED,
                    "error-message-secret",
                    serde_json::json!({ "token": "error-data-secret" }),
                ))))
            }
            SensitiveTraceResultRequest::METHOD => {
                Box::pin(std::future::ready(Ok(serde_json::json!({
                    "uri": "file:///workspace/ResultSecret.sol",
                    "text": "contract ResultSecret {}",
                    "token": "result-token-secret",
                }))))
            }
            PendingTraceRequest::METHOD => {
                let request_id = request.id.clone();
                let PendingTraceControl { entered, release } =
                    self.pending.take().expect("pending trace request should run once");
                Box::pin(async move {
                    entered.send(request_id).expect("pending trace receiver should be open");
                    release.await.expect("pending trace request should be released");
                    Ok(serde_json::Value::Null)
                })
            }
            TraceBarrierRequest::METHOD => {
                Box::pin(std::future::ready(Ok(serde_json::Value::Null)))
            }
            _ => self.inner.call(request),
        }
    }
}

impl LspService for ProtocolTraceTestRouter {
    fn notify(&mut self, notification: AnyNotification) -> ControlFlow<async_lsp::Result<()>> {
        self.inner.notify(notification)
    }

    fn emit(&mut self, event: AnyEvent) -> ControlFlow<async_lsp::Result<()>> {
        self.inner.emit(event)
    }
}

fn new_protocol_trace_test_service(
    client: ClientSocket,
    pending: Option<PendingTraceControl>,
) -> impl LspService<Response = serde_json::Value, Error = ResponseError, Future: Send + 'static> + Send
{
    let state = GlobalState::new(client.clone());
    let protocol_trace = state.protocol_trace();
    let router = ProtocolTraceTestRouter { inner: new_router_with_state(state), pending };
    ServiceBuilder::new()
        .layer(TracingLayer::default())
        .layer(LifecycleLayer::default())
        .layer(crate::protocol_trace::ProtocolTraceLayer::new(protocol_trace))
        .layer(request_layer())
        .layer(ClientProcessMonitorLayer::new(client))
        .service(router)
}

impl ProtocolTraceHarness {
    async fn initialize(&mut self, trace: Option<TraceValue>) {
        let params = InitializeParams { trace, ..Default::default() };
        self.server.initialize(params).await.unwrap();
        self.server.initialized(InitializedParams {}).unwrap();
    }

    fn set_trace(&self, value: TraceValue) {
        self.server.notify::<notif::SetTrace>(SetTraceParams { value }).unwrap();
    }

    async fn probe(&self) {
        self.client.request::<request::Shutdown>(()).await.unwrap();
    }

    fn take_traces(&mut self) -> Vec<LogTraceParams> {
        let mut traces = Vec::new();
        while let Ok(trace) = self.traces.try_recv() {
            traces.push(trace);
        }
        traces
    }

    async fn shutdown(mut self) {
        self.set_trace(TraceValue::Off);
        self.server.shutdown(()).await.unwrap();
        self.server.exit(()).unwrap();
        assert!(self.server_task.await.unwrap().is_ok());
        assert!(matches!(self.client_task.await.unwrap(), Err(async_lsp::Error::Eof)));
    }
}

fn protocol_trace_harness_with<S>(server: impl FnOnce(ClientSocket) -> S) -> ProtocolTraceHarness
where
    S: LspService<Response = serde_json::Value, Error = ResponseError> + Send + 'static,
    S::Future: Send + 'static,
{
    let (server_main, client) = async_lsp::MainLoop::new_server(server);
    let (trace_tx, traces) = mpsc::unbounded_channel::<LogTraceParams>();
    let (client_main, server) = async_lsp::MainLoop::new_client(move |_| {
        let mut router = Router::new(trace_tx);
        router.request::<request::Shutdown, _>(|_, ()| std::future::ready(Ok(())));
        router.notification::<notif::LogTrace>(|traces, params| {
            traces.send(params).unwrap();
            ControlFlow::Continue(())
        });
        router.notification::<notif::LogMessage>(|_, _| ControlFlow::Continue(()));
        router.notification::<notif::PublishDiagnostics>(|_, _| ControlFlow::Continue(()));
        router
    });

    let (server_stream, client_stream) = tokio::io::duplex(64 << 10);
    let (server_rx, server_tx) = tokio::io::split(server_stream);
    let server_task =
        tokio::spawn(server_main.run_buffered(server_rx.compat(), server_tx.compat_write()));
    let (client_rx, client_tx) = tokio::io::split(client_stream);
    let client_task =
        tokio::spawn(client_main.run_buffered(client_rx.compat(), client_tx.compat_write()));

    ProtocolTraceHarness { client, server, traces, server_task, client_task }
}

fn protocol_trace_harness() -> ProtocolTraceHarness {
    protocol_trace_harness_with(new_server_service)
}

fn protocol_trace_test_harness(pending: Option<PendingTraceControl>) -> ProtocolTraceHarness {
    protocol_trace_harness_with(move |client| new_protocol_trace_test_service(client, pending))
}

async fn write_lsp_frame(writer: &mut (impl AsyncWrite + Unpin), message: serde_json::Value) {
    let body = serde_json::to_vec(&message).unwrap();
    writer.write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes()).await.unwrap();
    writer.write_all(&body).await.unwrap();
    writer.flush().await.unwrap();
}

async fn read_lsp_frame(reader: &mut BufReader<impl AsyncRead + Unpin>) -> serde_json::Value {
    tokio::time::timeout(Duration::from_secs(1), async {
        let mut content_length = None;
        loop {
            let mut line = String::new();
            assert_ne!(
                reader.read_line(&mut line).await.unwrap(),
                0,
                "unexpected end of LSP stream"
            );
            if line == "\r\n" {
                break;
            }
            if let Some(value) =
                line.strip_prefix("Content-Length: ").and_then(|line| line.strip_suffix("\r\n"))
            {
                content_length = Some(value.parse::<usize>().unwrap());
            }
        }

        let mut body = vec![0; content_length.expect("LSP frame should have a content length")];
        reader.read_exact(&mut body).await.unwrap();
        serde_json::from_slice(&body).unwrap()
    })
    .await
    .expect("LSP frame should arrive")
}

fn server_processing_time(trace: &LogTraceParams) -> u128 {
    trace
        .verbose
        .as_deref()
        .and_then(|detail| detail.strip_prefix("Server processing took "))
        .and_then(|detail| detail.strip_suffix(" ms"))
        .expect("verbose trace should contain server processing time")
        .parse()
        .expect("server processing time should be numeric")
}

#[tokio::test(flavor = "current_thread")]
async fn completion_trace_precedes_the_response_on_the_wire() {
    let (server_main, _client) = async_lsp::MainLoop::new_server(|client| {
        let trace = crate::protocol_trace::ProtocolTrace::new(client);
        trace.set_level(TraceValue::Messages);
        let mut router = Router::new(());
        router.request::<TraceInitializeRequest, _>(|_, ()| std::future::ready(Ok(())));
        ServiceBuilder::new()
            .layer(crate::protocol_trace::ProtocolTraceLayer::new(trace))
            .service(router)
    });
    let (server_stream, client_stream) = tokio::io::duplex(64 << 10);
    let (server_rx, server_tx) = tokio::io::split(server_stream);
    let server_task =
        tokio::spawn(server_main.run_buffered(server_rx.compat(), server_tx.compat_write()));
    let (client_rx, mut client_tx) = tokio::io::split(client_stream);
    let mut client_rx = BufReader::new(client_rx);

    write_lsp_frame(
        &mut client_tx,
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": TraceInitializeRequest::METHOD,
            "params": null,
        }),
    )
    .await;
    assert_eq!(read_lsp_frame(&mut client_rx).await["id"], 1);

    write_lsp_frame(
        &mut client_tx,
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "test/instant",
            "params": null,
        }),
    )
    .await;
    let messages = [read_lsp_frame(&mut client_rx).await, read_lsp_frame(&mut client_rx).await];

    assert_eq!(messages[0]["method"], notif::LogTrace::METHOD);
    assert_eq!(
        messages[0]["params"]["message"],
        "Server completed request `test/instant` with an error"
    );
    assert_eq!(messages[1]["id"], 2);
    assert_eq!(messages[1]["error"]["code"], async_lsp::ErrorCode::METHOD_NOT_FOUND.0);

    drop(client_tx);
    drop(client_rx);
    assert!(matches!(server_task.await.unwrap(), Err(async_lsp::Error::Eof)));
}

#[tokio::test(flavor = "current_thread")]
async fn set_trace_updates_server_request_detail() {
    let mut harness = protocol_trace_harness();
    harness.initialize(None).await;
    let params = WillSaveTextDocumentParams {
        text_document: TextDocumentIdentifier {
            uri: lsp_types::Url::parse("file:///workspace/Secret.sol").unwrap(),
        },
        reason: TextDocumentSaveReason::MANUAL,
    };

    harness.server.notify::<notif::WillSaveTextDocument>(params.clone()).unwrap();
    harness.set_trace(TraceValue::Messages);
    harness.server.notify::<notif::WillSaveTextDocument>(params.clone()).unwrap();
    harness
        .server
        .request::<request::WorkspaceSymbolRequest>(WorkspaceSymbolParams {
            query: "messages trace".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    harness.set_trace(TraceValue::Verbose);
    harness.server.notify::<notif::WillSaveTextDocument>(params.clone()).unwrap();
    harness
        .server
        .request::<request::WorkspaceSymbolRequest>(WorkspaceSymbolParams {
            query: "verbose trace".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    harness.set_trace(TraceValue::Messages);
    harness.server.notify::<notif::WillSaveTextDocument>(params.clone()).unwrap();
    harness
        .server
        .request::<request::WorkspaceSymbolRequest>(WorkspaceSymbolParams {
            query: "messages trace again".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    harness.set_trace(TraceValue::Off);
    harness.server.notify::<notif::WillSaveTextDocument>(params).unwrap();
    harness
        .server
        .request::<request::WorkspaceSymbolRequest>(WorkspaceSymbolParams {
            query: "trace off".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    harness.probe().await;

    let traces = harness.take_traces();
    let [messages, verbose, messages_again] = traces.as_slice() else {
        panic!("expected three completion traces, got {traces:?}");
    };
    for trace in &traces {
        assert_eq!(trace.message, "Server completed request `workspace/symbol` successfully");
    }
    assert!(messages.verbose.is_none());
    let _ = server_processing_time(verbose);
    assert!(messages_again.verbose.is_none());
    harness.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn set_trace_before_initialize_does_not_emit_server_traces() {
    let mut harness = protocol_trace_test_harness(None);
    harness.set_trace(TraceValue::Messages);
    let error = harness.server.request::<TraceBarrierRequest>(()).await.unwrap_err();
    let async_lsp::Error::Response(error) = error else {
        panic!("expected a server-not-initialized response, got {error:?}");
    };
    assert_eq!(error.code, async_lsp::ErrorCode::SERVER_NOT_INITIALIZED);

    harness.initialize(None).await;
    harness.probe().await;

    assert!(harness.take_traces().is_empty());
    harness.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn messages_trace_reports_server_completion_without_payloads() {
    let mut harness = protocol_trace_harness();
    harness.initialize(None).await;
    harness.set_trace(TraceValue::Messages);

    harness
        .server
        .request::<request::WorkspaceSymbolRequest>(WorkspaceSymbolParams {
            query: "workspace-query-secret".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    harness.probe().await;

    assert_eq!(
        harness.take_traces(),
        [LogTraceParams {
            message: "Server completed request `workspace/symbol` successfully".into(),
            verbose: None,
        }]
    );
    harness.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn trace_level_changes_do_not_emit_orphan_request_completions() {
    const TIMEOUT: Duration = Duration::from_secs(1);

    let (entered, request_entered) = oneshot::channel();
    let (release_request, release) = oneshot::channel();
    let mut harness = protocol_trace_test_harness(Some(PendingTraceControl { entered, release }));
    harness.initialize(None).await;
    harness.set_trace(TraceValue::Messages);
    let server = harness.server.clone();
    let request = start_request(server.request::<PendingTraceRequest>(()));
    tokio::time::timeout(TIMEOUT, request_entered)
        .await
        .expect("pending request should start")
        .expect("pending request should signal entry");

    harness.set_trace(TraceValue::Off);
    harness.server.request::<TraceBarrierRequest>(()).await.unwrap();
    release_request.send(()).expect("pending request should still be running");
    tokio::time::timeout(TIMEOUT, request)
        .await
        .expect("pending request should finish")
        .expect("pending request should succeed");
    harness.probe().await;

    assert_eq!(harness.take_traces(), []);
    harness.shutdown().await;

    let (entered, request_entered) = oneshot::channel();
    let (release_request, release) = oneshot::channel();
    let mut harness = protocol_trace_test_harness(Some(PendingTraceControl { entered, release }));
    harness.initialize(None).await;
    let server = harness.server.clone();
    let request = start_request(server.request::<PendingTraceRequest>(()));
    tokio::time::timeout(TIMEOUT, request_entered)
        .await
        .expect("pending request should start")
        .expect("pending request should signal entry");

    harness.set_trace(TraceValue::Messages);
    harness.server.request::<TraceBarrierRequest>(()).await.unwrap();
    harness.probe().await;
    assert_eq!(
        harness.take_traces(),
        [LogTraceParams {
            message: "Server completed request `test/traceBarrier` successfully".into(),
            verbose: None,
        }]
    );

    release_request.send(()).expect("pending request should still be running");
    tokio::time::timeout(TIMEOUT, request)
        .await
        .expect("pending request should finish")
        .expect("pending request should succeed");
    harness.probe().await;
    assert!(harness.take_traces().is_empty());
    harness.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn messages_trace_reports_cancelled_requests_as_errors_without_ids() {
    const TIMEOUT: Duration = Duration::from_secs(1);

    let (entered, request_entered) = oneshot::channel();
    let (_release_request, release) = oneshot::channel();
    let mut harness = protocol_trace_test_harness(Some(PendingTraceControl { entered, release }));
    harness.initialize(None).await;
    harness.set_trace(TraceValue::Messages);
    let server = harness.server.clone();
    let request = start_request(server.request::<PendingTraceRequest>(()));
    let request_id = tokio::time::timeout(TIMEOUT, request_entered)
        .await
        .expect("pending request should start")
        .expect("pending request should signal entry");

    harness.server.notify::<notif::Cancel>(CancelParams { id: request_id }).unwrap();
    assert_request_cancelled(
        tokio::time::timeout(TIMEOUT, request).await.expect("pending request should be cancelled"),
    );
    harness.probe().await;

    assert_eq!(
        harness.take_traces(),
        [LogTraceParams {
            message: "Server completed request `test/pendingTrace` with an error".into(),
            verbose: None,
        }]
    );
    harness.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn initialize_trace_level_applies_after_the_initialize_response() {
    for (level, verbose) in [(TraceValue::Messages, false), (TraceValue::Verbose, true)] {
        let mut harness = protocol_trace_harness();
        harness.initialize(Some(level)).await;
        harness
            .server
            .request::<request::WorkspaceSymbolRequest>(WorkspaceSymbolParams {
                query: "initialized trace".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        harness.probe().await;

        let traces = harness.take_traces();
        let [completed] = traces.as_slice() else {
            panic!("expected one completion trace, got {traces:?}");
        };
        assert_eq!(completed.message, "Server completed request `workspace/symbol` successfully");
        if verbose {
            let _ = server_processing_time(completed);
        } else {
            assert!(completed.verbose.is_none());
        }
        harness.shutdown().await;
    }
}

#[tokio::test(flavor = "current_thread")]
async fn verbose_request_trace_reports_timing_and_redacts_sensitive_data() {
    const SOURCE_SECRET: &str = "contract TraceSecret {}";
    const ENV_SECRET: &str = "environment-value-secret";
    const QUERY_SECRET: &str = "workspace-query-secret";
    const ERROR_MESSAGE_SECRET: &str = "error-message-secret";
    const ERROR_DATA_SECRET: &str = "error-data-secret";

    let mut harness = protocol_trace_test_harness(None);
    harness.initialize(None).await;
    harness.set_trace(TraceValue::Verbose);

    let error = harness
        .server
        .request::<SensitiveTraceRequest>(serde_json::json!({
            "uri": "file:///workspace/Secret.sol",
            "text": SOURCE_SECRET,
            "environment": { "API_TOKEN": ENV_SECRET },
            "query": QUERY_SECRET,
        }))
        .await
        .unwrap_err();
    let async_lsp::Error::Response(error) = error else {
        panic!("expected a request-failed response, got {error:?}");
    };
    assert_eq!(error.code, async_lsp::ErrorCode::REQUEST_FAILED);
    assert_eq!(error.message, ERROR_MESSAGE_SECRET);
    assert_eq!(error.data, Some(serde_json::json!({ "token": ERROR_DATA_SECRET })));
    harness.probe().await;

    let traces = harness.take_traces();
    let [completed] = traces.as_slice() else {
        panic!("expected one completion trace, got {traces:?}");
    };
    assert_eq!(completed.message, "Server completed request `<redacted method>` with an error");
    let _ = server_processing_time(completed);

    let trace_json = serde_json::to_string(&traces).unwrap();
    for secret in [
        SensitiveTraceRequest::METHOD,
        "file:///workspace/Secret.sol",
        SOURCE_SECRET,
        ENV_SECRET,
        QUERY_SECRET,
        ERROR_MESSAGE_SECRET,
        ERROR_DATA_SECRET,
    ] {
        assert!(!trace_json.contains(secret), "protocol trace leaked `{secret}`");
    }
    harness.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn verbose_request_trace_reports_server_timing_without_payloads() {
    const PARAM_SECRET: &str = "request-parameter-secret";
    const RESULT_URI_SECRET: &str = "file:///workspace/ResultSecret.sol";
    const RESULT_SOURCE_SECRET: &str = "contract ResultSecret {}";
    const RESULT_TOKEN_SECRET: &str = "result-token-secret";

    let mut harness = protocol_trace_test_harness(None);
    harness.initialize(None).await;
    harness.set_trace(TraceValue::Verbose);

    let result = harness
        .server
        .request::<SensitiveTraceResultRequest>(serde_json::json!({ "query": PARAM_SECRET }))
        .await
        .unwrap();
    assert_eq!(
        result,
        serde_json::json!({
            "uri": RESULT_URI_SECRET,
            "text": RESULT_SOURCE_SECRET,
            "token": RESULT_TOKEN_SECRET,
        })
    );
    harness.probe().await;

    let traces = harness.take_traces();
    let [completed] = traces.as_slice() else {
        panic!("expected one completion trace, got {traces:?}");
    };
    assert_eq!(completed.message, "Server completed request `test/sensitiveResult` successfully");
    let _ = server_processing_time(completed);
    let trace_json = serde_json::to_string(&traces).unwrap();
    for secret in [PARAM_SECRET, RESULT_URI_SECRET, RESULT_SOURCE_SECRET, RESULT_TOKEN_SECRET] {
        assert!(!trace_json.contains(secret), "protocol trace leaked `{secret}`");
    }
    harness.shutdown().await;
}
