use super::*;
use crate::test_support::{
    MarkedProject, TestProject, assert_request_cancelled, spawn_lsp_pair, start_request,
};
use ::serde::de::DeserializeOwned;
use async_lsp::{
    AnyEvent, AnyNotification, AnyRequest, ErrorCode, LanguageServer, LspService, ResponseError,
    ServerSocket, router::Router,
};
use lsp_types::{
    CancelParams, CompletionResponse, DidChangeWorkspaceFoldersParams, FileChangeType,
    InitializeParams, InitializedParams, NumberOrString, ProgressParams, ProgressParamsValue,
    PublishDiagnosticsParams, RegistrationParams, SymbolKind, TextDocumentSaveReason,
    UnregistrationParams, WindowClientCapabilities, WorkDoneProgress, WorkDoneProgressCancelParams,
    WorkDoneProgressCreateParams, WorkDoneProgressEnd, WorkDoneProgressReport, WorkspaceFolder,
    WorkspaceFoldersChangeEvent, WorkspaceSymbolParams, notification as notif,
    notification::Notification, request, request::Request,
};
use serde_json::{Value, json};
use solar_interface::data_structures::sync::RwLock;
use std::{
    ops::ControlFlow,
    path::Path,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};
use tokio::sync::{mpsc, oneshot};
use tower::Service;

const TIMEOUT: Duration = Duration::from_secs(2);

fn new_router(client: ClientSocket) -> Router<GlobalState> {
    new_router_with_state(GlobalState::new(client))
}

fn from_json<T: DeserializeOwned>(value: Value) -> T {
    serde_json::from_value(value).unwrap()
}

struct Session {
    server: ServerSocket,
    server_task: tokio::task::JoinHandle<async_lsp::Result<()>>,
    client_task: tokio::task::JoinHandle<async_lsp::Result<()>>,
}

impl Session {
    fn spawn<S, C>(
        server: impl FnOnce(ClientSocket) -> S,
        client: impl FnOnce(ServerSocket) -> C,
    ) -> Self
    where
        S: LspService<Response = Value, Error = ResponseError> + Send + 'static,
        S::Future: Send + 'static,
        C: LspService<Response = Value, Error = ResponseError> + Send + 'static,
        C::Future: Send + 'static,
    {
        let (server_main, _client) = async_lsp::MainLoop::new_server(server);
        let (client_main, server) = async_lsp::MainLoop::new_client(client);
        let (server_task, client_task) = spawn_lsp_pair(server_main, client_main);
        Self { server, server_task, client_task }
    }

    async fn initialize(&mut self, params: InitializeParams) {
        self.server.initialize(params).await.unwrap();
        self.server.initialized(InitializedParams {}).unwrap();
    }

    async fn shutdown(mut self) {
        self.server.shutdown(()).await.unwrap();
        self.server.exit(()).unwrap();
        assert!(self.server_task.await.unwrap().is_ok());
        assert!(matches!(self.client_task.await.unwrap(), Err(async_lsp::Error::Eof)));
    }
}

struct ObservedRouter {
    inner: Router<GlobalState>,
    accepted: mpsc::UnboundedSender<String>,
}

impl Service<AnyRequest> for ObservedRouter {
    type Response = Value;
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
    tokio::time::timeout(TIMEOUT, events.recv())
        .await
        .expect("analysis client event should arrive")
        .expect("analysis client event channel should stay open")
}

async fn next_progress(
    events: &mut mpsc::UnboundedReceiver<AnalysisClientEvent>,
    token: &NumberOrString,
) -> WorkDoneProgress {
    match next_analysis_event(events).await {
        AnalysisClientEvent::Progress(ProgressParams {
            token: actual,
            value: ProgressParamsValue::WorkDone(value),
        }) if actual == *token => value,
        event => panic!("expected progress for {token:?}, got {event:?}"),
    }
}

#[derive(Debug)]
enum WatchedRegistrationClientEvent {
    Register(RegistrationParams, oneshot::Sender<()>),
    Unregister(UnregistrationParams, oneshot::Sender<()>),
}

