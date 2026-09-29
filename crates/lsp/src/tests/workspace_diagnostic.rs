use super::{
    indexing::{analysis_result, analysis_version, change, fail_analysis, state_with},
    *,
};
use crate::test_support::{read_lsp_frame, write_lsp_frame};
use async_lsp::LspService;
use lsp_types::{
    NumberOrString, PreviousResultId, WorkspaceDiagnosticParams, WorkspaceDiagnosticReport,
    WorkspaceDiagnosticReportPartialResult, WorkspaceDiagnosticReportResult,
    WorkspaceDocumentDiagnosticReport,
    request::{Request, WorkspaceDiagnosticRequest},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{BufReader, DuplexStream, ReadHalf, WriteHalf};
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
    value: Value,
}

/// A server connected to a raw LSP stream, for checking message order on the wire.
struct Wire {
    reader: BufReader<ReadHalf<DuplexStream>>,
    writer: WriteHalf<DuplexStream>,
    server_task: tokio::task::JoinHandle<async_lsp::Result<()>>,
}

impl Wire {
    fn new<T>(setup: impl FnOnce(&GlobalState) -> T) -> (Self, T) {
        let mut output = None;
        let (server_main, _client) = async_lsp::MainLoop::new_server(|client| {
            let request_client = client.clone();
            let state = GlobalState::new(client);
            output = Some(setup(&state));
            ServiceBuilder::new()
                .layer(crate::request_layer(request_client))
                .service(crate::new_router_with_state(state))
        });
        let (server_stream, client_stream) = tokio::io::duplex(64 << 10);
        let (server_reader, server_writer) = tokio::io::split(server_stream);
        let server_task = tokio::spawn(
            server_main.run_buffered(server_reader.compat(), server_writer.compat_write()),
        );
        let (reader, writer) = tokio::io::split(client_stream);
        (Self { reader: BufReader::new(reader), writer, server_task }, output.unwrap())
    }

    async fn send(&mut self, message: Value) {
        write_lsp_frame(&mut self.writer, message).await;
    }

    async fn recv(&mut self) -> Value {
        tokio::time::timeout(ASYNC_TEST_TIMEOUT, read_lsp_frame(&mut self.reader)).await.unwrap()
    }

    async fn exit(mut self) {
        self.send(
            json!({ "jsonrpc": "2.0", "method": notification::Exit::METHOD, "params": null }),
        )
        .await;
        assert!(self.server_task.await.unwrap().is_ok());
    }
}

fn workspace_request(id: impl Serialize, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": WorkspaceDiagnosticRequest::METHOD, "params": params })
}

fn cancel_request(id: &str) -> Value {
    json!({ "jsonrpc": "2.0", "method": notification::Cancel::METHOD, "params": { "id": id } })
}

fn temp_uri(name: &str) -> Url {
    Url::from_file_path(std::env::temp_dir().join(name)).unwrap()
}

fn clean_documents(prefix: &str) -> Vec<(Url, Option<i64>)> {
    (0..129).map(|index| (temp_uri(&format!("{prefix}-{index:03}.sol")), None)).collect()
}

fn publish(state: &GlobalState, version: usize, result: AnalysisResult) {
    assert!(state.snapshot().publish_analysis(version, result));
}

fn workspace_diagnostic_params(
    previous_result_ids: Vec<PreviousResultId>,
) -> WorkspaceDiagnosticParams {
    WorkspaceDiagnosticParams {
        identifier: None,
        previous_result_ids,
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    }
}

#[derive(Debug, PartialEq)]
enum Report {
    Full(Url, Option<i64>, Vec<Diagnostic>),
    Unchanged(Url, Option<i64>, String),
}

/// Summarizes a complete workspace response and collects the IDs a client would send back.
fn reports(result: WorkspaceDiagnosticReportResult) -> (Vec<Report>, Vec<PreviousResultId>) {
    let WorkspaceDiagnosticReportResult::Report(result) = result else {
        panic!("workspace diagnostic should return a complete response")
    };
    result
        .items
        .into_iter()
        .map(|report| match report {
            WorkspaceDocumentDiagnosticReport::Full(report) => {
                let full = report.full_document_diagnostic_report;
                let value = full.result_id.unwrap();
                let previous = PreviousResultId { uri: report.uri.clone(), value };
                (Report::Full(report.uri, report.version, full.items), previous)
            }
            WorkspaceDocumentDiagnosticReport::Unchanged(report) => {
                let value = report.unchanged_document_diagnostic_report.result_id;
                let previous = PreviousResultId { uri: report.uri.clone(), value: value.clone() };
                (Report::Unchanged(report.uri, report.version, value), previous)
            }
        })
        .unzip()
}

