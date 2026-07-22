#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/favicon.ico"
)]
#![cfg_attr(docsrs, feature(doc_cfg))]

use crate::global_state::GlobalState;
use async_lsp::{
    ClientSocket, client_monitor::ClientProcessMonitorLayer, router::Router,
    server::LifecycleLayer, tracing::TracingLayer,
};
#[cfg(test)]
use criterion as _;
use lsp_types::{notification as notif, request as req};
use serde_json as _;
use solar_config::LspArgs;
use std::ops::ControlFlow;
use tower::ServiceBuilder;

mod commands;
mod config;
mod diagnostics;
mod document_links;
mod flycheck;
mod formatter;
mod global_state;
mod handlers;
mod hover;
mod inlay_hints;
mod natspec_completion;
mod override_index;
#[cfg(any(test, feature = "bench"))]
#[cfg_attr(all(feature = "bench", not(test)), allow(dead_code))]
mod project_fixture;
mod proto;
mod rename;
mod request_cancellation;
mod serde;
mod signature_help;
mod symbols;
mod utils;
mod vfs;
mod workspace;

/// Benchmark-only access to prepared LSP projects and opaque analysis snapshots.
#[cfg(feature = "bench")]
#[doc(hidden)]
pub use global_state::benchmark::{
    BenchmarkAnalysis, BenchmarkDocumentChange, BenchmarkEdit, BenchmarkError, BenchmarkProject,
    BenchmarkRequest, BenchmarkResponse,
};

#[cfg(test)]
mod test_support;

pub(crate) type NotifyResult = ControlFlow<async_lsp::Result<()>>;

fn new_router(client: ClientSocket) -> Router<GlobalState> {
    new_router_with_state(GlobalState::new(client))
}

fn new_router_with_state(this: GlobalState) -> Router<GlobalState> {
    let mut router = Router::new(this);

    // Lifecycle
    router
        .request::<req::Initialize, _>(GlobalState::on_initialize)
        .notification::<notif::Initialized>(GlobalState::on_initialized)
        .request::<req::Shutdown, _>(|_, _| std::future::ready(Ok(())))
        .notification::<notif::Exit>(|_, _| ControlFlow::Break(Ok(())));

    // Requests
    router
        .request::<req::ExecuteCommand, _>(commands::execute_command)
        .request::<req::DocumentSymbolRequest, _>(handlers::document_symbol)
        .request::<req::DocumentLinkRequest, _>(handlers::document_links)
        .request::<req::WorkspaceSymbolRequest, _>(handlers::workspace_symbol)
        .request::<req::GotoDefinition, _>(handlers::goto_definition)
        .request::<req::GotoTypeDefinition, _>(handlers::goto_type_definition)
        .request::<req::GotoDeclaration, _>(handlers::goto_declaration)
        .request::<req::GotoImplementation, _>(handlers::goto_implementation)
        .request::<req::References, _>(handlers::references)
        .request::<req::DocumentHighlightRequest, _>(handlers::document_highlight)
        .request::<req::HoverRequest, _>(handlers::hover)
        .request::<req::PrepareRenameRequest, _>(handlers::prepare_rename)
        .request::<req::Rename, _>(handlers::rename)
        .request::<req::SignatureHelpRequest, _>(handlers::signature_help)
        .request::<req::InlayHintRequest, _>(handlers::inlay_hints)
        .request::<req::Completion, _>(handlers::completion)
        .request::<req::Formatting, _>(handlers::formatting);

    // Workspace management
    router
        .notification::<notif::DidChangeWorkspaceFolders>(handlers::did_change_workspace_folders)
        .notification::<notif::DidChangeWatchedFiles>(handlers::did_change_watched_files);

    // Notifications
    router
        .notification::<notif::DidOpenTextDocument>(handlers::did_open_text_document)
        .notification::<notif::DidCloseTextDocument>(handlers::did_close_text_document)
        .notification::<notif::DidChangeTextDocument>(handlers::did_change_text_document)
        .notification::<notif::DidSaveTextDocument>(handlers::did_save_text_document)
        .notification::<notif::DidChangeConfiguration>(handlers::did_change_configuration);

    router
}

fn request_layer() -> request_cancellation::RequestCancellationLayer {
    request_cancellation::RequestCancellationLayer
}

/// Start the LSP server over stdin/stdout.
///
/// This future is long running and will not stop until the server exits.
pub async fn run_server_stdio(_args: LspArgs) -> async_lsp::Result<()> {
    // Prefer truly asynchronous piped stdin/stdout without blocking tasks.
    #[cfg(unix)]
    let (stdin, stdout) =
        (async_lsp::stdio::PipeStdin::lock_tokio()?, async_lsp::stdio::PipeStdout::lock_tokio()?);

    // Fallback to spawn blocking read/write otherwise.
    #[cfg(not(unix))]
    let (stdin, stdout) = (
        tokio_util::compat::TokioAsyncReadCompatExt::compat(tokio::io::stdin()),
        tokio_util::compat::TokioAsyncWriteCompatExt::compat_write(tokio::io::stdout()),
    );

    let (eloop, _) = async_lsp::MainLoop::new_server(|client| {
        ServiceBuilder::new()
            .layer(TracingLayer::default())
            .layer(LifecycleLayer::default())
            .layer(request_layer())
            .layer(ClientProcessMonitorLayer::new(client.clone()))
            .service(new_router(client))
    });

    eloop.run_buffered(stdin, stdout).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestProject;
    use async_lsp::{
        AnyEvent, AnyNotification, AnyRequest, LanguageServer, LspService, ResponseError,
        router::Router,
    };
    use lsp_types::{
        CancelParams, CompletionParams, CompletionResponse,
        DidChangeWatchedFilesClientCapabilities, DidChangeWatchedFilesParams,
        DidSaveTextDocumentParams, DocumentFormattingParams, DocumentHighlightParams,
        DocumentLinkParams, DocumentSymbolParams, ExecuteCommandParams, FileChangeType, FileEvent,
        FormattingOptions, HoverParams, InitializeParams, InitializedParams, NumberOrString,
        PartialResultParams, Position, SignatureHelpParams, TextDocumentIdentifier,
        TextDocumentPositionParams, WorkDoneProgressParams, WorkspaceClientCapabilities,
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
    use tokio::sync::{mpsc, oneshot};
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
            let router =
                ObservedRouter { inner: new_router_with_state(state), accepted: accepted_tx };
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

        let registrations =
            tokio::time::timeout(std::time::Duration::from_secs(1), registration_rx)
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
}
