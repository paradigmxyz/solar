use crate::{
    config::{Config, negotiate_capabilities},
    global_state::GlobalState,
    handlers,
    project_fixture::{FixtureMarker, ProjectFixture},
    symbols::SymbolTables,
    vfs::{Vfs, VfsPath},
    workspace::Workspace,
};
use arc_swap::ArcSwap;
use async_lsp::{
    ClientSocket, ErrorCode, LspService, MainLoop, ResponseError, ServerSocket, router::Router,
};
use crop::Rope;
use lsp_types::{
    DidChangeTextDocumentParams, DidChangeWatchedFilesParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DidSaveTextDocumentParams, FileChangeType, FileEvent,
    InitializeParams, InitializedParams, LogTraceParams, NumberOrString, Position, ProgressParams,
    ProgressParamsValue, PublishDiagnosticsParams, TextDocumentContentChangeEvent,
    TextDocumentIdentifier, TextDocumentItem, Url, VersionedTextDocumentIdentifier,
    WorkDoneProgress, WorkDoneProgressCreateParams, WorkDoneProgressEnd, WorkspaceFolder,
    notification as notif, request as req,
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::{
    fs,
    future::Future,
    io::Read,
    ops::ControlFlow,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{Arc, mpsc as std_mpsc},
    task::{Context, Poll, Waker},
    time::Duration,
};
use tempfile::TempDir;
use tokio::{
    io::{
        AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader,
        DuplexStream, ReadHalf, WriteHalf,
    },
    sync::{mpsc, oneshot},
    task::JoinHandle,
};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

/// How long tests wait for asynchronous server work.
pub(crate) const TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) fn spawn_lsp_pair<S, C>(
    server: MainLoop<S>,
    client: MainLoop<C>,
) -> (JoinHandle<async_lsp::Result<()>>, JoinHandle<async_lsp::Result<()>>)
where
    S: LspService<Response = Value, Error = ResponseError> + Send + 'static,
    S::Future: Send + 'static,
    C: LspService<Response = Value, Error = ResponseError> + Send + 'static,
    C::Future: Send + 'static,
{
    let (server_stream, client_stream) = tokio::io::duplex(64 << 10);
    let (server_rx, server_tx) = tokio::io::split(server_stream);
    let server_task =
        tokio::spawn(server.run_buffered(server_rx.compat(), server_tx.compat_write()));
    let (client_rx, client_tx) = tokio::io::split(client_stream);
    let client_task =
        tokio::spawn(client.run_buffered(client_rx.compat(), client_tx.compat_write()));
    (server_task, client_task)
}

pub(crate) fn assert_request_cancelled<T>(result: async_lsp::Result<T>) {
    let Err(error) = result else { panic!("expected request cancellation") };
    let async_lsp::Error::Response(error) = error else {
        panic!("expected request cancellation, got {error:?}");
    };
    assert_eq!(error.code, ErrorCode::REQUEST_CANCELLED);
}

pub(crate) fn start_request<F: Future>(future: F) -> Pin<Box<F>> {
    let mut future = Box::pin(future);
    let mut cx = Context::from_waker(Waker::noop());
    assert!(future.as_mut().poll(&mut cx).is_pending());
    future
}

/// Polls `future` once and returns its output, which must already be ready.
#[track_caller]
pub(crate) fn expect_ready<F: Future>(future: F) -> F::Output {
    let mut cx = Context::from_waker(Waker::noop());
    let Poll::Ready(output) = std::pin::pin!(future).poll(&mut cx) else {
        panic!("future should complete immediately");
    };
    output
}

#[track_caller]
pub(crate) fn assert_polls(pending: bool, future: impl Future) {
    let mut cx = Context::from_waker(Waker::noop());
    assert_eq!(std::pin::pin!(future).poll(&mut cx).is_pending(), pending);
}

/// Runs `future` to completion on a fresh current-thread runtime.
pub(crate) fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(future)
}

/// A current-thread runtime with a single blocking worker.
pub(crate) fn single_blocking_worker_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .unwrap()
}