async fn pull(
    state: &mut GlobalState,
    previous_result_ids: Vec<PreviousResultId>,
) -> (Vec<Report>, Vec<PreviousResultId>) {
    let params = workspace_diagnostic_params(previous_result_ids);
    reports(crate::handlers::workspace_diagnostic(state, params).await.unwrap())
}

fn pull_ready(
    state: &mut GlobalState,
    previous_result_ids: Vec<PreviousResultId>,
) -> (Vec<Report>, Vec<PreviousResultId>) {
    let params = workspace_diagnostic_params(previous_result_ids);
    reports(expect_ready(crate::handlers::workspace_diagnostic(state, params)).unwrap())
}

fn open_project_state(fixture: &str) -> (TestProject, GlobalState) {
    let project = TestProject::from_fixture(fixture);
    let state = GlobalState::new(ClientSocket::new_closed());
    *state.vfs.write() = project.vfs();
    (project, state)
}

#[tokio::test(flavor = "current_thread")]
async fn workspace_diagnostic_progress_and_partial_batches_precede_the_final_response() {
    let uris = clean_documents("workspace-wire");
    let (mut wire, ()) = Wire::new(|state| {
        publish(state, 0, analysis_result(uris.clone(), []));
    });
    let params = json!({
        "previousResultIds": [],
        "partialResultToken": "workspace-wire-partial",
        "workDoneToken": "workspace-wire-work-done",
    });
    wire.send(workspace_request(1, params)).await;

    let begin = wire.recv().await;
    assert_eq!(begin["method"], notification::Progress::METHOD);
    assert_eq!(begin["params"]["token"], "workspace-wire-work-done");
    assert_eq!(
        begin["params"]["value"],
        json!({ "kind": "begin", "title": "Workspace diagnostics", "cancellable": false })
    );
    let mut batch_sizes = Vec::new();
    let mut actual_uris = Vec::new();
    for _ in 0..3 {
        let partial = wire.recv().await;
        assert_eq!(partial["method"], notification::Progress::METHOD);
        assert_eq!(partial["params"]["token"], "workspace-wire-partial");
        let partial = serde_json::from_value::<WorkspaceDiagnosticReportPartialResult>(
            partial["params"]["value"].clone(),
        )
        .unwrap();
        batch_sizes.push(partial.items.len());
        let result = WorkspaceDiagnosticReport { items: partial.items }.into();
        actual_uris.extend(reports(result).0.into_iter().map(|report| {
            let Report::Full(uri, None, diagnostics) = report else {
                panic!("first workspace pull should contain unversioned full reports")
            };
            assert!(diagnostics.is_empty());
            uri
        }));
    }
    assert_eq!(batch_sizes, [64, 64, 1]);
    assert_eq!(actual_uris, uris.into_iter().map(|(uri, _)| uri).collect::<Vec<_>>());
    let end = wire.recv().await;
    assert_eq!(end["params"]["token"], "workspace-wire-work-done");
    assert_eq!(end["params"]["value"]["kind"], "end");
    let response = wire.recv().await;
    assert_eq!(response["id"], 1);
    assert_eq!(response["result"], json!({ "items": [] }));
    wire.exit().await;
}

