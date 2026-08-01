use super::*;
use async_lsp::LspService;
use lsp_types::{
    NumberOrString, PreviousResultId, WorkspaceDiagnosticParams,
    WorkspaceDiagnosticReportPartialResult, WorkspaceDiagnosticReportResult,
    WorkspaceDocumentDiagnosticReport,
    request::{Request, WorkspaceDiagnosticRequest},
};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tower::{Layer, Service, ServiceBuilder};

#[derive(Debug)]
enum RawProgress {}

impl Notification for RawProgress {
    type Params = RawProgressParams;
    const METHOD: &'static str = notification::Progress::METHOD;
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct RawProgressParams {
    token: NumberOrString,
    value: serde_json::Value,
}

struct WorkspaceDiagnosticHarness {
    client: ClientSocket,
    server: ServerSocket,
    progress: mpsc::UnboundedReceiver<RawProgressParams>,
    server_task: tokio::task::JoinHandle<async_lsp::Result<()>>,
    client_task: tokio::task::JoinHandle<async_lsp::Result<()>>,
}

impl WorkspaceDiagnosticHarness {
    async fn next_progress(&mut self) -> RawProgressParams {
        tokio::time::timeout(ASYNC_TEST_TIMEOUT, self.progress.recv())
            .await
            .expect("workspace diagnostic progress should arrive")
            .expect("workspace diagnostic progress channel should stay open")
    }

    async fn shutdown(self) {
        self.server.notify::<notification::Exit>(()).unwrap();
        assert!(self.server_task.await.unwrap().is_ok());
        assert!(matches!(self.client_task.await.unwrap(), Err(async_lsp::Error::Eof)));
    }
}

fn workspace_diagnostic_harness() -> WorkspaceDiagnosticHarness {
    let (server_main, client) = async_lsp::MainLoop::new_server(|_| {
        let mut router = Router::new(());
        router.notification::<notification::Exit>(|_, ()| ControlFlow::Break(Ok(())));
        router
    });
    let (progress_tx, progress) = mpsc::unbounded_channel();
    let (client_main, server) = async_lsp::MainLoop::new_client(move |_| {
        let mut router = Router::new(progress_tx);
        router.notification::<RawProgress>(|progress, params| {
            progress.send(params).unwrap();
            ControlFlow::Continue(())
        });
        router
    });
    let (server_stream, client_stream) = tokio::io::duplex(64 << 10);
    let (server_rx, server_tx) = tokio::io::split(server_stream);
    let server_task =
        tokio::spawn(server_main.run_buffered(server_rx.compat(), server_tx.compat_write()));
    let (client_rx, client_tx) = tokio::io::split(client_stream);
    let client_task =
        tokio::spawn(client_main.run_buffered(client_rx.compat(), client_tx.compat_write()));

    WorkspaceDiagnosticHarness { client, server, progress, server_task, client_task }
}

fn workspace_diagnostic_params() -> WorkspaceDiagnosticParams {
    WorkspaceDiagnosticParams {
        identifier: None,
        previous_result_ids: Vec::new(),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    }
}

async fn write_wire_message(
    writer: &mut tokio::io::WriteHalf<tokio::io::DuplexStream>,
    value: serde_json::Value,
) {
    let body = serde_json::to_vec(&value).unwrap();
    writer.write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes()).await.unwrap();
    writer.write_all(&body).await.unwrap();
    writer.flush().await.unwrap();
}

