use crate::{
    LaunchConfig,
    global_state::GlobalState,
    new_router_with_state, new_server_service, new_server_service_with_router,
    protocol_trace::{ProtocolTrace, ProtocolTraceLayer},
    test_support::{ClientHarness, assert_request_cancelled, from_json, start_request, within},
};
use async_lsp::{
    AnyEvent, AnyNotification, AnyRequest, ErrorCode, LspService, ResponseError, router::Router,
};
use lsp_types::{
    CancelParams, InitializeParams, InitializeResult, LogTraceParams, NumberOrString,
    SetTraceParams, TextDocumentSaveReason, TraceValue, WorkspaceSymbolParams,
    notification as notif, request, request::Request,
};
use serde_json::{Value, json};
use std::{
    future::Future,
    ops::ControlFlow,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::sync::oneshot;
use tower::{Service, ServiceBuilder};

macro_rules! test_request {
    ($name:ident, $method:literal, $ty:ty) => {
        enum $name {}

        impl Request for $name {
            type Params = $ty;
            type Result = $ty;

            const METHOD: &'static str = $method;
        }
    };
}

test_request!(SensitiveTraceRequest, "/workspace/Secret.sol", Value);
test_request!(SensitiveTraceResultRequest, "test/sensitiveResult", Value);
test_request!(PendingTraceRequest, "test/pendingTrace", ());
test_request!(TraceBarrierRequest, "test/traceBarrier", ());

struct PendingTraceControl {
    entered: oneshot::Sender<NumberOrString>,
    release: oneshot::Receiver<()>,
}

struct ProtocolTraceTestRouter {
    inner: Router<GlobalState>,
    pending: Option<PendingTraceControl>,
}

impl Service<AnyRequest> for ProtocolTraceTestRouter {
    type Response = Value;
    type Error = ResponseError;
    type Future = Pin<Box<dyn Future<Output = Result<Value, ResponseError>> + Send + 'static>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: AnyRequest) -> Self::Future {
        match &*request.method {
            SensitiveTraceRequest::METHOD => {
                Box::pin(std::future::ready(Err(ResponseError::new_with_data(
                    ErrorCode::REQUEST_FAILED,
                    "error-message-secret",
                    json!({ "token": "error-data-secret" }),
                ))))
            }
            SensitiveTraceResultRequest::METHOD => Box::pin(std::future::ready(Ok(json!({
                "uri": "file:///workspace/ResultSecret.sol",
                "text": "contract ResultSecret {}",
                "token": "result-token-secret",
            })))),
            PendingTraceRequest::METHOD => {
                let request_id = request.id.clone();
                let PendingTraceControl { entered, release } =
                    self.pending.take().expect("pending trace request should run once");
                Box::pin(async move {
                    entered.send(request_id).expect("pending trace receiver should be open");
                    release.await.expect("pending trace request should be released");
                    Ok(Value::Null)
                })
            }
            TraceBarrierRequest::METHOD => Box::pin(std::future::ready(Ok(Value::Null))),
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

async fn initialize(harness: &ClientHarness, trace: Option<TraceValue>) {
    harness.initialize(InitializeParams { trace, ..Default::default() }).await;
}

fn set_trace(harness: &ClientHarness, value: TraceValue) {
    harness.server().notify::<notif::SetTrace>(SetTraceParams { value }).unwrap();
}

async fn workspace_symbols(harness: &ClientHarness) {
    let params = WorkspaceSymbolParams { query: "query-secret".into(), ..Default::default() };
    harness.server().request::<request::WorkspaceSymbolRequest>(params).await.unwrap();
}

/// Starts a request that the test router holds until the paired sender releases it.
async fn start_pending(
    harness: &ClientHarness,
    entered: oneshot::Receiver<NumberOrString>,
) -> (impl Future<Output = async_lsp::Result<()>> + use<>, NumberOrString) {
    let server = harness.server().clone();
    let request = start_request(async move { server.request::<PendingTraceRequest>(()).await });
    let id = within("pending request", entered).await.expect("pending request should signal entry");
    (request, id)
}

async fn shutdown(harness: ClientHarness) {
    set_trace(&harness, TraceValue::Off);
    harness.shutdown().await;
}

fn protocol_trace_harness() -> ClientHarness {
    ClientHarness::with_server(|client| new_server_service(client, LaunchConfig::default()))
}

fn protocol_trace_test_harness(pending: Option<PendingTraceControl>) -> ClientHarness {
    ClientHarness::with_server(move |client| {
        new_server_service_with_router(client, LaunchConfig::default(), |state| {
            ProtocolTraceTestRouter { inner: new_router_with_state(state), pending }
        })
    })
}

/// Returns an initialized harness with a pending request gate at the `Messages` trace level.
async fn pending_trace_harness()
-> (ClientHarness, oneshot::Receiver<NumberOrString>, oneshot::Sender<()>) {
    let (entered, request_entered) = oneshot::channel();
    let (release_request, release) = oneshot::channel();
    let harness = protocol_trace_test_harness(Some(PendingTraceControl { entered, release }));
    initialize(&harness, None).await;
    (harness, request_entered, release_request)
}

#[track_caller]
fn response_error<T: std::fmt::Debug>(result: async_lsp::Result<T>) -> ResponseError {
    match result {
        Err(async_lsp::Error::Response(error)) => error,
        result => panic!("expected a response error, got {result:?}"),
    }
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
    let mut harness = ClientHarness::with_server(|client| {
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
    harness.server().request::<request::Initialize>(InitializeParams::default()).await.unwrap();

    let error = response_error(harness.server().request::<TraceBarrierRequest>(()).await);
    // The client loop handles the trace before the response frame that follows it.
    assert_eq!(
        harness.take_traces(),
        [trace("Server completed request `test/traceBarrier` with an error")]
    );
    assert_eq!(error.code, ErrorCode::METHOD_NOT_FOUND);
    harness.exit().await;
}

#[tokio::test(flavor = "current_thread")]
async fn set_trace_updates_request_detail_without_tracing_notifications() {
    let mut harness = protocol_trace_harness();
    initialize(&harness, None).await;
    let text_document = json!({ "uri": "file:///workspace/Secret.sol" });
    let will_save =
        json!({ "textDocument": text_document, "reason": TextDocumentSaveReason::MANUAL });

    for level in [TraceValue::Messages, TraceValue::Verbose, TraceValue::Messages, TraceValue::Off]
    {
        set_trace(&harness, level);
        harness
            .server()
            .notify::<notif::WillSaveTextDocument>(from_json(will_save.clone()))
            .unwrap();
        workspace_symbols(&harness).await;
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
    shutdown(harness).await;
}

#[tokio::test(flavor = "current_thread")]
async fn set_trace_before_initialize_does_not_emit_server_traces() {
    let mut harness = protocol_trace_test_harness(None);
    set_trace(&harness, TraceValue::Messages);
    let error = response_error(harness.server().request::<TraceBarrierRequest>(()).await);
    assert_eq!(error.code, ErrorCode::SERVER_NOT_INITIALIZED);

    initialize(&harness, None).await;
    harness.probe().await;

    assert!(harness.take_traces().is_empty());
    shutdown(harness).await;
}

#[tokio::test(flavor = "current_thread")]
async fn changing_trace_during_a_request_does_not_trace_its_completion() {
    for (before, during) in
        [(TraceValue::Messages, TraceValue::Off), (TraceValue::Off, TraceValue::Messages)]
    {
        let (mut harness, entered, release) = pending_trace_harness().await;
        set_trace(&harness, before);
        let (request, _) = start_pending(&harness, entered).await;

        set_trace(&harness, during);
        harness.server().request::<TraceBarrierRequest>(()).await.unwrap();
        harness.probe().await;
        let barrier = trace("Server completed request `test/traceBarrier` successfully");
        let expected = Vec::from_iter((during == TraceValue::Messages).then_some(barrier));
        assert_eq!(harness.take_traces(), expected, "{before:?} -> {during:?}");

        release.send(()).expect("pending request should still be running");
        within("pending request", request).await.unwrap();
        harness.probe().await;
        assert!(harness.take_traces().is_empty(), "{before:?} -> {during:?}");
        shutdown(harness).await;
    }
}

#[tokio::test(flavor = "current_thread")]
async fn messages_trace_reports_cancelled_requests_as_errors_without_ids() {
    let (mut harness, entered, _release) = pending_trace_harness().await;
    set_trace(&harness, TraceValue::Messages);
    let (request, id) = start_pending(&harness, entered).await;

    harness.server().notify::<notif::Cancel>(CancelParams { id }).unwrap();
    assert_request_cancelled(within("pending request", request).await);
    harness.probe().await;

    assert_eq!(
        harness.take_traces(),
        [trace("Server completed request `test/pendingTrace` with an error")]
    );
    shutdown(harness).await;
}

#[tokio::test(flavor = "current_thread")]
async fn initialize_trace_level_applies_after_the_initialize_response() {
    for (level, verbose) in [(TraceValue::Messages, false), (TraceValue::Verbose, true)] {
        let mut harness = protocol_trace_harness();
        initialize(&harness, Some(level)).await;
        workspace_symbols(&harness).await;
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
        shutdown(harness).await;
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
    initialize(&harness, None).await;
    set_trace(&harness, TraceValue::Verbose);

    let params = json!({
        "uri": "file:///workspace/Secret.sol",
        "text": SOURCE_SECRET,
        "environment": { "API_TOKEN": ENV_SECRET },
        "query": QUERY_SECRET,
    });
    let error = response_error(harness.server().request::<SensitiveTraceRequest>(params).await);
    assert_eq!(error.code, ErrorCode::REQUEST_FAILED);
    assert_eq!(error.message, ERROR_MESSAGE_SECRET);
    assert_eq!(error.data, Some(json!({ "token": ERROR_DATA_SECRET })));
    let result = harness
        .server()
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
    shutdown(harness).await;
}