#[tokio::test(flavor = "current_thread")]
async fn cancelling_workspace_diagnostics_sends_end_before_the_error_without_starting_analysis() {
    let (mut wire, (requested_version, analysis_version, scheduler)) = Wire::new(|state| {
        state.mark_analysis_pending_for_test();
        let requested_version = super::indexing::analysis_version(state);
        (requested_version, state.analysis_version.clone(), state.analysis_scheduler.clone())
    });
    let params =
        json!({ "previousResultIds": [], "workDoneToken": "workspace-wire-cancel-progress" });
    wire.send(workspace_request("workspace-wire-cancel", params)).await;

    let begin = wire.recv().await;
    assert_eq!(begin["params"]["token"], "workspace-wire-cancel-progress");
    assert_eq!(begin["params"]["value"]["kind"], "begin");
    wire.send(cancel_request("workspace-wire-cancel")).await;
    let end = wire.recv().await;
    assert_eq!(end["method"], notification::Progress::METHOD);
    assert_eq!(end["params"]["token"], "workspace-wire-cancel-progress");
    assert_eq!(end["params"]["value"]["kind"], "end");
    let response = wire.recv().await;
    assert_eq!(response["id"], "workspace-wire-cancel");
    assert_eq!(response["error"]["code"], ErrorCode::REQUEST_CANCELLED.0);
    assert_eq!(response["error"]["message"], "Client cancelled the request");

    assert_eq!(analysis_version.load(Ordering::Acquire), requested_version);
    {
        let tasks = scheduler.tasks.lock();
        assert!(tasks.coordinator.is_none());
        assert!(tasks.worker.is_none());
    }
    wire.exit().await;
}

#[tokio::test(flavor = "current_thread")]
async fn workspace_diagnostics_can_be_cancelled_between_partial_batches() {
    let (server_main, client) = async_lsp::MainLoop::new_server(|_| {
        let mut router = Router::new(());
        router.notification::<notification::Exit>(|_, ()| ControlFlow::Break(Ok(())));
        router
    });
    let (progress_tx, mut progress) = mpsc::unbounded_channel();
    let (client_main, server) = async_lsp::MainLoop::new_client(move |_| {
        let mut router = Router::new(progress_tx);
        router.notification::<RawProgress>(|progress, params| {
            progress.send(params).unwrap();
            ControlFlow::Continue(())
        });
        router
    });
    let (server_task, client_task) = spawn_lsp_pair(server_main, client_main);
    let state = GlobalState::new(client);
    publish(&state, 0, analysis_result(clean_documents("workspace-cancel"), []));
    let router = crate::new_router_with_state(state);
    let mut service = crate::request_layer(ClientSocket::new_closed()).layer(router);
    std::future::poll_fn(|context| service.poll_ready(context)).await.unwrap();
    let params =
        json!({ "previousResultIds": [], "partialResultToken": "workspace-cancel-partial" });
    let request = serde_json::from_value(workspace_request("workspace-mid-stream-cancel", params));
    let mut response = std::pin::pin!(service.call(request.unwrap()));

    assert!(response.as_mut().poll(&mut Context::from_waker(Waker::noop())).is_pending());
    let partial = tokio::time::timeout(ASYNC_TEST_TIMEOUT, progress.recv()).await.unwrap().unwrap();
    assert_eq!(partial.token, NumberOrString::String("workspace-cancel-partial".into()));
    let partial =
        serde_json::from_value::<WorkspaceDiagnosticReportPartialResult>(partial.value).unwrap();
    assert_eq!(partial.items.len(), 64);

    let cancel = serde_json::from_value(cancel_request("workspace-mid-stream-cancel")).unwrap();
    assert!(service.notify(cancel).is_continue());

    assert_eq!(response.await.unwrap_err().code, ErrorCode::REQUEST_CANCELLED);
    assert!(matches!(progress.try_recv(), Err(mpsc::error::TryRecvError::Empty)));
    server.notify::<notification::Exit>(()).unwrap();
    assert!(server_task.await.unwrap().is_ok());
    assert!(matches!(client_task.await.unwrap(), Err(async_lsp::Error::Eof)));
}