async fn read_wire_message(
    reader: &mut BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>,
) -> serde_json::Value {
    let mut line = String::new();
    let mut content_length = None;
    loop {
        line.clear();
        reader.read_line(&mut line).await.unwrap();
        if line == "\r\n" {
            break;
        }
        if let Some(value) = line.strip_prefix("Content-Length: ") {
            content_length = Some(value.trim().parse::<usize>().unwrap());
        }
    }
    let mut body = vec![0; content_length.expect("wire message content length")];
    reader.read_exact(&mut body).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn workspace_diagnostic_progress_precedes_the_final_response_on_the_wire() {
    let uri = Url::from_file_path(std::env::temp_dir().join("workspace-wire-order.sol")).unwrap();
    let (server_main, _client) = async_lsp::MainLoop::new_server(move |client| {
        let request_client = client.clone();
        let state = GlobalState::new(client);
        assert!(state.snapshot().publish_analysis(
            0,
            AnalysisResult {
                analyzed_documents: AnalyzedDocuments::from_iter([(uri, None)]),
                diagnostics: DiagnosticMap::default(),
                symbol_tables: SymbolTables::default(),
            },
        ));
        ServiceBuilder::new()
            .layer(crate::request_layer(request_client))
            .service(crate::new_router_with_state(state))
    });
    let (server_stream, client_stream) = tokio::io::duplex(64 << 10);
    let (server_reader, server_writer) = tokio::io::split(server_stream);
    let server_task = tokio::spawn(
        server_main.run_buffered(server_reader.compat(), server_writer.compat_write()),
    );
    let (client_reader, mut client_writer) = tokio::io::split(client_stream);
    let mut client_reader = BufReader::new(client_reader);
    write_wire_message(
        &mut client_writer,
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": WorkspaceDiagnosticRequest::METHOD,
            "params": {
                "previousResultIds": [],
                "partialResultToken": "workspace-wire-partial",
                "workDoneToken": "workspace-wire-work-done",
            },
        }),
    )
    .await;

    let mut messages = Vec::new();
    for _ in 0..4 {
        messages.push(
            tokio::time::timeout(ASYNC_TEST_TIMEOUT, read_wire_message(&mut client_reader))
                .await
                .expect("workspace diagnostic wire message"),
        );
    }

    assert_eq!(messages[0]["method"], notification::Progress::METHOD);
    assert_eq!(messages[0]["params"]["value"]["kind"], "begin");
    assert_eq!(messages[1]["method"], notification::Progress::METHOD);
    assert_eq!(messages[1]["params"]["token"], "workspace-wire-partial");
    assert_eq!(messages[1]["params"]["value"]["items"].as_array().unwrap().len(), 1);
    assert_eq!(messages[2]["method"], notification::Progress::METHOD);
    assert_eq!(messages[2]["params"]["value"]["kind"], "end");
    assert_eq!(messages[3]["id"], 1);
    assert!(messages[3]["result"]["items"].as_array().unwrap().is_empty());

    write_wire_message(
        &mut client_writer,
        serde_json::json!({
            "jsonrpc": "2.0",
            "method": notification::Exit::METHOD,
            "params": null,
        }),
    )
    .await;
    assert!(server_task.await.unwrap().is_ok());
}

#[tokio::test(flavor = "current_thread")]
async fn cancelling_workspace_diagnostics_sends_end_before_the_error_response_on_the_wire() {
    let (server_main, _client) = async_lsp::MainLoop::new_server(|client| {
        let request_client = client.clone();
        let state = GlobalState::new(client);
        state.mark_analysis_pending_for_test();
        ServiceBuilder::new()
            .layer(crate::request_layer(request_client))
            .service(crate::new_router_with_state(state))
    });
    let (server_stream, client_stream) = tokio::io::duplex(64 << 10);
    let (server_reader, server_writer) = tokio::io::split(server_stream);
    let server_task = tokio::spawn(
        server_main.run_buffered(server_reader.compat(), server_writer.compat_write()),
    );
    let (client_reader, mut client_writer) = tokio::io::split(client_stream);
    let mut client_reader = BufReader::new(client_reader);
    write_wire_message(
        &mut client_writer,
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": "workspace-wire-cancel",
            "method": WorkspaceDiagnosticRequest::METHOD,
            "params": {
                "previousResultIds": [],
                "workDoneToken": "workspace-wire-cancel-progress",
            },
        }),
    )
    .await;

    let mut messages = vec![
        tokio::time::timeout(ASYNC_TEST_TIMEOUT, read_wire_message(&mut client_reader))
            .await
            .expect("workspace diagnostic begin progress"),
    ];
    write_wire_message(
        &mut client_writer,
        serde_json::json!({
            "jsonrpc": "2.0",
            "method": notification::Cancel::METHOD,
            "params": { "id": "workspace-wire-cancel" },
        }),
    )
    .await;
    for _ in 0..2 {
        messages.push(
            tokio::time::timeout(ASYNC_TEST_TIMEOUT, read_wire_message(&mut client_reader))
                .await
                .expect("workspace diagnostic cancellation wire message"),
        );
    }

    assert_eq!(messages[0]["method"], notification::Progress::METHOD);
    assert_eq!(messages[0]["params"]["token"], "workspace-wire-cancel-progress");
    assert_eq!(messages[0]["params"]["value"]["kind"], "begin");
    assert_eq!(messages[1]["method"], notification::Progress::METHOD);
    assert_eq!(messages[1]["params"]["token"], "workspace-wire-cancel-progress");
    assert_eq!(messages[1]["params"]["value"]["kind"], "end");
    assert_eq!(messages[2]["id"], "workspace-wire-cancel");
    assert_eq!(messages[2]["error"]["code"], ErrorCode::REQUEST_CANCELLED.0);
    assert_eq!(messages[2]["error"]["message"], "Client cancelled the request");

    write_wire_message(
        &mut client_writer,
        serde_json::json!({
            "jsonrpc": "2.0",
            "method": notification::Exit::METHOD,
            "params": null,
        }),
    )
    .await;
    assert!(server_task.await.unwrap().is_ok());
}

