use crate::{
    LaunchConfig,
    global_state::GlobalState,
    new_router_with_state, new_server_service, new_server_service_with_router,
    protocol_trace::{ProtocolTrace, ProtocolTraceLayer},
    test_support::{assert_request_cancelled, spawn_lsp_pair, start_request},
};
use async_lsp::{
    AnyEvent, AnyNotification, AnyRequest, ClientSocket, LanguageServer, LspService, ResponseError,
    router::Router,
};
use lsp_types::{
    CancelParams, InitializeParams, InitializeResult, InitializedParams, LogTraceParams,
    NumberOrString, SetTraceParams, TextDocumentIdentifier, TextDocumentSaveReason, TraceValue,
    WillSaveTextDocumentParams, WorkspaceSymbolParams, notification as notif, request,
    request::Request,
};
use serde_json::json;
use std::{
    future::Future,
    ops::ControlFlow,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use tokio::sync::{mpsc, oneshot};
use tower::{Service, ServiceBuilder};

const TIMEOUT: Duration = Duration::from_secs(1);

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

    async fn workspace_symbols(&self) {
        let params = WorkspaceSymbolParams { query: "query-secret".into(), ..Default::default() };
        self.server.request::<request::WorkspaceSymbolRequest>(params).await.unwrap();
    }

    /// Starts a request that the test router holds until the paired sender releases it.
    async fn start_pending(
        &self,
        entered: oneshot::Receiver<NumberOrString>,
    ) -> (impl Future<Output = async_lsp::Result<()>> + use<>, NumberOrString) {
        let server = self.server.clone();
        let request = start_request(async move { server.request::<PendingTraceRequest>(()).await });
        let id = tokio::time::timeout(TIMEOUT, entered)
            .await
            .expect("pending request should start")
            .expect("pending request should signal entry");
        (request, id)
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
        self.exit().await;
    }

    async fn exit(mut self) {
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

    let (server_task, client_task) = spawn_lsp_pair(server_main, client_main);

    ProtocolTraceHarness { client, server, traces, server_task, client_task }
}

fn protocol_trace_harness() -> ProtocolTraceHarness {
    protocol_trace_harness_with(|client| new_server_service(client, LaunchConfig::default()))
}

fn protocol_trace_test_harness(pending: Option<PendingTraceControl>) -> ProtocolTraceHarness {
    protocol_trace_harness_with(move |client| {
        new_server_service_with_router(client, LaunchConfig::default(), |state| {
            ProtocolTraceTestRouter { inner: new_router_with_state(state), pending }
        })
    })
}

/// Returns an initialized harness with a pending request gate at the `Messages` trace level.
async fn pending_trace_harness()
-> (ProtocolTraceHarness, oneshot::Receiver<NumberOrString>, oneshot::Sender<()>) {
    let (entered, request_entered) = oneshot::channel();
    let (release_request, release) = oneshot::channel();
    let mut harness = protocol_trace_test_harness(Some(PendingTraceControl { entered, release }));
    harness.initialize(None).await;
    (harness, request_entered, release_request)
}

fn trace(message: &str) -> LogTraceParams {
    LogTraceParams { message: message.into(), verbose: None }
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
    let mut harness = protocol_trace_harness_with(|client| {
        let trace = ProtocolTrace::new(client);
        trace.set_level(TraceValue::Messages);
        let mut router = Router::new(());
        router
            .request::<request::Initialize, _>(|_, _| {
                std::future::ready(Ok(InitializeResult::default()))
            })
            .notification::<notif::Exit>(|_, ()| ControlFlow::Break(Ok(())));
        ServiceBuilder::new().layer(ProtocolTraceLayer::new(trace)).service(router)
    });
    harness.server.initialize(InitializeParams::default()).await.unwrap();

    let error = harness.server.request::<TraceBarrierRequest>(()).await.unwrap_err();
    // The client loop handles the trace before the response frame that follows it.
    assert_eq!(
        harness.take_traces(),
        [trace("Server completed request `test/traceBarrier` with an error")]
    );
    let async_lsp::Error::Response(error) = error else { panic!("expected response error") };
    assert_eq!(error.code, async_lsp::ErrorCode::METHOD_NOT_FOUND);
    harness.exit().await;
}

#[tokio::test(flavor = "current_thread")]
async fn set_trace_updates_request_detail_without_tracing_notifications() {
    let mut harness = protocol_trace_harness();
    harness.initialize(None).await;
    let will_save = WillSaveTextDocumentParams {
        text_document: TextDocumentIdentifier {
            uri: lsp_types::Url::parse("file:///workspace/Secret.sol").unwrap(),
        },
        reason: TextDocumentSaveReason::MANUAL,
    };

    for level in [TraceValue::Messages, TraceValue::Verbose, TraceValue::Messages, TraceValue::Off]
    {
        harness.set_trace(level);
        harness.server.notify::<notif::WillSaveTextDocument>(will_save.clone()).unwrap();
        harness.workspace_symbols().await;
    }
    harness.probe().await;

    let traces = harness.take_traces();
    let [messages, verbose, messages_again] = traces.as_slice() else {
        panic!("expected three completion traces, got {traces:?}");
    };
    let completed = "Server completed request `workspace/symbol` successfully";
    assert_eq!(messages, &trace(completed));
    assert_eq!(verbose.message, completed);
    assert_server_processing_time(verbose);
    assert_eq!(messages_again, &trace(completed));
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
async fn disabling_trace_during_a_request_suppresses_its_completion() {
    let (mut harness, entered, release) = pending_trace_harness().await;
    harness.set_trace(TraceValue::Messages);
    let (request, _) = harness.start_pending(entered).await;

    harness.set_trace(TraceValue::Off);
    harness.server.request::<TraceBarrierRequest>(()).await.unwrap();
    release.send(()).expect("pending request should still be running");
    tokio::time::timeout(TIMEOUT, request).await.unwrap().unwrap();
    harness.probe().await;

    assert_eq!(harness.take_traces(), []);
    harness.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn enabling_trace_during_a_request_does_not_create_a_completion() {
    let (mut harness, entered, release) = pending_trace_harness().await;
    let (request, _) = harness.start_pending(entered).await;

    harness.set_trace(TraceValue::Messages);
    harness.server.request::<TraceBarrierRequest>(()).await.unwrap();
    harness.probe().await;
    assert_eq!(
        harness.take_traces(),
        [trace("Server completed request `test/traceBarrier` successfully")]
    );

    release.send(()).expect("pending request should still be running");
    tokio::time::timeout(TIMEOUT, request).await.unwrap().unwrap();
    harness.probe().await;
    assert!(harness.take_traces().is_empty());
    harness.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn messages_trace_reports_cancelled_requests_as_errors_without_ids() {
    let (mut harness, entered, _release) = pending_trace_harness().await;
    harness.set_trace(TraceValue::Messages);
    let (request, id) = harness.start_pending(entered).await;

    harness.server.notify::<notif::Cancel>(CancelParams { id }).unwrap();
    assert_request_cancelled(tokio::time::timeout(TIMEOUT, request).await.unwrap());
    harness.probe().await;

    assert_eq!(
        harness.take_traces(),
        [trace("Server completed request `test/pendingTrace` with an error")]
    );
    harness.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn initialize_trace_level_applies_after_the_initialize_response() {
    for (level, verbose) in [(TraceValue::Messages, false), (TraceValue::Verbose, true)] {
        let mut harness = protocol_trace_harness();
        harness.initialize(Some(level)).await;
        harness.workspace_symbols().await;
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
async fn verbose_request_traces_report_timing_without_sensitive_data() {
    const SOURCE_SECRET: &str = "contract TraceSecret {}";
    const ENV_SECRET: &str = "environment-value-secret";
    const QUERY_SECRET: &str = "workspace-query-secret";
    const ERROR_MESSAGE_SECRET: &str = "error-message-secret";
    const ERROR_DATA_SECRET: &str = "error-data-secret";
    const PARAM_SECRET: &str = "request-parameter-secret";
    const RESULT_URI_SECRET: &str = "file:///workspace/ResultSecret.sol";
    const RESULT_SOURCE_SECRET: &str = "contract ResultSecret {}";
    const RESULT_TOKEN_SECRET: &str = "result-token-secret";

    let mut harness = protocol_trace_test_harness(None);
    harness.initialize(None).await;
    harness.set_trace(TraceValue::Verbose);

    let error = harness
        .server
        .request::<SensitiveTraceRequest>(json!({
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
    assert_eq!(error.data, Some(json!({ "token": ERROR_DATA_SECRET })));
    let result = harness
        .server
        .request::<SensitiveTraceResultRequest>(json!({ "query": PARAM_SECRET }))
        .await
        .unwrap();
    assert_eq!(
        result,
        json!({
            "uri": RESULT_URI_SECRET,
            "text": RESULT_SOURCE_SECRET,
            "token": RESULT_TOKEN_SECRET,
        })
    );
    harness.probe().await;

    let traces = harness.take_traces();
    let [failed, succeeded] = traces.as_slice() else {
        panic!("expected two completion traces, got {traces:?}");
    };
    assert_eq!(failed.message, "Server completed request `<redacted method>` with an error");
    assert_eq!(succeeded.message, "Server completed request `test/sensitiveResult` successfully");
    traces.iter().for_each(assert_server_processing_time);
    let trace_json = serde_json::to_string(&traces).unwrap();
    for secret in [
        SensitiveTraceRequest::METHOD,
        "file:///workspace/Secret.sol",
        SOURCE_SECRET,
        ENV_SECRET,
        QUERY_SECRET,
        ERROR_MESSAGE_SECRET,
        ERROR_DATA_SECRET,
        PARAM_SECRET,
        RESULT_URI_SECRET,
        RESULT_SOURCE_SECRET,
        RESULT_TOKEN_SECRET,
    ] {
        assert!(!trace_json.contains(secret), "protocol trace leaked `{secret}`");
    }
    harness.shutdown().await;
}