#[test]
fn workspace_diagnostics_round_trip_previous_result_ids_and_clear_stale_reports_once() {
    let current = temp_uri("A-Current-Diagnostic.sol");
    let stale = temp_uri("B-Stale-Diagnostic.sol");
    let mut state = GlobalState::new(ClientSocket::new_closed());
    let documents = [(current.clone(), Some(9)), (stale.clone(), None)];
    publish(&state, 0, analysis_result(documents, [(stale.clone(), vec![diagnostic("stale")])]));

    let (initial, previous) = pull_ready(&mut state, Vec::new());
    assert_eq!(
        initial,
        [
            Report::Full(current.clone(), Some(9), Vec::new()),
            Report::Full(stale.clone(), None, vec![diagnostic("stale")]),
        ]
    );
    let current_id = previous[0].value.clone();
    let stale_id = previous[1].value.clone();

    let unchanged = pull_ready(&mut state, previous.clone()).0;
    assert_eq!(
        unchanged,
        [
            Report::Unchanged(current.clone(), Some(9), current_id.clone()),
            Report::Unchanged(stale.clone(), None, stale_id.clone()),
        ]
    );

    publish(&state, 0, analysis_result([(current.clone(), Some(9))], []));
    // Stale IDs sent for an equivalent spelling still clear the canonical document.
    let encoded = Url::parse(&stale.as_str().replacen("Stale", "%53tale", 1)).unwrap();
    let previous = vec![previous[0].clone(), PreviousResultId { uri: encoded, value: stale_id }];
    let (cleared, previous) = pull_ready(&mut state, previous);
    assert_eq!(
        cleared,
        [
            Report::Unchanged(current.clone(), Some(9), current_id.clone()),
            Report::Full(stale, None, Vec::new()),
        ]
    );

    let acknowledged = pull_ready(&mut state, previous).0;
    assert_eq!(acknowledged, [Report::Unchanged(current, Some(9), current_id)]);
}

#[test]
fn unchanged_document_changes_relabel_report_versions_without_analysis() {
    let (project, mut state) =
        open_project_state("//- /Unchanged.sol open\ncontract Unchanged {}\n");
    let uri = Url::from_file_path(project.path("/Unchanged.sol")).unwrap();
    let text = project.read_file("/Unchanged.sol");
    publish(&state, 0, analysis_result([(uri.clone(), Some(0))], []));
    let version = analysis_version(&state);

    change(&mut state, &uri, 1, text.clone());
    assert_eq!(analysis_version(&state), version);
    assert_eq!(
        pull_ready(&mut state, Vec::new()).0,
        [Report::Full(uri.clone(), Some(1), Vec::new())]
    );

    // An analysis started before the unchanged edit cannot overwrite the newer version.
    state.mark_analysis_pending_for_test();
    let pending = analysis_version(&state);
    change(&mut state, &uri, 2, text);
    assert_eq!(analysis_version(&state), pending);
    publish(&state, pending, analysis_result([(uri.clone(), Some(0))], []));
    assert_eq!(pull_ready(&mut state, Vec::new()).0, [Report::Full(uri, Some(2), Vec::new())]);
}

#[test]
fn changed_document_version_does_not_relabel_pending_analysis() {
    let (project, mut state) = open_project_state("//- /Changed.sol open\ncontract Old {}\n");
    let path = project.path("/Changed.sol");
    let uri = Url::from_file_path(&path).unwrap();
    state.mark_analysis_pending_for_test();
    let pending = analysis_version(&state);

    super::indexing::set_overlay(&state, &path, "contract New {}", 1);
    let old = [(uri.clone(), vec![diagnostic("old analysis")])];
    publish(&state, pending, analysis_result([(uri.clone(), Some(0))], old));

    let reports = pull_ready(&mut state, Vec::new()).0;
    assert_eq!(reports, [Report::Full(uri, Some(0), vec![diagnostic("old analysis")])]);
}

#[tokio::test(flavor = "current_thread")]
async fn unchanged_edit_does_not_relabel_stale_diagnostics_after_failed_analysis() {
    let (project, mut state) = open_project_state("//- /Changed.sol open\ncontract Old {}\n");
    let uri = Url::from_file_path(project.path("/Changed.sol")).unwrap();
    let old = [(uri.clone(), vec![diagnostic("old analysis")])];
    publish(&state, 0, analysis_result([(uri.clone(), Some(0))], old));

    change(&mut state, &uri, 1, "contract New {}");
    let failed_version = analysis_version(&state);
    super::indexing::cancel_analysis(&state);
    change(&mut state, &uri, 2, "contract New {}");
    assert_eq!(analysis_version(&state), failed_version);
    fail_analysis(&state, "test document analysis failure");
    change(&mut state, &uri, 3, "contract New {}");
    fail_analysis(&state, "test document analysis retry failure");

    let reports = pull(&mut state, Vec::new()).await.0;
    assert_eq!(reports, [Report::Full(uri, Some(0), vec![diagnostic("old analysis")])]);
}

