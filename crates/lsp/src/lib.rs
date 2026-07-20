#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/favicon.ico"
)]
#![cfg_attr(docsrs, feature(doc_cfg))]

use crate::global_state::GlobalState;
use async_lsp::{
    ClientSocket, client_monitor::ClientProcessMonitorLayer, concurrency::ConcurrencyLayer,
    router::Router, server::LifecycleLayer, tracing::TracingLayer,
};
use lsp_types::{notification as notif, request as req};
use serde_json as _;
use solar_config::LspArgs;
use std::ops::ControlFlow;
use tower::ServiceBuilder;

mod config;
mod diagnostics;
mod document_links;
mod flycheck;
mod formatter;
mod global_state;
mod handlers;
mod hover;
mod inlay_hints;
mod proto;
mod rename;
mod serde;
mod signature_help;
mod symbols;
mod utils;
mod vfs;
mod workspace;

#[cfg(test)]
mod test_support;

pub(crate) type NotifyResult = ControlFlow<async_lsp::Result<()>>;

fn new_router(client: ClientSocket) -> Router<GlobalState> {
    let this = GlobalState::new(client);
    let mut router = Router::new(this);

    // Lifecycle
    router
        .request::<req::Initialize, _>(GlobalState::on_initialize)
        .notification::<notif::Initialized>(GlobalState::on_initialized)
        .request::<req::Shutdown, _>(|_, _| std::future::ready(Ok(())))
        .notification::<notif::Exit>(|_, _| ControlFlow::Break(Ok(())));

    // Requests
    router
        .request::<req::DocumentSymbolRequest, _>(handlers::document_symbol)
        .request::<req::DocumentLinkRequest, _>(handlers::document_links)
        .request::<req::WorkspaceSymbolRequest, _>(handlers::workspace_symbol)
        .request::<req::GotoDefinition, _>(handlers::goto_definition)
        .request::<req::GotoTypeDefinition, _>(handlers::goto_type_definition)
        .request::<req::GotoDeclaration, _>(handlers::goto_declaration)
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
            // TODO: infer concurrency
            .layer(ConcurrencyLayer::new(2.try_into().unwrap()))
            .layer(ClientProcessMonitorLayer::new(client.clone()))
            .service(new_router(client))
    });

    eloop.run_buffered(stdin, stdout).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_lsp::{AnyNotification, AnyRequest, LanguageServer, LspService, router::Router};
    use lsp_types::{
        DidChangeWatchedFilesClientCapabilities, DidChangeWatchedFilesParams,
        DidSaveTextDocumentParams, DocumentFormattingParams, DocumentHighlightParams,
        DocumentLinkParams, FileChangeType, FileEvent, FormattingOptions, HoverParams,
        InitializeParams, InitializedParams, PartialResultParams, Position, SignatureHelpParams,
        TextDocumentIdentifier, TextDocumentPositionParams, WorkDoneProgressParams,
        WorkspaceClientCapabilities, notification as notif, notification::Notification, request,
        request::Request,
    };
    use std::ops::ControlFlow;
    use tokio::sync::oneshot;
    use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
    use tower::Service;

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