/// Occupies a blocking worker until the returned sender releases it.
pub(crate) fn pause_blocking_pool() -> (std_mpsc::Sender<()>, JoinHandle<()>) {
    let (started_tx, started_rx) = std_mpsc::channel();
    let (release_tx, release_rx) = std_mpsc::channel();
    let task = tokio::task::spawn_blocking(move || {
        started_tx.send(()).unwrap();
        release_rx.recv().unwrap();
    });
    started_rx.recv_timeout(TIMEOUT).expect("blocking worker should start");
    (release_tx, task)
}

/// Runs `test` while the runtime's only blocking worker stays busy until `test` releases it.
pub(crate) fn with_paused_blocking_pool<F: Future>(
    test: impl FnOnce(std_mpsc::Sender<()>) -> F,
) -> F::Output {
    single_blocking_worker_runtime().block_on(async {
        let (release, worker) = pause_blocking_pool();
        let (output, ()) = tokio::join!(test(release), async { worker.await.unwrap() });
        output
    })
}

/// Awaits `future`, panicking with `what` if it takes longer than [`TIMEOUT`].
pub(crate) async fn within<F: Future>(what: &str, future: F) -> F::Output {
    tokio::time::timeout(TIMEOUT, future)
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for {what}"))
}

pub(crate) fn from_json<T: DeserializeOwned>(value: Value) -> T {
    serde_json::from_value(value).unwrap()
}

/// Builds request params for `uri` at `position` over the whole document, extended with `extra`.
pub(crate) fn request_params<T: DeserializeOwned>(
    uri: &Url,
    position: Position,
    extra: Value,
) -> T {
    let mut params = json!({
        "textDocument": { "uri": uri },
        "position": position,
        "range": {
            "start": { "line": 0, "character": 0 },
            "end": { "line": u32::MAX, "character": u32::MAX },
        },
    });
    let Value::Object(extra) = extra else { panic!("extra params must be an object") };
    params.as_object_mut().unwrap().extend(extra);
    from_json(params)
}

/// Builds request params for `uri` at the start of the document.
pub(crate) fn document_params<T: DeserializeOwned>(uri: &Url) -> T {
    request_params(uri, Position::default(), json!({}))
}

/// Negotiates `params` and discovers the workspaces of the resulting config.
pub(crate) fn rediscovered_config(params: InitializeParams) -> Config {
    let (_, mut config) = negotiate_capabilities(params);
    config.rediscover_workspaces();
    config
}

/// Returns `params` with initialization `options`.
pub(crate) fn with_options(mut params: InitializeParams, options: Value) -> InitializeParams {
    params.initialization_options = Some(options);
    params
}

/// Returns `params` with the given JSON client capabilities.
pub(crate) fn with_capabilities(
    mut params: InitializeParams,
    capabilities: Value,
) -> InitializeParams {
    params.capabilities = from_json(capabilities);
    params
}

/// Returns `params` with dynamic watched-file registration that supports relative patterns.
pub(crate) fn with_relative_watchers(params: InitializeParams) -> InitializeParams {
    let watched_files = json!({ "dynamicRegistration": true, "relativePatternSupport": true });
    with_capabilities(params, json!({ "workspace": { "didChangeWatchedFiles": watched_files } }))
}

pub(crate) fn workspace_at<'a>(config: &'a Config, root: &Path) -> &'a Workspace {
    config
        .workspaces()
        .iter()
        .find(|workspace| workspace.compile_opts().base_path.as_deref() == Some(root))
        .unwrap_or_else(|| panic!("expected a workspace at `{}`", root.display()))
}

/// A state without a client connection that uses `config`.
pub(crate) fn state_with(config: Config) -> GlobalState {
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(config);
    state
}

/// Waits for the latest scheduled analysis to publish.
pub(crate) async fn settle(state: &GlobalState) -> Arc<ArcSwap<SymbolTables>> {
    within("analysis", state.latest_analysis()).await.unwrap()
}

pub(crate) fn open(state: &mut GlobalState, uri: &Url, version: i32, text: impl Into<String>) {
    let text_document = TextDocumentItem::new(uri.clone(), "solidity".into(), version, text.into());
    let params = DidOpenTextDocumentParams { text_document };
    assert!(handlers::did_open_text_document(state, params).is_continue());
}