#[tokio::test(flavor = "current_thread")]
async fn removed_workspace_membership_stays_cleared_after_failed_reindex() {
    let project = TestProject::from_fixture(
        r#"
        //- /removed/Stale.sol open
        contract Stale {}

        //- /removed/kept/Current.sol
        contract Current {}
        "#,
    );
    let removed_uri = Url::from_file_path(project.path("/removed/Stale.sol")).unwrap();
    let kept_uri = Url::from_file_path(project.path("/removed/kept/Current.sol")).unwrap();
    let mut state = state_with(project.config_with_roots(&["/removed", "/removed/kept"]));
    *state.vfs.write() = project.vfs();
    super::indexing::set_overlay(
        &state,
        &project.path("/removed/Stale.sol"),
        "contract Stale {}",
        7,
    );
    let documents = [(removed_uri.clone(), Some(7)), (kept_uri.clone(), None)];
    publish(
        &state,
        0,
        analysis_result(documents, [(removed_uri.clone(), vec![diagnostic("stale")])]),
    );
    let previous = pull_ready(&mut state, Vec::new()).1;

    let removed = WorkspaceFolder {
        uri: Url::from_file_path(project.path("/removed")).unwrap(),
        name: "removed".into(),
    };
    let event = WorkspaceFoldersChangeEvent { added: Vec::new(), removed: vec![removed] };
    let params = DidChangeWorkspaceFoldersParams { event };
    assert!(crate::handlers::did_change_workspace_folders(&mut state, params).is_continue());
    fail_analysis(&state, "test workspace reindex failure");

    // The stale clearing report keeps the version of the removed open document.
    let (reports, _) = pull(&mut state, previous.clone()).await;
    assert_eq!(
        reports,
        [
            Report::Full(removed_uri, Some(7), Vec::new()),
            Report::Unchanged(kept_uri, None, previous[1].value.clone()),
        ]
    );
}

#[test]
fn concurrent_workspace_diagnostic_requests_share_the_published_analysis() {
    let clean = temp_uri("A-Clean.sol");
    let broken = temp_uri("B-Broken.sol");
    let state = GlobalState::new(ClientSocket::new_closed());
    state.mark_analysis_pending_for_test();
    let requested_version = analysis_version(&state);
    let mut snapshot = state.snapshot();
    let scheduler = state.analysis_scheduler.clone();
    let mut router = crate::new_router_with_state(state);
    let request = |id| {
        serde_json::from_value(workspace_request(id, json!({ "previousResultIds": [] }))).unwrap()
    };
    let mut first = std::pin::pin!(router.call(request(1)));
    let mut second = std::pin::pin!(router.call(request(2)));
    let mut context = Context::from_waker(Waker::noop());

    assert!(first.as_mut().poll(&mut context).is_pending());
    assert!(second.as_mut().poll(&mut context).is_pending());
    let tasks = scheduler.tasks.lock();
    assert!(tasks.coordinator.is_none());
    assert!(tasks.worker.is_none());
    drop(tasks);

    let documents = [(clean.clone(), None), (broken.clone(), Some(9))];
    let result = analysis_result(documents, [(broken.clone(), vec![diagnostic("broken")])]);
    assert!(snapshot.publish_analysis(requested_version, result));

    for request in [&mut first, &mut second] {
        let Poll::Ready(response) = request.as_mut().poll(&mut context) else {
            panic!("workspace diagnostic should finish after publication");
        };
        let response = serde_json::from_value(response.unwrap()).unwrap();
        assert_eq!(
            reports(response).0,
            [
                Report::Full(clean.clone(), None, Vec::new()),
                Report::Full(broken.clone(), Some(9), vec![diagnostic("broken")]),
            ]
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn invalidation_returns_retryable_server_cancellation() {
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.mark_analysis_pending_for_test();
    let params = workspace_diagnostic_params(Vec::new());
    let mut request = std::pin::pin!(crate::handlers::workspace_diagnostic(&mut state, params));
    let mut cx = Context::from_waker(Waker::noop());
    assert!(request.as_mut().poll(&mut cx).is_pending());
    state.mark_analysis_pending_for_test();
    let Poll::Ready(Err(error)) = request.as_mut().poll(&mut cx) else {
        panic!("invalidation must cancel the diagnostic pull without publication");
    };
    assert_eq!(error.code, ErrorCode::SERVER_CANCELLED);
    assert_eq!(error.data, None);
}