#[tokio::test(flavor = "current_thread")]
async fn workspace_diagnostics_stream_at_most_64_reports_per_partial_result() {
    let mut harness = workspace_diagnostic_harness();
    let mut state = GlobalState::new(harness.client.clone());
    let expected_uris = (0..129)
        .map(|index| {
            Url::from_file_path(
                std::env::temp_dir().join(format!("workspace-diagnostic-{index:03}.sol")),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let analyzed_documents = expected_uris.iter().cloned().map(|uri| (uri, None)).collect();
    assert!(state.snapshot().publish_analysis(
        0,
        AnalysisResult {
            analyzed_documents,
            diagnostics: DiagnosticMap::default(),
            symbol_tables: SymbolTables::default(),
        },
    ));
    let partial_result_token = NumberOrString::String("workspace-partial".into());
    let mut params = workspace_diagnostic_params();
    params.partial_result_params.partial_result_token = Some(partial_result_token.clone());

    let response = crate::handlers::workspace_diagnostic(&mut state, params).await.unwrap();

    let WorkspaceDiagnosticReportResult::Report(response) = response else {
        panic!("workspace diagnostic should return a complete response");
    };
    assert!(response.items.is_empty());
    let mut batch_sizes = Vec::new();
    let mut actual_uris = Vec::new();
    for _ in 0..3 {
        let progress = harness.next_progress().await;
        assert_eq!(progress.token, partial_result_token);
        let partial =
            serde_json::from_value::<WorkspaceDiagnosticReportPartialResult>(progress.value)
                .unwrap();
        batch_sizes.push(partial.items.len());
        actual_uris.extend(partial.items.into_iter().map(|report| match report {
            WorkspaceDocumentDiagnosticReport::Full(report) => {
                assert_eq!(report.version, None);
                assert!(report.full_document_diagnostic_report.items.is_empty());
                report.uri
            }
            WorkspaceDocumentDiagnosticReport::Unchanged(_) => {
                panic!("first workspace pull should contain full reports")
            }
        }));
    }
    assert_eq!(batch_sizes, [64, 64, 1]);
    assert_eq!(actual_uris, expected_uris);

    harness.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn workspace_diagnostics_can_be_cancelled_between_partial_batches() {
    let mut harness = workspace_diagnostic_harness();
    let state = GlobalState::new(harness.client.clone());
    let analyzed_documents = (0..129)
        .map(|index| {
            let uri = Url::from_file_path(
                std::env::temp_dir().join(format!("workspace-cancel-{index:03}.sol")),
            )
            .unwrap();
            (uri, None)
        })
        .collect();
    assert!(state.snapshot().publish_analysis(
        0,
        AnalysisResult {
            analyzed_documents,
            diagnostics: DiagnosticMap::default(),
            symbol_tables: SymbolTables::default(),
        },
    ));
    let router = crate::new_router_with_state(state);
    let mut service = crate::request_layer(ClientSocket::new_closed()).layer(router);
    std::future::poll_fn(|context| service.poll_ready(context)).await.unwrap();
    let request = serde_json::from_value(serde_json::json!({
        "id": "workspace-mid-stream-cancel",
        "method": WorkspaceDiagnosticRequest::METHOD,
        "params": {
            "previousResultIds": [],
            "partialResultToken": "workspace-cancel-partial",
        },
    }))
    .unwrap();
    let mut response = std::pin::pin!(service.call(request));
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);

    assert!(response.as_mut().poll(&mut context).is_pending());
    let progress = harness.next_progress().await;
    assert_eq!(progress.token, NumberOrString::String("workspace-cancel-partial".into()));
    let partial =
        serde_json::from_value::<WorkspaceDiagnosticReportPartialResult>(progress.value).unwrap();
    assert_eq!(partial.items.len(), 64);

    let cancel = serde_json::from_value(serde_json::json!({
        "method": notification::Cancel::METHOD,
        "params": { "id": "workspace-mid-stream-cancel" },
    }))
    .unwrap();
    assert!(service.notify(cancel).is_continue());

    let error = response.await.unwrap_err();
    assert_eq!(error.code, ErrorCode::REQUEST_CANCELLED);
    assert!(matches!(
        harness.progress.try_recv(),
        Err(tokio::sync::mpsc::error::TryRecvError::Empty)
    ));

    harness.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn workspace_diagnostics_report_work_done_on_the_request_token() {
    let mut harness = workspace_diagnostic_harness();
    let mut state = GlobalState::new(harness.client.clone());
    let work_done_token = NumberOrString::String("workspace-work-done".into());
    let mut params = workspace_diagnostic_params();
    params.work_done_progress_params.work_done_token = Some(work_done_token.clone());

    crate::handlers::workspace_diagnostic(&mut state, params).await.unwrap();

    let begin = harness.next_progress().await;
    assert_eq!(begin.token, work_done_token);
    let begin = serde_json::from_value::<WorkDoneProgress>(begin.value).unwrap();
    assert!(matches!(
        begin,
        WorkDoneProgress::Begin(begin)
            if begin.title == "Workspace diagnostics" && begin.cancellable == Some(false)
    ));
    let end = harness.next_progress().await;
    assert_eq!(end.token, work_done_token);
    assert!(matches!(
        serde_json::from_value::<WorkDoneProgress>(end.value).unwrap(),
        WorkDoneProgress::End(_)
    ));

    harness.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn cancelling_workspace_diagnostics_ends_request_progress_without_starting_analysis() {
    let mut harness = workspace_diagnostic_harness();
    let state = GlobalState::new(harness.client.clone());
    state.mark_analysis_pending_for_test();
    let requested_version = state.analysis_version.load(Ordering::Acquire);
    let analysis_version = state.analysis_version.clone();
    let scheduler = state.analysis_scheduler.clone();
    let router = crate::new_router_with_state(state);
    let mut service = crate::request_layer(ClientSocket::new_closed()).layer(router);
    std::future::poll_fn(|context| service.poll_ready(context)).await.unwrap();
    let request = serde_json::from_value(serde_json::json!({
        "id": "workspace-cancel",
        "method": WorkspaceDiagnosticRequest::METHOD,
        "params": {
            "previousResultIds": [],
            "workDoneToken": "workspace-cancel-progress",
        },
    }))
    .unwrap();
    let mut response = std::pin::pin!(service.call(request));
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);

    assert!(response.as_mut().poll(&mut context).is_pending());
    let begin = harness.next_progress().await;
    assert_eq!(begin.token, NumberOrString::String("workspace-cancel-progress".into()));
    assert!(matches!(
        serde_json::from_value::<WorkDoneProgress>(begin.value).unwrap(),
        WorkDoneProgress::Begin(_)
    ));
    let cancel = serde_json::from_value(serde_json::json!({
        "method": notification::Cancel::METHOD,
        "params": { "id": "workspace-cancel" },
    }))
    .unwrap();
    assert!(service.notify(cancel).is_continue());

    let error = response.await.unwrap_err();
    assert_eq!(error.code, ErrorCode::REQUEST_CANCELLED);
    let end = harness.next_progress().await;
    assert_eq!(end.token, NumberOrString::String("workspace-cancel-progress".into()));
    assert!(matches!(
        serde_json::from_value::<WorkDoneProgress>(end.value).unwrap(),
        WorkDoneProgress::End(_)
    ));
    assert_eq!(analysis_version.load(Ordering::Acquire), requested_version);
    {
        let tasks = scheduler.tasks.lock();
        assert!(tasks.coordinator.is_none());
        assert!(tasks.worker.is_none());
    }

    harness.shutdown().await;
}

#[test]
fn workspace_diagnostics_round_trip_previous_result_ids_and_clear_stale_reports_once() {
    let current_uri =
        Url::from_file_path(std::env::temp_dir().join("A-Current-Diagnostic.sol")).unwrap();
    let stale_uri =
        Url::from_file_path(std::env::temp_dir().join("B-Stale-Diagnostic.sol")).unwrap();
    let mut state = GlobalState::new(ClientSocket::new_closed());
    assert!(state.snapshot().publish_analysis(
        0,
        AnalysisResult {
            analyzed_documents: AnalyzedDocuments::from_iter([
                (current_uri.clone(), Some(9)),
                (stale_uri.clone(), None),
            ]),
            diagnostics: DiagnosticMap::from_iter([
                (stale_uri.clone(), vec![diagnostic("stale")],)
            ]),
            symbol_tables: SymbolTables::default(),
        },
    ));

    let initial = expect_ready(crate::handlers::workspace_diagnostic(
        &mut state,
        workspace_diagnostic_params(),
    ))
    .unwrap();
    let WorkspaceDiagnosticReportResult::Report(initial) = initial else {
        panic!("workspace diagnostic should return a complete response");
    };
    let [
        WorkspaceDocumentDiagnosticReport::Full(current),
        WorkspaceDocumentDiagnosticReport::Full(stale),
    ] = initial.items.as_slice()
    else {
        panic!("initial workspace diagnostic should contain two full reports");
    };
    assert_eq!(current.uri, current_uri);
    assert_eq!(current.version, Some(9));
    assert!(current.full_document_diagnostic_report.items.is_empty());
    let current_result_id = current.full_document_diagnostic_report.result_id.clone().unwrap();
    assert_eq!(stale.uri, stale_uri);
    assert_eq!(stale.version, None);
    assert_eq!(stale.full_document_diagnostic_report.items, vec![diagnostic("stale")]);
    let stale_result_id = stale.full_document_diagnostic_report.result_id.clone().unwrap();

    let mut params = workspace_diagnostic_params();
    params.previous_result_ids = vec![
        PreviousResultId { uri: current_uri.clone(), value: current_result_id.clone() },
        PreviousResultId { uri: stale_uri.clone(), value: stale_result_id.clone() },
    ];
    let unchanged =
        expect_ready(crate::handlers::workspace_diagnostic(&mut state, params)).unwrap();
    let WorkspaceDiagnosticReportResult::Report(unchanged) = unchanged else {
        panic!("workspace diagnostic should return a complete response");
    };
    let [
        WorkspaceDocumentDiagnosticReport::Unchanged(current),
        WorkspaceDocumentDiagnosticReport::Unchanged(stale),
    ] = unchanged.items.as_slice()
    else {
        panic!("known result IDs should produce two unchanged reports");
    };
    assert_eq!(current.uri, current_uri);
    assert_eq!(current.version, Some(9));
    assert_eq!(current.unchanged_document_diagnostic_report.result_id, current_result_id);
    assert_eq!(stale.uri, stale_uri);
    assert_eq!(stale.version, None);
    assert_eq!(stale.unchanged_document_diagnostic_report.result_id, stale_result_id);

    assert!(state.snapshot().publish_analysis(
        0,
        AnalysisResult {
            analyzed_documents: AnalyzedDocuments::from_iter([(current_uri.clone(), Some(9),)]),
            diagnostics: DiagnosticMap::default(),
            symbol_tables: SymbolTables::default(),
        },
    ));
    let mut params = workspace_diagnostic_params();
    params.previous_result_ids = vec![
        PreviousResultId { uri: current_uri.clone(), value: current_result_id.clone() },
        PreviousResultId { uri: stale_uri.clone(), value: stale_result_id },
    ];
    let cleared = expect_ready(crate::handlers::workspace_diagnostic(&mut state, params)).unwrap();
    let WorkspaceDiagnosticReportResult::Report(cleared) = cleared else {
        panic!("workspace diagnostic should return a complete response");
    };
    let [
        WorkspaceDocumentDiagnosticReport::Unchanged(current),
        WorkspaceDocumentDiagnosticReport::Full(stale),
    ] = cleared.items.as_slice()
    else {
        panic!("removed diagnostics should be cleared by one full report");
    };
    assert_eq!(current.uri, current_uri);
    assert_eq!(current.version, Some(9));
    assert_eq!(current.unchanged_document_diagnostic_report.result_id, current_result_id);
    assert_eq!(stale.uri, stale_uri);
    assert_eq!(stale.version, None);
    assert!(stale.full_document_diagnostic_report.items.is_empty());
    let empty_result_id = stale.full_document_diagnostic_report.result_id.clone().unwrap();

    let mut params = workspace_diagnostic_params();
    params.previous_result_ids = vec![
        PreviousResultId { uri: current_uri.clone(), value: current_result_id.clone() },
        PreviousResultId { uri: stale_uri, value: empty_result_id },
    ];
    let cleared = expect_ready(crate::handlers::workspace_diagnostic(&mut state, params)).unwrap();
    let WorkspaceDiagnosticReportResult::Report(cleared) = cleared else {
        panic!("workspace diagnostic should return a complete response");
    };
    let [WorkspaceDocumentDiagnosticReport::Unchanged(current)] = cleared.items.as_slice() else {
        panic!("the stale report should disappear after its empty result is acknowledged");
    };
    assert_eq!(current.uri, current_uri);
    assert_eq!(current.version, Some(9));
    assert_eq!(current.unchanged_document_diagnostic_report.result_id, current_result_id);
}

#[test]
fn unchanged_document_changes_update_workspace_report_versions_without_analysis() {
    let project = TestProject::from_fixture(
        r#"
        //- /Unchanged.sol open
        contract Unchanged {}
        "#,
    );
    let path = project.path("/Unchanged.sol");
    let uri = Url::from_file_path(&path).unwrap();
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.vfs = Arc::new(RwLock::new(project.vfs()));
    assert!(state.snapshot().publish_analysis(
        0,
        AnalysisResult {
            analyzed_documents: AnalyzedDocuments::from_iter([(uri.clone(), Some(0))]),
            diagnostics: DiagnosticMap::default(),
            symbol_tables: SymbolTables::default(),
        },
    ));
    let analysis_version = state.analysis_version.load(Ordering::Acquire);

    let result = crate::handlers::did_change_text_document(
        &mut state,
        DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier::new(uri.clone(), 1),
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: project.read_file("/Unchanged.sol"),
            }],
        },
    );

    assert!(result.is_continue());
    assert_eq!(state.analysis_version.load(Ordering::Acquire), analysis_version);
    let mut request = std::pin::pin!(crate::handlers::workspace_diagnostic(
        &mut state,
        workspace_diagnostic_params(),
    ));
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    let Poll::Ready(response) = request.as_mut().poll(&mut context) else {
        panic!("workspace report should use the current analysis")
    };
    let response = response.unwrap();
    let WorkspaceDiagnosticReportResult::Report(response) = response else {
        panic!("workspace diagnostic should return a complete response")
    };
    let [WorkspaceDocumentDiagnosticReport::Full(report)] = response.items.as_slice() else {
        panic!("workspace diagnostic should contain one full report")
    };
    assert_eq!(report.uri, uri);
    assert_eq!(report.version, Some(1));
}

#[test]
fn current_analysis_cannot_overwrite_a_newer_unchanged_document_version() {
    let project = TestProject::from_fixture(
        r#"
        //- /Unchanged.sol open
        contract Unchanged {}
        "#,
    );
    let uri = Url::from_file_path(project.path("/Unchanged.sol")).unwrap();
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.vfs = Arc::new(RwLock::new(project.vfs()));
    state.mark_analysis_pending_for_test();
    let pending_version = state.analysis_version.load(Ordering::Acquire);
    let result = crate::handlers::did_change_text_document(
        &mut state,
        DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier::new(uri.clone(), 1),
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: project.read_file("/Unchanged.sol"),
            }],
        },
    );
    assert!(result.is_continue());
    assert_eq!(state.analysis_version.load(Ordering::Acquire), pending_version);

    assert!(state.snapshot().publish_analysis(
        pending_version,
        AnalysisResult {
            analyzed_documents: AnalyzedDocuments::from_iter([(uri.clone(), Some(0))]),
            diagnostics: DiagnosticMap::default(),
            symbol_tables: SymbolTables::default(),
        },
    ));

    let reports = state.diagnostics.read().workspace_pull_reports(Vec::new());
    let [report] = reports.as_slice() else {
        panic!("workspace diagnostic should contain one report")
    };
    assert_eq!(report.uri, uri);
    assert_eq!(report.version, Some(1));
}

