use super::*;
use crate::test_support::{TestProject, assert_request_cancelled, start_request};
use async_lsp::{
    AnyEvent, AnyNotification, AnyRequest, LanguageServer, LspService, ResponseError,
    router::Router,
};
use lsp_types::{
    CallHierarchyIncomingCallsParams, CallHierarchyItem, CallHierarchyOutgoingCallsParams,
    CallHierarchyPrepareParams, CancelParams, CodeActionContext, CodeActionParams,
    CompletionParams, CompletionResponse, DidChangeWatchedFilesClientCapabilities,
    DidChangeWatchedFilesParams, DidSaveTextDocumentParams, DocumentFormattingParams,
    DocumentHighlightParams, DocumentLinkParams, DocumentSymbolParams, ExecuteCommandParams,
    FileChangeType, FileEvent, FoldingRangeParams, FormattingOptions, HoverParams,
    InitializeParams, InitializedParams, NumberOrString, PartialResultParams, Position,
    ProgressParams, ProgressParamsValue, PublishDiagnosticsParams, Range, SelectionRangeParams,
    SignatureHelpParams, SymbolKind, TextDocumentIdentifier, TextDocumentPositionParams,
    TextDocumentSaveReason, WillSaveTextDocumentParams, WindowClientCapabilities, WorkDoneProgress,
    WorkDoneProgressCancelParams, WorkDoneProgressCreateParams, WorkDoneProgressParams,
    WorkspaceClientCapabilities, WorkspaceSymbolParams, notification as notif,
    notification::Notification, request, request::Request,
};
use solar_interface::data_structures::sync::RwLock;
use std::{
    ops::ControlFlow,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};
use tokio::sync::{mpsc, oneshot};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use tower::Service;

fn new_router(client: ClientSocket) -> Router<GlobalState> {
    new_router_with_state(GlobalState::new(client))
}

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
async fn router_handles_code_action_requests() {
    let mut router = new_router(ClientSocket::new_closed());
    let params = CodeActionParams {
        text_document: TextDocumentIdentifier {
            uri: lsp_types::Url::parse("file:///workspace/src/Test.sol").unwrap(),
        },
        range: Range::default(),
        context: CodeActionContext { diagnostics: Vec::new(), ..Default::default() },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };
    let request = serde_json::from_value::<AnyRequest>(serde_json::json!({
        "id": 1,
        "method": request::CodeActionRequest::METHOD,
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
async fn router_handles_workspace_diagnostic_requests() {
    let mut router = new_router(ClientSocket::new_closed());
    let request = serde_json::from_value::<AnyRequest>(serde_json::json!({
        "id": 1,
        "method": request::WorkspaceDiagnosticRequest::METHOD,
        "params": {
            "previousResultIds": [],
        },
    }))
    .unwrap();

    let response = router.call(request).await.unwrap();

    assert_eq!(response, serde_json::json!({ "items": [] }));
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
        let request_client = client.clone();
        let mut state = GlobalState::new(client);
        state.vfs = Arc::new(RwLock::new(vfs));
        state.config = Arc::new(config);
        state.mark_analysis_pending_for_test();
        let router = ObservedRouter { inner: new_router_with_state(state), accepted: accepted_tx };
        ServiceBuilder::new().layer(request_layer(request_client)).service(router)
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