/// Forwards watched-file (un)registrations and holds each response until the test acknowledges it.
fn watched_registration_client(
    events: mpsc::UnboundedSender<WatchedRegistrationClientEvent>,
) -> Router<mpsc::UnboundedSender<WatchedRegistrationClientEvent>> {
    let mut router = Router::new(events);
    router.request::<request::RegisterCapability, _>(|events, params| {
        let (acknowledge, acknowledged) = oneshot::channel();
        events.send(WatchedRegistrationClientEvent::Register(params, acknowledge)).unwrap();
        async move {
            acknowledged.await.unwrap();
            Ok(())
        }
    });
    router.request::<request::UnregisterCapability, _>(|events, params| {
        let (acknowledge, acknowledged) = oneshot::channel();
        events.send(WatchedRegistrationClientEvent::Unregister(params, acknowledge)).unwrap();
        async move {
            acknowledged.await.unwrap();
            Ok(())
        }
    });
    router.notification::<notif::LogMessage>(|_, _| ControlFlow::Continue(()));
    router
}

fn watched_initialize_params(project: &TestProject, root: &str) -> InitializeParams {
    let mut params = project.initialize_params_with_roots(&[root]);
    params.capabilities.workspace = Some(from_json(json!({
        "didChangeWatchedFiles": { "dynamicRegistration": true, "relativePatternSupport": true },
    })));
    params
}

async fn next_watched_registration_event(
    events: &mut mpsc::UnboundedReceiver<WatchedRegistrationClientEvent>,
) -> WatchedRegistrationClientEvent {
    tokio::time::timeout(TIMEOUT, events.recv())
        .await
        .expect("watched-file registration request should arrive")
        .expect("watched-file registration event channel should stay open")
}

/// Acknowledges other watched-file traffic until a registration covers the discovered `root`.
async fn next_registration_for_root(
    events: &mut mpsc::UnboundedReceiver<WatchedRegistrationClientEvent>,
    root: &Path,
) -> (RegistrationParams, oneshot::Sender<()>) {
    loop {
        match next_watched_registration_event(events).await {
            WatchedRegistrationClientEvent::Register(params, acknowledge)
                if watched_registration_has_discovered_root(&params, root) =>
            {
                return (params, acknowledge);
            }
            WatchedRegistrationClientEvent::Register(params, acknowledge) => {
                watched_registration_id(&params);
                acknowledge.send(()).unwrap();
            }
            WatchedRegistrationClientEvent::Unregister(params, acknowledge) => {
                watched_unregistration_id(&params);
                acknowledge.send(()).unwrap();
            }
        }
    }
}

fn watched_registration_watchers(params: &RegistrationParams) -> &[Value] {
    let [registration] = params.registrations.as_slice() else {
        panic!("expected one watched-file registration, got {params:?}")
    };
    assert_eq!(registration.method, notif::DidChangeWatchedFiles::METHOD);
    registration.register_options.as_ref().unwrap()["watchers"].as_array().unwrap()
}

fn watched_registration_pattern_count(
    params: &RegistrationParams,
    root: &Path,
    pattern: &str,
) -> usize {
    let root_uri = lsp_types::Url::from_file_path(root).unwrap().to_string();
    watched_registration_watchers(params)
        .iter()
        .filter(|watcher| {
            watcher["globPattern"]["baseUri"].as_str() == Some(&root_uri)
                && watcher["globPattern"]["pattern"] == pattern
        })
        .count()
}

fn watched_registration_has_discovered_root(params: &RegistrationParams, root: &Path) -> bool {
    ["foundry.toml", "remappings.txt", "*", "*.sol"]
        .into_iter()
        .all(|pattern| watched_registration_pattern_count(params, root, pattern) == 1)
}

fn assert_watched_registration_root(params: &RegistrationParams, root: &Path) {
    for pattern in ["foundry.toml", "remappings.txt"] {
        let matching_watchers = watched_registration_pattern_count(params, root, pattern);
        assert_eq!(matching_watchers, 1, "expected one `{pattern}` watcher for `{root:?}`");
    }
}

fn assert_watched_registration_excludes_root(params: &RegistrationParams, root: &Path) {
    let root_uri = lsp_types::Url::from_file_path(root).unwrap().to_string();
    assert!(
        watched_registration_watchers(params)
            .iter()
            .all(|watcher| watcher["globPattern"]["baseUri"].as_str() != Some(&root_uri)),
        "unexpected watchers for stale workspace root `{root_uri}`"
    );
}