#[test]
fn changed_document_version_does_not_relabel_pending_analysis() {
    let project = TestProject::from_fixture(
        r#"
        //- /Changed.sol open
        contract Old {}
        "#,
    );
    let path = project.path("/Changed.sol");
    let uri = Url::from_file_path(&path).unwrap();
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.vfs = Arc::new(RwLock::new(project.vfs()));
    state.mark_analysis_pending_for_test();
    let pending_version = state.analysis_version.load(Ordering::Acquire);

    state.vfs.write().set_file_contents_with_version(
        crate::vfs::VfsPath::from(path),
        Some(crop::Rope::from("contract New {}")),
        Some(1),
    );
    assert!(state.snapshot().publish_analysis(
        pending_version,
        AnalysisResult {
            analyzed_documents: AnalyzedDocuments::from_iter([(uri.clone(), Some(0))]),
            diagnostics: DiagnosticMap::from_iter([(
                uri.clone(),
                vec![diagnostic("old analysis")],
            )]),
            symbol_tables: SymbolTables::default(),
        },
    ));

    let reports = state.diagnostics.read().workspace_pull_reports(Vec::new());
    let [report] = reports.as_slice() else {
        panic!("workspace diagnostic should contain one report")
    };
    assert_eq!(report.uri, uri);
    assert_eq!(report.version, Some(0));
    assert!(matches!(
        &report.report,
        PullReport::Full { diagnostics, .. }
            if diagnostics.as_slice() == [diagnostic("old analysis")]
    ));
}

