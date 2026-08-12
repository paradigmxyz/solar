use crate::{
    global_state::GlobalState,
    new_router_with_state, new_server_service, request_layer,
    test_support::{assert_request_cancelled, start_request},
};
use async_lsp::{
    AnyEvent, AnyNotification, AnyRequest, ClientSocket, LanguageServer, LspService, ResponseError,
    client_monitor::ClientProcessMonitorLayer, router::Router, server::LifecycleLayer,
    tracing::TracingLayer,
};
use lsp_types::{
    CancelParams, InitializeParams, InitializedParams, LogTraceParams, NumberOrString,
    SetTraceParams, TextDocumentIdentifier, TextDocumentSaveReason, TraceValue,
    WillSaveTextDocumentParams, WorkspaceSymbolParams, notification as notif,
    notification::Notification, request, request::Request,
};
use std::{
    future::Future,
    ops::ControlFlow,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    sync::{mpsc, oneshot},
};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use tower::{Service, ServiceBuilder};

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
        .layer(request_layer(client.clone()))
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

fn assert_server_processing_time(trace: &LogTraceParams) {
    trace
        .verbose
        .as_deref()
        .and_then(|detail| detail.strip_prefix("Server processing took "))
        .and_then(|detail| detail.strip_suffix(" ms"))
        .expect("verbose trace should contain server processing time")
        .parse::<u128>()
        .expect("server processing time should be numeric");
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
async fn server_notifications_do_not_emit_protocol_traces() {
    let mut harness = protocol_trace_harness();
    harness.initialize(None).await;
    let params = WillSaveTextDocumentParams {
        text_document: TextDocumentIdentifier {
            uri: lsp_types::Url::parse("file:///workspace/Secret.sol").unwrap(),
        },
        reason: TextDocumentSaveReason::MANUAL,
    };

    harness.set_trace(TraceValue::Messages);
    harness.server.notify::<notif::WillSaveTextDocument>(params.clone()).unwrap();
    harness.set_trace(TraceValue::Verbose);
    harness.server.notify::<notif::WillSaveTextDocument>(params).unwrap();
    harness.set_trace(TraceValue::Off);
    harness
        .server
        .request::<request::WorkspaceSymbolRequest>(WorkspaceSymbolParams {
            query: "notification barrier".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    harness.probe().await;

    assert!(harness.take_traces().is_empty());
    harness.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn set_trace_updates_server_request_detail() {
    let mut harness = protocol_trace_harness();
    harness.initialize(None).await;

    harness.set_trace(TraceValue::Messages);
    harness
        .server
        .request::<request::WorkspaceSymbolRequest>(WorkspaceSymbolParams {
            query: "messages trace".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    harness.set_trace(TraceValue::Verbose);
    harness
        .server
        .request::<request::WorkspaceSymbolRequest>(WorkspaceSymbolParams {
            query: "verbose trace".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    harness.set_trace(TraceValue::Messages);
    harness
        .server
        .request::<request::WorkspaceSymbolRequest>(WorkspaceSymbolParams {
            query: "messages trace again".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    harness.set_trace(TraceValue::Off);
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
    assert_server_processing_time(verbose);
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
async fn disabling_trace_during_a_request_suppresses_its_completion() {
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
}

#[tokio::test(flavor = "current_thread")]
async fn enabling_trace_during_a_request_does_not_create_a_completion() {
    const TIMEOUT: Duration = Duration::from_secs(1);

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
            assert_server_processing_time(completed);
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
    assert_server_processing_time(completed);

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
    assert_server_processing_time(completed);
    let trace_json = serde_json::to_string(&traces).unwrap();
    for secret in [PARAM_SECRET, RESULT_URI_SECRET, RESULT_SOURCE_SECRET, RESULT_TOKEN_SECRET] {
        assert!(!trace_json.contains(secret), "protocol trace leaked `{secret}`");
    }
    harness.shutdown().await;
}