/// Replaces the whole text of an open document.
pub(crate) fn change(state: &mut GlobalState, uri: &Url, version: i32, text: impl Into<String>) {
    let change =
        TextDocumentContentChangeEvent { range: None, range_length: None, text: text.into() };
    edit(state, uri, version, vec![change]);
}

pub(crate) fn edit(
    state: &mut GlobalState,
    uri: &Url,
    version: i32,
    content_changes: Vec<TextDocumentContentChangeEvent>,
) {
    let text_document = VersionedTextDocumentIdentifier::new(uri.clone(), version);
    let params = DidChangeTextDocumentParams { text_document, content_changes };
    assert!(handlers::did_change_text_document(state, params).is_continue());
}

pub(crate) fn close(state: &mut GlobalState, uri: &Url) {
    let params =
        DidCloseTextDocumentParams { text_document: TextDocumentIdentifier::new(uri.clone()) };
    assert!(handlers::did_close_text_document(state, params).is_continue());
}

pub(crate) fn save(state: &mut GlobalState, uri: &Url) {
    let params = DidSaveTextDocumentParams {
        text_document: TextDocumentIdentifier::new(uri.clone()),
        text: None,
    };
    assert!(handlers::did_save_text_document(state, params).is_continue());
}

pub(crate) fn watch_files(
    state: &mut GlobalState,
    events: impl IntoIterator<Item = (impl AsRef<Path>, FileChangeType)>,
) {
    let changes = events
        .into_iter()
        .map(|(path, typ)| FileEvent { uri: Url::from_file_path(path).unwrap(), typ })
        .collect();
    let params = DidChangeWatchedFilesParams { changes };
    assert!(handlers::did_change_watched_files(state, params).is_continue());
}

/// Sets an unsaved buffer for `path`.
pub(crate) fn set_overlay(
    state: &GlobalState,
    path: &Path,
    text: &str,
    version: impl Into<Option<i32>>,
) {
    state.vfs.write().set_file_contents_with_version(
        VfsPath::from(path.to_path_buf()),
        Some(Rope::from(text)),
        version.into(),
    );
}

/// Drops the unsaved buffer for `path`.
pub(crate) fn remove_overlay(state: &GlobalState, path: &Path) {
    state.vfs.write().set_file_contents(VfsPath::from(path.to_path_buf()), None);
}

/// A router that stops its main loop on `exit`.
pub(crate) fn exit_router<S>(state: S) -> Router<S> {
    let mut router = Router::new(state);
    router.notification::<notif::Exit>(|_, ()| ControlFlow::Break(Ok(())));
    router
}

/// A client that ignores published diagnostics and log messages.
pub(crate) fn quiet_client() -> Router<()> {
    let mut router = Router::new(());
    router.notification::<notif::PublishDiagnostics>(|_, _| ControlFlow::Continue(()));
    router.notification::<notif::LogMessage>(|_, _| ControlFlow::Continue(()));
    router
}

/// A server and a client main loop connected in memory.
pub(crate) struct LspPair {
    /// Sends messages to the client main loop.
    pub(crate) client: ClientSocket,
    /// Sends messages to the server main loop.
    pub(crate) server: ServerSocket,
    server_task: JoinHandle<async_lsp::Result<()>>,
    client_task: JoinHandle<async_lsp::Result<()>>,
}

impl LspPair {
    pub(crate) fn spawn<S, C>(
        server: impl FnOnce(ClientSocket) -> S,
        client: impl FnOnce(ServerSocket) -> C,
    ) -> Self
    where
        S: LspService<Response = Value, Error = ResponseError> + Send + 'static,
        S::Future: Send + 'static,
        C: LspService<Response = Value, Error = ResponseError> + Send + 'static,
        C::Future: Send + 'static,
    {
        let (server_main, client_socket) = MainLoop::new_server(server);
        let (client_main, server) = MainLoop::new_client(client);
        let (server_task, client_task) = spawn_lsp_pair(server_main, client_main);
        Self { client: client_socket, server, server_task, client_task }
    }