#[tokio::test(flavor = "current_thread")]
async fn unchanged_edit_does_not_relabel_stale_diagnostics_after_failed_analysis() {
    let project = TestProject::from_fixture(
        r#"
        //- /Changed.sol open
        contract Old {}
        "#,
    );
    let uri = Url::from_file_path(project.path("/Changed.sol")).unwrap();
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.vfs = Arc::new(RwLock::new(project.vfs()));
    assert!(state.snapshot().publish_analysis(
        0,
        AnalysisResult {
            analyzed_documents: AnalyzedDocuments::from_iter([(uri.clone(), Some(0))]),
            diagnostics: DiagnosticMap::from_iter([(
                uri.clone(),
                vec![diagnostic("old analysis")],
            )]),
            symbol_tables: SymbolTables::default(),
        },
    ));

    let changed = |version| DidChangeTextDocumentParams {
        text_document: VersionedTextDocumentIdentifier::new(uri.clone(), version),
        content_changes: vec![TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: "contract New {}".into(),
        }],
    };
    assert!(crate::handlers::did_change_text_document(&mut state, changed(1)).is_continue());
    let failed_version = state.analysis_version.load(Ordering::Acquire);
    state.analysis_scheduler.tasks.lock().cancel();
    assert!(crate::handlers::did_change_text_document(&mut state, changed(2)).is_continue());
    assert_eq!(state.analysis_version.load(Ordering::Acquire), failed_version);

    let error = tokio::spawn(async { panic!("test document analysis failure") }).await.unwrap_err();
    assert!(
        handle_analysis_failure(
            failed_version,
            error,
            &state.analysis_version,
            &state.published_analysis_version,
            &state.analysis_commit,
        )
        .is_some()
    );
    assert!(crate::handlers::did_change_text_document(&mut state, changed(3)).is_continue());
    let retry_version = state.analysis_version.load(Ordering::Acquire);
    state.analysis_scheduler.tasks.lock().cancel();
    let error =
        tokio::spawn(async { panic!("test document analysis retry failure") }).await.unwrap_err();
    assert!(
        handle_analysis_failure(
            retry_version,
            error,
            &state.analysis_version,
            &state.published_analysis_version,
            &state.analysis_commit,
        )
        .is_some()
    );

    let response = crate::handlers::workspace_diagnostic(&mut state, workspace_diagnostic_params())
        .await
        .unwrap();
    let WorkspaceDiagnosticReportResult::Report(response) = response else {
        panic!("workspace diagnostic should return a complete response")
    };
    let [WorkspaceDocumentDiagnosticReport::Full(report)] = response.items.as_slice() else {
        panic!("workspace diagnostic should contain one full report")
    };
    assert_eq!(report.uri, uri);
    assert_eq!(report.version, Some(0));
    assert_eq!(
        report.full_document_diagnostic_report.items.as_slice(),
        [diagnostic("old analysis")]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn removed_workspace_membership_stays_cleared_after_failed_reindex() {
    let project = TestProject::from_fixture(
        r#"
        //- /removed/Stale.sol
        contract Stale {}

        //- /removed/kept/Current.sol
        contract Current {}
        "#,
    );
    let removed_root = project.path("/removed");
    let removed_uri = Url::from_file_path(project.path("/removed/Stale.sol")).unwrap();
    let kept_uri = Url::from_file_path(project.path("/removed/kept/Current.sol")).unwrap();
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(project.config_with_roots(&["/removed", "/removed/kept"]));
    assert!(state.snapshot().publish_analysis(
        0,
        AnalysisResult {
            analyzed_documents: AnalyzedDocuments::from_iter([
                (removed_uri.clone(), None),
                (kept_uri.clone(), None),
            ]),
            diagnostics: DiagnosticMap::from_iter([(
                removed_uri.clone(),
                vec![diagnostic("stale")],
            )]),
            symbol_tables: SymbolTables::default(),
        },
    ));
    let previous = state
        .diagnostics
        .read()
        .workspace_pull_reports(Vec::new())
        .into_iter()
        .map(|report| PreviousResultId {
            uri: report.uri,
            value: match report.report {
                PullReport::Full { result_id, .. } | PullReport::Unchanged { result_id } => {
                    result_id
                }
            },
        })
        .collect::<Vec<_>>();

    let result = crate::handlers::did_change_workspace_folders(
        &mut state,
        DidChangeWorkspaceFoldersParams {
            event: WorkspaceFoldersChangeEvent {
                added: Vec::new(),
                removed: vec![WorkspaceFolder {
                    uri: Url::from_file_path(removed_root).unwrap(),
                    name: "removed".into(),
                }],
            },
        },
    );

    assert!(result.is_continue());
    let failed_version = state.analysis_version.load(Ordering::Acquire);
    state.analysis_scheduler.tasks.lock().cancel();
    let error = tokio::spawn(async { panic!("test workspace reindex failure") }).await.unwrap_err();
    assert!(
        handle_analysis_failure(
            failed_version,
            error,
            &state.analysis_version,
            &state.published_analysis_version,
            &state.analysis_commit,
        )
        .is_some()
    );
    tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
        .await
        .expect("failed workspace reindex should release waiters")
        .unwrap();

    let reports = state.diagnostics.read().workspace_pull_reports(previous);
    let removed = reports.iter().find(|report| report.uri == removed_uri).unwrap();
    assert!(matches!(
        &removed.report,
        PullReport::Full { diagnostics, .. } if diagnostics.is_empty()
    ));
    let kept = reports.iter().find(|report| report.uri == kept_uri).unwrap();
    assert!(matches!(kept.report, PullReport::Unchanged { .. }));
}

#[tokio::test(flavor = "current_thread")]
async fn stale_clearing_report_keeps_the_removed_open_document_version() {
    let project = TestProject::from_fixture(
        r#"
        //- /removed/Open.sol open
        contract Open {}
        "#,
    );
    let removed_root = project.path("/removed");
    let path = project.path("/removed/Open.sol");
    let uri = Url::from_file_path(&path).unwrap();
    let mut vfs = project.vfs();
    vfs.set_file_contents_with_version(
        crate::vfs::VfsPath::from(path),
        Some(crop::Rope::from(project.read_file("/removed/Open.sol"))),
        Some(7),
    );
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(project.config_with_roots(&["/removed"]));
    state.vfs = Arc::new(RwLock::new(vfs));
    assert!(state.snapshot().publish_analysis(
        0,
        AnalysisResult {
            analyzed_documents: AnalyzedDocuments::from_iter([(uri.clone(), Some(7))]),
            diagnostics: DiagnosticMap::from_iter([(uri.clone(), vec![diagnostic("stale")],)]),
            symbol_tables: SymbolTables::default(),
        },
    ));
    let [initial] = state.diagnostics.read().workspace_pull_reports(Vec::new()).try_into().unwrap();
    let PullReport::Full { result_id, .. } = initial.report else {
        panic!("initial workspace report should be full")
    };

    assert!(
        crate::handlers::did_change_workspace_folders(
            &mut state,
            DidChangeWorkspaceFoldersParams {
                event: WorkspaceFoldersChangeEvent {
                    added: Vec::new(),
                    removed: vec![WorkspaceFolder {
                        uri: Url::from_file_path(removed_root).unwrap(),
                        name: "removed".into(),
                    }],
                },
            },
        )
        .is_continue()
    );
    let failed_version = state.analysis_version.load(Ordering::Acquire);
    state.analysis_scheduler.tasks.lock().cancel();
    let error = tokio::spawn(async { panic!("test workspace reindex failure") }).await.unwrap_err();
    assert!(
        handle_analysis_failure(
            failed_version,
            error,
            &state.analysis_version,
            &state.published_analysis_version,
            &state.analysis_commit,
        )
        .is_some()
    );

    let mut params = workspace_diagnostic_params();
    params.previous_result_ids = vec![PreviousResultId { uri: uri.clone(), value: result_id }];
    let response = crate::handlers::workspace_diagnostic(&mut state, params).await.unwrap();
    let WorkspaceDiagnosticReportResult::Report(response) = response else {
        panic!("workspace diagnostic should return a complete response")
    };
    let [WorkspaceDocumentDiagnosticReport::Full(report)] = response.items.as_slice() else {
        panic!("removed document should receive one full clearing report")
    };
    assert_eq!(report.uri, uri);
    assert_eq!(report.version, Some(7));
    assert!(report.full_document_diagnostic_report.items.is_empty());
}

#[test]
fn concurrent_workspace_diagnostic_requests_share_the_published_analysis() {
    let clean_uri = Url::from_file_path(std::env::temp_dir().join("A-Clean.sol")).unwrap();
    let broken_uri = Url::from_file_path(std::env::temp_dir().join("B-Broken.sol")).unwrap();
    let state = GlobalState::new(ClientSocket::new_closed());
    state.mark_analysis_pending_for_test();
    let requested_version = state.analysis_version.load(Ordering::Acquire);
    let mut snapshot = state.snapshot();
    let scheduler = state.analysis_scheduler.clone();
    let mut router = crate::new_router_with_state(state);
    let request = |id| {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "method": WorkspaceDiagnosticRequest::METHOD,
            "params": { "previousResultIds": [] },
        }))
        .unwrap()
    };
    let mut first = std::pin::pin!(router.call(request(1)));
    let mut second = std::pin::pin!(router.call(request(2)));
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);

    assert!(first.as_mut().poll(&mut context).is_pending());
    assert!(second.as_mut().poll(&mut context).is_pending());
    let tasks = scheduler.tasks.lock();
    assert!(tasks.coordinator.is_none());
    assert!(tasks.worker.is_none());
    drop(tasks);

    assert!(snapshot.publish_analysis(
        requested_version,
        AnalysisResult {
            analyzed_documents: AnalyzedDocuments::from_iter([
                (clean_uri.clone(), None),
                (broken_uri.clone(), Some(9)),
            ]),
            diagnostics: DiagnosticMap::from_iter([(
                broken_uri.clone(),
                vec![diagnostic("broken")],
            )]),
            symbol_tables: SymbolTables::default(),
        },
    ));

    for request in [&mut first, &mut second] {
        let Poll::Ready(response) = request.as_mut().poll(&mut context) else {
            panic!("workspace diagnostic should finish after publication");
        };
        let response =
            serde_json::from_value::<WorkspaceDiagnosticReportResult>(response.unwrap()).unwrap();
        let WorkspaceDiagnosticReportResult::Report(report) = response else {
            panic!("workspace diagnostic should return a complete report");
        };
        let [
            WorkspaceDocumentDiagnosticReport::Full(clean),
            WorkspaceDocumentDiagnosticReport::Full(broken),
        ] = report.items.as_slice()
        else {
            panic!("both requests should receive two full reports");
        };
        assert_eq!(clean.uri, clean_uri);
        assert_eq!(clean.version, None);
        assert!(clean.full_document_diagnostic_report.items.is_empty());
        assert_eq!(broken.uri, broken_uri);
        assert_eq!(broken.version, Some(9));
        assert_eq!(broken.full_document_diagnostic_report.items, vec![diagnostic("broken")]);
    }
}