fn watched_registration_id(params: &RegistrationParams) -> &str {
    let [registration] = params.registrations.as_slice() else {
        panic!("expected one watched-file registration, got {params:?}")
    };
    assert!(registration.id.starts_with("solar-watched-files-"));
    assert_eq!(registration.method, notif::DidChangeWatchedFiles::METHOD);
    &registration.id
}

fn watched_unregistration_id(params: &UnregistrationParams) -> &str {
    let [unregistration] = params.unregisterations.as_slice() else {
        panic!("expected one watched-file unregistration, got {params:?}")
    };
    assert!(unregistration.id.starts_with("solar-watched-files-"));
    assert_eq!(unregistration.method, notif::DidChangeWatchedFiles::METHOD);
    &unregistration.id
}

#[tokio::test(flavor = "current_thread")]
async fn router_dispatches_requests_and_notifications() {
    let project = TestProject::from_fixture("//- /Test.sol open\n");
    let state = GlobalState::new(ClientSocket::new_closed());
    *state.vfs.write() = project.vfs();
    let mut router = new_router_with_state(state);
    let uri = "file:///workspace/src/Test.sol";
    let open_uri = lsp_types::Url::from_file_path(project.path("/Test.sol")).unwrap();
    let text_document = json!({ "uri": uri });
    let position = json!({ "line": 0, "character": 0 });
    let range = json!({ "start": position, "end": { "line": 0, "character": 1 } });
    let at_start = json!({ "textDocument": text_document, "position": position });
    let item = |uri: &str, kind| {
        let item = json!({
            "name": "C",
            "kind": kind,
            "uri": uri,
            "range": range,
            "selectionRange": range,
        });
        json!({ "item": item })
    };

    for (method, params) in [
        (
            notif::DidChangeWatchedFiles::METHOD,
            json!({ "changes": [{ "uri": uri, "type": FileChangeType::CHANGED }] }),
        ),
        (notif::DidCreateFiles::METHOD, json!({ "files": [] })),
        (notif::DidRenameFiles::METHOD, json!({ "files": [] })),
        (notif::DidDeleteFiles::METHOD, json!({ "files": [] })),
        (notif::DidSaveTextDocument::METHOD, json!({ "textDocument": text_document })),
        (
            notif::WillSaveTextDocument::METHOD,
            json!({ "textDocument": text_document, "reason": TextDocumentSaveReason::MANUAL }),
        ),
        (notif::WorkDoneProgressCancel::METHOD, json!({ "token": "solar/workspace-index/1" })),
    ] {
        let notification =
            from_json::<AnyNotification>(json!({ "method": method, "params": params }));
        assert!(router.notify(notification).is_continue(), "notification `{method}`");
    }

    let diagnostic_uri = json!({ "uri": "untitled:Diagnostics.sol" });
    let method = request::DocumentDiagnosticRequest::METHOD;
    let request =
        json!({ "id": 0, "method": method, "params": { "textDocument": diagnostic_uri } });
    let response = router.call(from_json(request)).await.unwrap();
    assert_eq!(response["kind"], "full");
    assert_eq!(response["items"], json!([]));
    let result_id = response["resultId"].as_str().expect("full report should have a result ID");
    let params_with_id = json!({ "textDocument": diagnostic_uri, "previousResultId": result_id });
    let request = json!({ "id": 1, "method": method, "params": params_with_id });
    let response = router.call(from_json(request)).await.unwrap();
    assert_eq!(response, json!({ "kind": "unchanged", "resultId": result_id }));

    let untitled = "untitled:Test.sol";
    let formatting = json!({
        "textDocument": { "uri": "file:///missing/Test.sol" },
        "options": { "tabSize": 0, "insertSpaces": false },
    });
    let requests = [
        (request::WillCreateFiles::METHOD, json!({ "files": [] }), Ok(Value::Null)),
        (request::WillRenameFiles::METHOD, json!({ "files": [] }), Ok(Value::Null)),
        (request::WillDeleteFiles::METHOD, json!({ "files": [] }), Ok(Value::Null)),
        (
            request::DocumentLinkRequest::METHOD,
            json!({ "textDocument": text_document }),
            Ok(json!([])),
        ),
        (
            request::CodeActionRequest::METHOD,
            json!({
                "textDocument": text_document,
                "range": range,
                "context": { "diagnostics": [] },
            }),
            Ok(json!([])),
        ),
        (request::CodeLensRequest::METHOD, json!({ "textDocument": text_document }), Ok(json!([]))),
        (
            request::FoldingRangeRequest::METHOD,
            json!({ "textDocument": { "uri": open_uri } }),
            Ok(json!([])),
        ),
        (
            request::SelectionRangeRequest::METHOD,
            json!({ "textDocument": { "uri": open_uri }, "positions": [] }),
            Ok(json!([])),
        ),
        (
            request::WorkspaceDiagnosticRequest::METHOD,
            json!({ "previousResultIds": [] }),
            Ok(json!({ "items": [] })),
        ),
        (request::DocumentHighlightRequest::METHOD, at_start.clone(), Ok(Value::Null)),
        (
            request::TypeHierarchyPrepare::METHOD,
            json!({ "textDocument": { "uri": untitled }, "position": position }),
            Ok(Value::Null),
        ),
        (
            request::TypeHierarchySupertypes::METHOD,
            item(untitled, SymbolKind::CLASS),
            Ok(Value::Null),
        ),
        (
            request::TypeHierarchySubtypes::METHOD,
            item(untitled, SymbolKind::CLASS),
            Ok(Value::Null),
        ),
        (request::HoverRequest::METHOD, at_start.clone(), Ok(Value::Null)),
        (request::CallHierarchyPrepare::METHOD, at_start.clone(), Ok(Value::Null)),
        (
            request::CallHierarchyIncomingCalls::METHOD,
            item(uri, SymbolKind::FUNCTION),
            Ok(Value::Null),
        ),
        (
            request::CallHierarchyOutgoingCalls::METHOD,
            item(uri, SymbolKind::FUNCTION),
            Ok(Value::Null),
        ),
        (request::SignatureHelpRequest::METHOD, at_start.clone(), Ok(Value::Null)),
        (request::GotoImplementation::METHOD, at_start.clone(), Ok(Value::Null)),
        (request::GotoTypeDefinition::METHOD, at_start, Ok(Value::Null)),
        (request::Formatting::METHOD, formatting, Err(ErrorCode::REQUEST_FAILED)),
        (
            request::ExecuteCommand::METHOD,
            json!({ "command": "solar.unknown" }),
            Err(ErrorCode::INVALID_PARAMS),
        ),
        (
            request::ExecuteCommand::METHOD,
            json!({ "command": "solar.clearCache" }),
            Ok(json!({ "success": true })),
        ),
        (
            request::ExecuteCommand::METHOD,
            json!({ "command": "solar.reindex" }),
            Ok(json!({ "success": true })),
        ),
    ];
    for (id, (method, params, expected)) in requests.into_iter().enumerate() {
        let request = json!({ "id": id, "method": method, "params": params });
        let response = router.call(from_json(request)).await.map_err(|error| {
            assert!(!error.message.ends_with('.'), "`{method}` error: {}", error.message);
            error.code
        });
        assert_eq!(response, expected, "request `{method}`");
    }

    let initialize = json!({
        "id": 0,
        "method": request::Initialize::METHOD,
        "params": InitializeParams::default(),
    });
    let response = router.call(from_json(initialize)).await.unwrap();
    assert_eq!(response["capabilities"]["typeHierarchyProvider"], true);
    assert_eq!(response["capabilities"]["completionProvider"]["resolveProvider"], true);
    assert_eq!(response["capabilities"]["codeLensProvider"]["resolveProvider"], false);
    assert_eq!(response["capabilities"]["hoverProvider"], true);
    assert_eq!(response["serverInfo"]["name"], "solar");
}