    pub(crate) async fn initialize(&self, params: InitializeParams) {
        self.server.request::<req::Initialize>(params).await.unwrap();
        self.server.notify::<notif::Initialized>(InitializedParams {}).unwrap();
    }

    /// Sends `shutdown`, then exits.
    pub(crate) async fn shutdown(self) {
        self.server.request::<req::Shutdown>(()).await.unwrap();
        self.exit().await;
    }

    /// Sends `exit` and expects the server to stop cleanly and the client to see the stream end.
    pub(crate) async fn exit(self) {
        self.server.notify::<notif::Exit>(()).unwrap();
        assert!(within("server exit", self.server_task).await.unwrap().is_ok());
        assert!(matches!(self.client_task.await.unwrap(), Err(async_lsp::Error::Eof)));
    }
}

/// A server message observed by [`ClientHarness`].
#[derive(Debug, PartialEq)]
pub(crate) enum ClientEvent {
    Create(WorkDoneProgressCreateParams),
    Progress(ProgressParams),
    Diagnostics(PublishDiagnosticsParams),
    Trace(LogTraceParams),
    DiagnosticRefresh,
    InlayHintRefresh,
    CodeLensRefresh,
}

struct ClientState {
    events: mpsc::UnboundedSender<ClientEvent>,
    create_acks: mpsc::UnboundedSender<oneshot::Sender<()>>,
    created: bool,
}

impl ClientState {
    fn record(&self, event: ClientEvent) -> ControlFlow<async_lsp::Result<()>> {
        self.events.send(event).unwrap();
        ControlFlow::Continue(())
    }

    fn refresh(
        &mut self,
        event: ClientEvent,
    ) -> impl Future<Output = Result<(), ResponseError>> + use<> {
        self.events.send(event).unwrap();
        async { Ok(()) }
    }
}

/// A client that records server messages and answers progress creation only when acknowledged.
///
/// The client accepts at most one progress creation; a second one fails the client loop, which
/// `shutdown` and `exit` report.
pub(crate) struct ClientHarness {
    pair: LspPair,
    events: mpsc::UnboundedReceiver<ClientEvent>,
    create_acks: mpsc::UnboundedReceiver<oneshot::Sender<()>>,
}

impl ClientHarness {
    /// Connects the recording client to a server that only handles `exit`.
    pub(crate) fn new() -> Self {
        Self::with_server(|_| exit_router(()))
    }

    pub(crate) fn with_server<S>(server: impl FnOnce(ClientSocket) -> S) -> Self
    where
        S: LspService<Response = Value, Error = ResponseError> + Send + 'static,
        S::Future: Send + 'static,
    {
        let (events_tx, events) = mpsc::unbounded_channel();
        let (create_acks_tx, create_acks) = mpsc::unbounded_channel();
        let pair = LspPair::spawn(server, move |_| {
            let mut router = Router::new(ClientState {
                events: events_tx,
                create_acks: create_acks_tx,
                created: false,
            });
            router.request::<req::WorkDoneProgressCreate, _>(|state, params| {
                assert!(!std::mem::replace(&mut state.created, true), "second progress creation");
                let (ack, acked) = oneshot::channel();
                state.create_acks.send(ack).unwrap();
                state.events.send(ClientEvent::Create(params)).unwrap();
                async move {
                    acked.await.map_err(|_| {
                        ResponseError::new(ErrorCode::REQUEST_FAILED, "test create ack dropped")
                    })
                }
            });
            router.request::<req::Shutdown, _>(|_, ()| async { Ok(()) });
            router.request::<req::WorkspaceDiagnosticRefresh, _>(|state, ()| {
                state.refresh(ClientEvent::DiagnosticRefresh)
            });
            router.request::<req::InlayHintRefreshRequest, _>(|state, ()| {
                state.refresh(ClientEvent::InlayHintRefresh)
            });
            router.request::<req::CodeLensRefresh, _>(|state, ()| {
                state.refresh(ClientEvent::CodeLensRefresh)
            });
            router.notification::<notif::Progress>(|state, params| {
                state.record(ClientEvent::Progress(params))
            });
            router.notification::<notif::PublishDiagnostics>(|state, params| {
                state.record(ClientEvent::Diagnostics(params))
            });
            router.notification::<notif::LogTrace>(|state, params| {
                state.record(ClientEvent::Trace(params))
            });
            router.notification::<notif::LogMessage>(|_, _| ControlFlow::Continue(()));
            router
        });
        Self { pair, events, create_acks }
    }

    /// Sends messages to the client.
    pub(crate) fn client(&self) -> &ClientSocket {
        &self.pair.client
    }

    /// Sends messages to the server.
    pub(crate) fn server(&self) -> &ServerSocket {
        &self.pair.server
    }

    /// A state that uses `config` and sends server messages to this client.
    pub(crate) fn state(&self, config: Config) -> GlobalState {
        let mut state = GlobalState::new(self.pair.client.clone());
        state.config = Arc::new(config);
        state
    }

    pub(crate) async fn initialize(&self, params: InitializeParams) {
        self.pair.initialize(params).await;
    }

    pub(crate) async fn next_event(&mut self) -> ClientEvent {
        within("client event", self.events.recv()).await.expect("client event channel is open")
    }

    pub(crate) async fn next_published(&mut self) -> PublishDiagnosticsParams {
        match self.next_event().await {
            ClientEvent::Diagnostics(params) => params,
            event => panic!("expected published diagnostics, got {event:?}"),
        }
    }

    pub(crate) async fn expect_no_event(&mut self) {
        if let Ok(event) =
            tokio::time::timeout(Duration::from_millis(100), self.events.recv()).await
        {
            panic!("unexpected client event: {event:?}");
        }
    }

    /// Expects exactly the given pull-result refresh requests, in any order.
    pub(crate) async fn expect_refreshes(&mut self, diagnostics: bool, inlay_hints: bool) {
        let mut expected = Vec::new();
        expected.extend(diagnostics.then_some(ClientEvent::DiagnosticRefresh));
        expected.extend(inlay_hints.then_some(ClientEvent::InlayHintRefresh));
        for _ in 0..expected.len() {
            let event = self.next_event().await;
            let idx = expected.iter().position(|expected| *expected == event);
            expected.swap_remove(idx.unwrap_or_else(|| panic!("unexpected event {event:?}")));
        }
        self.expect_no_event().await;
    }

    pub(crate) fn acknowledge_create(&mut self) {
        self.create_acks.try_recv().expect("pending progress creation").send(()).unwrap();
    }

    pub(crate) async fn expect_create(&mut self) -> NumberOrString {
        match self.next_event().await {
            ClientEvent::Create(create) => create.token,
            event => panic!("expected progress creation, got {event:?}"),
        }
    }

    pub(crate) async fn expect_progress(&mut self, token: &NumberOrString) -> WorkDoneProgress {
        match self.next_event().await {
            ClientEvent::Progress(ProgressParams {
                token: actual,
                value: ProgressParamsValue::WorkDone(value),
            }) if actual == *token => value,
            event => panic!("expected progress for {token:?}, got {event:?}"),
        }
    }

    /// Acknowledges the next progress creation and expects its `begin` with `message`.
    pub(crate) async fn expect_progress_begin(&mut self, message: Option<&str>) -> NumberOrString {
        let token = self.expect_create().await;
        self.acknowledge_create();
        let WorkDoneProgress::Begin(begin) = self.expect_progress(&token).await else {
            panic!("expected progress begin");
        };
        assert_eq!(begin.message.as_deref(), message);
        token
    }

    pub(crate) async fn expect_progress_end(&mut self, token: &NumberOrString, message: &str) {
        let end = WorkDoneProgressEnd { message: Some(message.into()) };
        assert_eq!(self.expect_progress(token).await, WorkDoneProgress::End(end));
    }

    /// Round-trips a request so the client has handled every earlier server message.
    pub(crate) async fn probe(&self) {
        self.pair.client.request::<req::Shutdown>(()).await.unwrap();
    }