#[tokio::test(flavor = "current_thread")]
async fn requests_use_one_identity_for_equivalent_file_uris() {
    async fn response<R: Request>(server: &ServerSocket, params: Value) -> Value {
        let response = tokio::time::timeout(TIMEOUT, server.request::<R>(from_json(params)))
            .await
            .unwrap_or_else(|_| panic!("{} should finish after analysis", R::METHOD))
            .unwrap();
        serde_json::to_value(response).unwrap()
    }

    let marked = MarkedProject::from_fixture(
        r#"
        //- /Token.sol
        contract Base {}
        contract $4Token is Base {
            uint value;
            function callee(uint input) internal pure returns (uint) { return input; }
            function $1caller() public view returns (uint) { return $2callee($3value); }
        }
        "#,
    );
    let project = marked.project();
    let text = project.read_file("/Token.sol");
    // The opened alias is the only copy of the source, so results prove its identity.
    project.remove_file("/Token.sol");
    let canonical = lsp_types::Url::from_file_path(project.path("/Token.sol")).unwrap();
    let prefix = canonical.as_str().strip_suffix("Token.sol").unwrap();
    let mut session = Session::spawn(new_router, |_| {
        let mut router = Router::new(());
        router.notification::<notif::PublishDiagnostics>(|_, _| ControlFlow::Continue(()));
        router.notification::<notif::LogMessage>(|_, _| ControlFlow::Continue(()));
        router
    });
    session.initialize(project.initialize_params()).await;
    let opened = format!("{prefix}nested%2F..%2FToken.sol");
    let text_document =
        json!({ "uri": opened, "languageId": "solidity", "version": 1, "text": text });
    session
        .server
        .notify::<notif::DidOpenTextDocument>(from_json(json!({ "textDocument": text_document })))
        .unwrap();

    let server = &session.server;
    for spelling in ["%54oken.sol", "/Token.sol", "nested%2F..%2FToken.sol"] {
        let alias = lsp_types::Url::parse(&format!("{prefix}{spelling}")).unwrap();
        macro_rules! equivalent_response {
            ($request:ty, $params:expr) => {{
                let mut params = $params;
                params["textDocument"] = json!({ "uri": canonical });
                let expected = response::<$request>(server, params.clone()).await;
                assert!(!expected.is_null(), "{} should return a result", <$request>::METHOD);
                if let Some(items) = expected.as_array() {
                    assert!(!items.is_empty(), "{} should return items", <$request>::METHOD);
                }
                params["textDocument"] = json!({ "uri": alias });
                let actual = response::<$request>(server, params).await;
                assert_eq!(actual, expected, "{} for {alias}", <$request>::METHOD);
                actual
            }};
        }
        let symbols = equivalent_response!(request::DocumentSymbolRequest, json!({}));
        assert!(symbols.as_array().unwrap().iter().any(|symbol| symbol["name"] == "Token"));
        let position = marked.marker("$2").position();
        equivalent_response!(request::HoverRequest, json!({ "position": position }));
        equivalent_response!(request::GotoDefinition, json!({ "position": position }));
        equivalent_response!(
            request::References,
            json!({ "position": position, "context": { "includeDeclaration": true } })
        );
        let argument = marked.marker("$3").position();
        let completions =
            equivalent_response!(request::Completion, json!({ "position": argument }));
        assert!(completions.as_array().unwrap().iter().any(|item| item["label"] == "value"));
        let signature =
            equivalent_response!(request::SignatureHelpRequest, json!({ "position": argument }));
        assert!(!signature["signatures"].as_array().unwrap().is_empty());

        let calls = equivalent_response!(
            request::CallHierarchyPrepare,
            json!({ "position": marked.marker("$1").position() })
        );
        let outgoing =
            response::<request::CallHierarchyOutgoingCalls>(server, json!({ "item": calls[0] }))
                .await;
        assert_eq!(outgoing[0]["to"]["name"], "callee");

        let types = equivalent_response!(
            request::TypeHierarchyPrepare,
            json!({ "position": marked.marker("$4").position() })
        );
        let supertypes =
            response::<request::TypeHierarchySupertypes>(server, json!({ "item": types[0] })).await;
        assert_eq!(supertypes[0]["name"], "Base");
    }

    session.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn pending_analysis_requests_do_not_block_completion_or_cancellation() {
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

    let session = Session::spawn(
        move |client| {
            let request_client = client.clone();
            let mut state = GlobalState::new(client);
            state.vfs = Arc::new(RwLock::new(vfs));
            state.config = Arc::new(config);
            state.mark_analysis_pending_for_test();
            let router =
                ObservedRouter { inner: new_router_with_state(state), accepted: accepted_tx };
            ServiceBuilder::new().layer(request_layer(request_client)).service(router)
        },
        |_| Router::new(()),
    );
    let server = &session.server;
    let text_document = json!({ "textDocument": { "uri": uri } });

    let document_symbols = start_request(
        server.request::<request::DocumentSymbolRequest>(from_json(text_document.clone())),
    );
    assert_eq!(
        tokio::time::timeout(TIMEOUT, accepted_rx.recv()).await.unwrap().unwrap(),
        request::DocumentSymbolRequest::METHOD
    );
    let document_links =
        start_request(server.request::<request::DocumentLinkRequest>(from_json(text_document)));
    assert_eq!(
        tokio::time::timeout(TIMEOUT, accepted_rx.recv()).await.unwrap().unwrap(),
        request::DocumentLinkRequest::METHOD
    );

    let completion_params =
        json!({ "textDocument": { "uri": uri }, "position": { "line": 0, "character": 3 } });
    let completion =
        start_request(server.request::<request::Completion>(from_json(completion_params)));
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

    assert_request_cancelled(tokio::time::timeout(TIMEOUT, document_symbols).await.unwrap());
    assert_request_cancelled(tokio::time::timeout(TIMEOUT, document_links).await.unwrap());
    session.shutdown().await;
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

        let (events_tx, mut events_rx) = mpsc::unbounded_channel();
        let mut session = Session::spawn(
            move |client| {
                let mut state = GlobalState::new(client);
                state.vfs = Arc::new(RwLock::new(vfs));
                new_router_with_state(state)
            },
            move |_| {
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
            },
        );
        session.initialize(initialize).await;
        let server = &session.server;
        let barrier = async |query: &str| {
            let params = WorkspaceSymbolParams { query: query.into(), ..Default::default() };
            server.request::<request::WorkspaceSymbolRequest>(params).await.unwrap();
        };
        let reindex = async || {
            let params = from_json(json!({ "command": commands::REINDEX }));
            let response = tokio::time::timeout(
                Duration::from_secs(1),
                server.request::<request::ExecuteCommand>(params),
            )
            .await
            .expect("reindex acknowledgement should not wait for analysis")
            .unwrap();
            assert_eq!(response, Some(json!({ "success": true })));
        };

        // Process and drain the automatic initialization reindex before testing cancellation.
        barrier("initialization barrier").await;
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

        reindex().await;
        let AnalysisClientEvent::Create(create) = next_analysis_event(&mut events_rx).await else {
            panic!("expected progress creation")
        };
        let token = create.token;
        let WorkDoneProgress::Begin(begin) = next_progress(&mut events_rx, &token).await else {
            panic!("expected progress begin")
        };
        assert_eq!(begin.title, "Indexing workspace");
        assert_eq!(begin.cancellable, Some(false));

        reindex().await;
        assert_eq!(
            next_progress(&mut events_rx, &token).await,
            WorkDoneProgress::Report(WorkDoneProgressReport {
                cancellable: Some(false),
                message: Some("Workspace changed, restarting analysis".into()),
                percentage: None,
            })
        );

        server
            .notify::<notif::WorkDoneProgressCancel>(WorkDoneProgressCancelParams {
                token: token.clone(),
            })
            .unwrap();
        barrier("cancel barrier").await;
        assert_eq!(
            next_progress(&mut events_rx, &token).await,
            WorkDoneProgress::End(WorkDoneProgressEnd { message: None })
        );

        release_blocker_tx.send(()).unwrap();
        blocker.await.unwrap();
        loop {
            match next_analysis_event(&mut events_rx).await {
                AnalysisClientEvent::Diagnostics(params)
                    if params.uri == broken_uri && !params.diagnostics.is_empty() =>
                {
                    break;
                }
                AnalysisClientEvent::Diagnostics(_) => {}
                event => panic!("progress continued after cancellation: {event:?}"),
            }
        }

        session.shutdown().await;
    });
}

#[tokio::test(flavor = "current_thread")]
async fn watched_file_reregistration_keeps_latest_workspace_folders() {
    let project = TestProject::new();
    let [initial_path, stale_path, latest_path] =
        ["/initial", "/stale", "/latest"].map(|path| project.path(path));
    for path in [&initial_path, &stale_path, &latest_path] {
        std::fs::create_dir(path).unwrap();
    }
    let workspace_folder = |path: &Path, name: &str| WorkspaceFolder {
        uri: lsp_types::Url::from_file_path(path).unwrap(),
        name: name.into(),
    };
    let initial = workspace_folder(&initial_path, "initial");
    let stale = workspace_folder(&stale_path, "stale");
    let latest = workspace_folder(&latest_path, "latest");
    let change_folders = |added, removed| DidChangeWorkspaceFoldersParams {
        event: WorkspaceFoldersChangeEvent { added: vec![added], removed: vec![removed] },
    };

    let (events_tx, mut events_rx) = mpsc::unbounded_channel();
    let mut session = Session::spawn(new_router, move |_| watched_registration_client(events_tx));
    session.initialize(watched_initialize_params(&project, "/initial")).await;

    let (params, acknowledge) = next_registration_for_root(&mut events_rx, &initial_path).await;
    watched_registration_id(&params);
    acknowledge.send(()).unwrap();

    let server = &session.server;
    server
        .notify::<notif::DidChangeWorkspaceFolders>(change_folders(stale.clone(), initial))
        .unwrap();
    let (stale_params, stale_register) =
        next_registration_for_root(&mut events_rx, &stale_path).await;
    watched_registration_id(&stale_params);

    server.notify::<notif::DidChangeWorkspaceFolders>(change_folders(latest, stale)).unwrap();
    stale_register.send(()).unwrap();
    let (params, acknowledge) = next_registration_for_root(&mut events_rx, &latest_path).await;
    assert_watched_registration_excludes_root(&params, &initial_path);
    assert_watched_registration_excludes_root(&params, &stale_path);
    let latest_id = watched_registration_id(&params).to_owned();
    acknowledge.send(()).unwrap();
    let WatchedRegistrationClientEvent::Unregister(params, acknowledge) =
        next_watched_registration_event(&mut events_rx).await
    else {
        panic!("expected superseded watched-file unregistration")
    };
    assert_ne!(watched_unregistration_id(&params), latest_id);
    acknowledge.send(()).unwrap();

    session.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn watched_file_reregistration_follows_workspace_root_file_operations() {
    async fn reregistration(
        events: &mut mpsc::UnboundedReceiver<WatchedRegistrationClientEvent>,
        old_id: Option<&str>,
    ) -> RegistrationParams {
        let WatchedRegistrationClientEvent::Register(params, acknowledge) =
            next_watched_registration_event(events).await
        else {
            panic!("expected watched-file registration")
        };
        acknowledge.send(()).unwrap();
        if let Some(old_id) = old_id {
            let WatchedRegistrationClientEvent::Unregister(unregister, acknowledge) =
                next_watched_registration_event(events).await
            else {
                panic!("expected old watched-file unregistration")
            };
            assert_eq!(watched_unregistration_id(&unregister), old_id);
            acknowledge.send(()).unwrap();
        }
        params
    }

    let project = TestProject::new();
    let old_root = project.path("/old");
    let new_root = project.path("/new");
    std::fs::create_dir(&old_root).unwrap();
    let file_uri = |path: &Path| lsp_types::Url::from_file_path(path).unwrap().to_string();

    let (events_tx, mut events_rx) = mpsc::unbounded_channel();
    let mut session = Session::spawn(new_router, move |_| watched_registration_client(events_tx));
    session.initialize(watched_initialize_params(&project, "/old")).await;

    let params = reregistration(&mut events_rx, None).await;
    assert_watched_registration_root(&params, &old_root);
    let old_registration_id = watched_registration_id(&params).to_owned();

    std::fs::rename(&old_root, &new_root).unwrap();
    let rename =
        json!({ "files": [{ "oldUri": file_uri(&old_root), "newUri": file_uri(&new_root) }] });
    session.server.notify::<notif::DidRenameFiles>(from_json(rename)).unwrap();
    let params = reregistration(&mut events_rx, Some(&old_registration_id)).await;
    assert_watched_registration_root(&params, &new_root);
    let new_registration_id = watched_registration_id(&params).to_owned();

    std::fs::remove_dir(&new_root).unwrap();
    let delete = json!({ "files": [{ "uri": file_uri(&new_root) }] });
    session.server.notify::<notif::DidDeleteFiles>(from_json(delete)).unwrap();
    let params = reregistration(&mut events_rx, Some(&new_registration_id)).await;
    assert!(watched_registration_watchers(&params).is_empty());

    session.shutdown().await;
}