    /// Expects that the client has received nothing new, after handling earlier messages.
    pub(crate) async fn assert_silent(&mut self) {
        self.probe().await;
        assert!(matches!(self.events.try_recv(), Err(mpsc::error::TryRecvError::Empty)));
    }

    /// Returns the traces received so far, dropping other events.
    pub(crate) fn take_traces(&mut self) -> Vec<LogTraceParams> {
        let mut traces = Vec::new();
        while let Ok(event) = self.events.try_recv() {
            if let ClientEvent::Trace(trace) = event {
                traces.push(trace);
            }
        }
        traces
    }

    pub(crate) async fn shutdown(self) {
        self.pair.shutdown().await;
    }

    pub(crate) async fn exit(self) {
        self.pair.exit().await;
    }
}

/// A server connected to a raw LSP stream, for checking messages on the wire.
pub(crate) struct WireServer {
    reader: BufReader<ReadHalf<DuplexStream>>,
    writer: WriteHalf<DuplexStream>,
    server_task: JoinHandle<async_lsp::Result<()>>,
    _client: ClientSocket,
}

impl WireServer {
    pub(crate) fn spawn<S>(server: impl FnOnce(ClientSocket) -> S) -> Self
    where
        S: LspService<Response = Value, Error = ResponseError> + Send + 'static,
        S::Future: Send + 'static,
    {
        let (main_loop, client) = MainLoop::new_server(server);
        let (server_stream, client_stream) = tokio::io::duplex(64 << 10);
        let (server_reader, server_writer) = tokio::io::split(server_stream);
        let server_task = tokio::spawn(
            main_loop.run_buffered(server_reader.compat(), server_writer.compat_write()),
        );
        let (reader, writer) = tokio::io::split(client_stream);
        Self { reader: BufReader::new(reader), writer, server_task, _client: client }
    }

    pub(crate) async fn send(&mut self, message: Value) {
        write_lsp_frame(&mut self.writer, message).await;
    }

    pub(crate) async fn notify(&mut self, method: &str, params: Value) {
        self.send(json!({ "jsonrpc": "2.0", "method": method, "params": params })).await;
    }

    pub(crate) async fn recv(&mut self) -> Value {
        within("server message", read_lsp_frame(&mut self.reader)).await
    }

    /// Sends `exit`, closes the stream, and expects the server loop to stop cleanly.
    pub(crate) async fn exit(mut self) {
        self.notify("exit", Value::Null).await;
        self.writer.shutdown().await.unwrap();
        let result = within("server exit", self.server_task).await.expect("server task panicked");
        assert!(result.is_ok(), "server loop failed after `exit`: {result:?}");
    }
}

async fn write_lsp_frame(writer: &mut (impl AsyncWrite + Unpin), message: Value) {
    let body = serde_json::to_vec(&message).unwrap();
    writer.write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes()).await.unwrap();
    writer.write_all(&body).await.unwrap();
    writer.flush().await.unwrap();
}

async fn read_lsp_frame(reader: &mut (impl AsyncBufRead + Unpin)) -> Value {
    let mut line = String::new();
    let mut content_length = None;
    loop {
        line.clear();
        assert_ne!(reader.read_line(&mut line).await.unwrap(), 0, "unexpected end of LSP stream");
        if line == "\r\n" {
            break;
        }

        let (name, value) = line
            .strip_suffix("\r\n")
            .and_then(|line| line.split_once(": "))
            .unwrap_or_else(|| panic!("invalid LSP header: {line:?}"));
        if name.eq_ignore_ascii_case("Content-Length") {
            content_length = Some(
                value
                    .parse::<usize>()
                    .unwrap_or_else(|_| panic!("invalid LSP content length: {value}")),
            );
        }
    }

    let mut body = vec![0; content_length.expect("LSP frame should have a content length")];
    reader.read_exact(&mut body).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

#[cfg(unix)]
pub(crate) fn process_exists(pid: u32) -> bool {
    std::process::Command::new("ps")
        .args(["-p", &pid.to_string()])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

pub(crate) struct TestProject {
    tmp: TempDir,
    open_files: Vec<(PathBuf, String)>,
}

pub(crate) struct MarkedProject {
    project: TestProject,
    fixture: ProjectFixture,
}

impl TestProject {
    pub(crate) fn new() -> Self {
        Self { tmp: TempDir::new().unwrap(), open_files: Vec::new() }
    }

    pub(crate) fn from_fixture(fixture: &str) -> Self {
        Self::from_project_fixture(&ProjectFixture::parse_without_markers(fixture))
    }

    fn from_project_fixture(fixture: &ProjectFixture) -> Self {
        let mut project = Self::new();
        for file in fixture.files() {
            project.write_file(file.path(), file.text());
            if file.is_open() {
                project.open_file(file.path(), file.text());
            }
        }
        project
    }

    pub(crate) fn root(&self) -> &Path {
        self.tmp.path()
    }

    pub(crate) fn path(&self, path: &str) -> PathBuf {
        self.tmp.path().join(path.strip_prefix('/').unwrap_or(path))
    }

    pub(crate) fn uri(&self, path: &str) -> Url {
        Url::from_file_path(self.path(path)).unwrap()
    }

    pub(crate) fn write_file(&self, path: &str, contents: &str) {
        let path = self.path(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    pub(crate) fn read_file(&self, path: &str) -> String {
        let mut contents = String::new();
        fs::File::open(self.path(path)).unwrap().read_to_string(&mut contents).unwrap();
        contents
    }

    pub(crate) fn open_file(&mut self, path: &str, contents: &str) {
        let path = self.path(path);
        if let Some((_, existing)) =
            self.open_files.iter_mut().find(|(candidate, _)| candidate == &path)
        {
            existing.clear();
            existing.push_str(contents);
        } else {
            self.open_files.push((path, contents.to_string()));
        }
    }

    pub(crate) fn remove_file(&self, path: &str) {
        fs::remove_file(self.path(path)).unwrap();
    }

    pub(crate) fn initialize_params(&self) -> InitializeParams {
        self.initialize_params_with_roots(&["/"])
    }

    pub(crate) fn initialize_params_with_roots(&self, roots: &[&str]) -> InitializeParams {
        InitializeParams {
            workspace_folders: Some(
                roots
                    .iter()
                    .map(|root| {
                        let path = self.path(root);
                        WorkspaceFolder {
                            uri: Url::from_file_path(&path).unwrap(),
                            name: path
                                .file_name()
                                .and_then(|name| name.to_str())
                                .unwrap_or("root")
                                .into(),
                        }
                    })
                    .collect(),
            ),
            ..Default::default()
        }
    }

    pub(crate) fn config(&self) -> Config {
        rediscovered_config(self.initialize_params())
    }

    pub(crate) fn config_with_roots(&self, roots: &[&str]) -> Config {
        rediscovered_config(self.initialize_params_with_roots(roots))
    }

    /// A state without a client connection that uses this project's config and open files.
    pub(crate) fn state(&self) -> GlobalState {
        let state = state_with(self.config());
        *state.vfs.write() = self.vfs();
        state
    }

    pub(crate) fn vfs(&self) -> Vfs {
        let mut vfs = Vfs::default();
        for (path, contents) in &self.open_files {
            vfs.set_file_contents_with_version(
                VfsPath::from(path.clone()),
                Some(Rope::from(contents.as_str())),
                Some(0),
            );
        }
        vfs
    }
}

impl MarkedProject {
    pub(crate) fn from_fixture(fixture: &str) -> Self {
        let fixture = ProjectFixture::parse(fixture);
        let project = TestProject::from_project_fixture(&fixture);
        Self { project, fixture }
    }

    pub(crate) fn project(&self) -> &TestProject {
        &self.project
    }

    pub(crate) fn project_mut(&mut self) -> &mut TestProject {
        &mut self.project
    }

    pub(crate) fn marker(&self, name: &str) -> &FixtureMarker {
        self.fixture.marker(name)
    }

    /// Returns the document and position of a marker.
    pub(crate) fn location(&self, name: &str) -> (Url, Position) {
        let marker = self.marker(name);
        (self.project.uri(marker.path()), marker.position())
    }
}
